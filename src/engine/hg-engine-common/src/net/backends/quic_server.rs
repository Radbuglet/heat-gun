use std::{
    net::SocketAddr,
    num::NonZeroU64,
    pin::pin,
    sync::{
        atomic::{AtomicBool, Ordering::*},
        Arc, Mutex,
    },
};

use anyhow::Context as _;
use bytes::Bytes;
use futures::StreamExt as _;
use hg_utils::hash::FxHashMap;
use tokio::sync::mpsc;
use tokio_util::codec::FramedRead;
use tracing::{instrument, Instrument};

use crate::{
    net::{
        back_pressure::{BackPressureAsync, ErasedTaskGuard},
        codec::FrameDecoder,
        transport::{PeerDisconnectError, PeerId, ServerTransport, ServerTransportEvent},
    },
    utils::lang::{
        absorb_result_anyhow, absorb_result_std, catch_termination_async, worker_panic_error,
        MultiError, MultiResult,
    },
};

use super::quic_shared::{
    filter_framed_read_failure, run_transport_data_handler, SocketCloseReason,
};

// === Transport === //

#[derive(Debug)]
pub struct QuicServerTransport {
    listen_state: Arc<TransportListenState>,
    event_rx: mpsc::UnboundedReceiver<ServerTransportEvent>,
}

#[derive(Debug)]
enum PeerSendAction {
    Reliable {
        framed: Bytes,
        task_guard: ErasedTaskGuard,
    },
    Disconnect(Bytes),
}

#[derive(Debug)]
struct TransportListenState {
    event_tx: mpsc::UnboundedSender<ServerTransportEvent>,
    peer_map: Mutex<FxHashMap<PeerId, Arc<TransportPeerState>>>,
}

#[derive(Debug)]
struct TransportPeerState {
    peer_id: PeerId,
    remote_addr: SocketAddr,
    send_action_tx: mpsc::UnboundedSender<PeerSendAction>,
    kicked: AtomicBool,
}

impl QuicServerTransport {
    pub fn new(config: quinn::ServerConfig, bind_addr: SocketAddr) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let listen_state = Arc::new(TransportListenState {
            event_tx,
            peer_map: Mutex::default(),
        });

        let listen_worker = TransportListenWorker {
            listen_state: listen_state.clone(),
            next_peer_id: NonZeroU64::new(1).unwrap(),
        };

        tokio::spawn(listen_worker.run_listen(config, bind_addr));

        Self {
            listen_state,
            event_rx,
        }
    }

    fn peer(&self, id: PeerId) -> Result<Arc<TransportPeerState>, PeerDisconnectError> {
        self.listen_state
            .peer_map
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .filter(|v| !v.kicked.load(Relaxed))
            .ok_or(PeerDisconnectError)
    }
}

impl ServerTransport for QuicServerTransport {
    fn process(&mut self) -> Option<ServerTransportEvent> {
        while let Some(ev) = self.event_rx.try_recv().ok() {
            if matches!(
                ev,
                ServerTransportEvent::DataReceived { peer, .. }
                    if !self.peer_alive(peer),
            ) {
                // (drop incoming packet from kicked peer)
                continue;
            }

            return Some(ev);
        }

        None
    }

    fn peer_remote_addr(&mut self, id: PeerId) -> Result<SocketAddr, PeerDisconnectError> {
        self.peer(id).map(|peer| peer.remote_addr)
    }

    fn peer_alive(&mut self, id: PeerId) -> bool {
        self.peer(id).is_ok()
    }

    fn peer_send(&mut self, id: PeerId, framed: Bytes, task_guard: ErasedTaskGuard) {
        absorb_result_std::<_, PeerDisconnectError>("send a packet", || {
            self.peer(id)?
                .send_action_tx
                .send(PeerSendAction::Reliable { framed, task_guard })
                .map_err(|_| PeerDisconnectError)?;

            Ok(())
        });
    }

    fn peer_kick(&mut self, id: PeerId, data: Bytes) {
        absorb_result_anyhow("kick a peer", || {
            let peer = self.peer(id)?;

            if peer.kicked.swap(true, Relaxed) {
                anyhow::bail!("cannot kick a peer more than once");
            }

            tracing::info!("Kicked peer {id}");

            peer.send_action_tx
                .send(PeerSendAction::Disconnect(data))
                .map_err(|_| PeerDisconnectError)?;

            Ok(())
        });
    }
}

impl TransportListenState {
    fn send_event(&self, event: ServerTransportEvent) {
        let _ = self.event_tx.send(event);
    }
}

// === Workers === //

#[derive(Debug)]
struct TransportListenWorker {
    listen_state: Arc<TransportListenState>,
    next_peer_id: NonZeroU64,
}

impl TransportListenWorker {
    #[instrument(skip_all, name = "listen worker")]
    async fn run_listen(self, config: quinn::ServerConfig, bind_addr: SocketAddr) {
        let listen_state = self.listen_state.clone();

        catch_termination_async(self.run_listen_inner(config, bind_addr), |cause| {
            let cause = cause.unwrap_or_else(|| Err(worker_panic_error()));

            if let Err(err) = &cause {
                tracing::error!("server listener task crashed:\n{err:?}");
            }

            listen_state.send_event(ServerTransportEvent::Shutdown { cause });
        })
        .await;
    }

    async fn run_listen_inner(
        mut self,
        config: quinn::ServerConfig,
        bind_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let endpoint = quinn::Endpoint::server(config, bind_addr)
            .with_context(|| format!("failed to create endpoint on `{bind_addr}`"))?;

        tracing::info!("Listening on `{}`!", endpoint.local_addr().unwrap());

        let mut listen_pressure = BackPressureAsync::new(64);

        while let Some(incoming) = endpoint.accept().await {
            let conn = incoming.accept()?.await?;
            let remote_addr = conn.remote_address();
            let peer_id = PeerId(self.next_peer_id);
            self.next_peer_id = self
                .next_peer_id
                .checked_add(1)
                .context("created too many peers")?;

            let accept_task = listen_pressure.start(1);

            let (send_action_tx, send_action_rx) = mpsc::unbounded_channel();

            let peer_state = Arc::new(TransportPeerState {
                peer_id,
                remote_addr,
                send_action_tx,
                kicked: AtomicBool::new(false),
            });

            let peer_worker = TransportPeerWorker {
                listen_state: self.listen_state.clone(),
                peer_state,
                conn,
            };

            tokio::spawn(peer_worker.run_conn(ErasedTaskGuard::new(accept_task), send_action_rx));

            listen_pressure.wait().await;
        }

        // (only reachable if `endpoint` is manually closed)

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TransportPeerWorker {
    listen_state: Arc<TransportListenState>,
    peer_state: Arc<TransportPeerState>,
    conn: quinn::Connection,
}

impl TransportPeerWorker {
    #[instrument(skip_all, name = "peer worker", fields(peer = %self.peer_state.peer_id))]
    async fn run_conn(
        self,
        accept_task: ErasedTaskGuard,
        send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>,
    ) {
        tracing::info!("Got connection from {}", self.peer_state.remote_addr);

        // Add the peer to the peer map
        self.listen_state
            .peer_map
            .lock()
            .unwrap()
            .insert(self.peer_state.peer_id, self.peer_state.clone());

        // Handle connections
        catch_termination_async(
            self.clone().run_conn_inner(accept_task, send_action_rx),
            |cause| {
                let cause = cause.unwrap_or_else(|| Err(worker_panic_error().into()));

                match &cause {
                    Ok(()) => tracing::info!("Peer disconnected."),
                    Err(error) => tracing::error!("Socket handler crashed:\n{error:?}"),
                }

                self.listen_state
                    .peer_map
                    .lock()
                    .unwrap()
                    .remove(&self.peer_state.peer_id);

                self.listen_state
                    .send_event(ServerTransportEvent::Disconnected {
                        peer: self.peer_state.peer_id,
                        cause: cause.map_err(Into::into),
                    });
            },
        )
        .await;
    }

    async fn run_conn_inner(
        self,
        accept_task: ErasedTaskGuard,
        send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>,
    ) -> MultiResult<()> {
        // Send connection event.
        self.listen_state
            .send_event(ServerTransportEvent::Connected {
                peer: self.peer_state.peer_id,
                task: accept_task,
            });

        // We ask the user to send the initial packet.
        let (tx, rx) = self
            .conn
            .accept_bi()
            .await
            .map_err(MultiError::new)
            .context("failed to open main stream")?;

        tracing::info!("Received initial packet!");

        // Process the stream!
        run_transport_data_handler(
            self.conn.clone(),
            tokio::spawn(self.clone().run_conn_rx(rx).in_current_span()),
            tokio::spawn(
                self.clone()
                    .run_conn_tx(tx, send_action_rx)
                    .in_current_span(),
            ),
        )
        .await?;

        Ok(())
    }

    async fn run_conn_rx(self, rx: quinn::RecvStream) -> anyhow::Result<()> {
        let mut pressure = BackPressureAsync::new(1024);
        let mut rx = pin!(FramedRead::new(
            rx,
            FrameDecoder {
                max_packet_size: 1024,
            },
        ));

        while let Some(packet) = rx.next().await {
            let packet = match packet {
                Ok(v) => v,
                Err(e) => return filter_framed_read_failure(e),
            };

            let task = ErasedTaskGuard::new(pressure.start(packet.len()));

            self.listen_state
                .send_event(ServerTransportEvent::DataReceived {
                    peer: self.peer_state.peer_id,
                    packet,
                    task,
                });

            tokio::select! {
                _ = pressure.wait() => {
                    // (fallthrough)
                }
                _err = self.conn.closed() => {
                    // The `run_conn_inner` driver will interpret the `close_reason()` for us. We
                    // should only return `Err(())` if some novel kind of error occurs.
                    return Ok(());
                }
            };
        }

        Ok(())
    }

    async fn run_conn_tx(
        self,
        mut tx: quinn::SendStream,
        mut send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>,
    ) -> anyhow::Result<()> {
        loop {
            // Wait for the next send request.
            let send_action = tokio::select! {
                send_action = send_action_rx.recv() => {
                    // Unwrap OK: The sender is stored in the `TransportPeerState` associated with
                    // this connection and that object isn't destroyed until this task exists.
                    send_action.unwrap()
                },
                _err = self.conn.closed() => {
                    // The `run_conn_inner` driver will interpret the `close_reason()` for us. We
                    // should only return `Err(())` if some novel kind of error occurs.
                    return Ok(());
                }
            };

            // Process it!
            match send_action {
                PeerSendAction::Reliable { framed, task_guard } => {
                    // TODO: parse error
                    tx.write_all(&framed).await?;
                    drop(task_guard);
                }
                PeerSendAction::Disconnect(bytes) => {
                    self.conn
                        .close(SocketCloseReason::Application.code().into(), &bytes);

                    break;
                }
            }
        }

        Ok(())
    }
}
