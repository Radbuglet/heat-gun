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
use hg_common::{
    base::net::{
        back_pressure::{BackPressureAsync, ErasedTaskGuard},
        codec::FrameDecoder,
        protocol::SocketCloseReason,
        transport::{filter_framed_read_failure, run_transport_data_handler, PeerId},
    },
    utils::lang::{
        absorb_result_anyhow, absorb_result_std, catch_termination_async, worker_panic_error,
        MultiError, MultiResult,
    },
};
use hg_utils::hash::FxHashMap;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::codec::FramedRead;
use tracing::{instrument, Instrument};

// === Tidbits === //

#[derive(Debug, Clone, Error)]
#[error("peer disconnected")]
pub struct PeerDisconnectError;

#[derive(Debug)]
pub enum TransportEvent {
    Connected {
        peer: PeerId,
        task: ErasedTaskGuard,
    },
    Disconnected {
        peer: PeerId,
        cause: anyhow::Result<()>,
    },
    DataReceived {
        peer: PeerId,
        packet: Bytes,
        task: ErasedTaskGuard,
    },
    Shutdown {
        cause: anyhow::Result<()>,
    },
}

#[derive(Debug)]
enum PeerSendAction {
    Reliable {
        pre_framed: Bytes,
        task_guard: ErasedTaskGuard,
    },
    Disconnect(Bytes),
}

// === Transport === //

#[derive(Debug)]
pub struct Transport {
    write_handle: TransportWriteHandle,
    event_rx: mpsc::UnboundedReceiver<TransportEvent>,
}

#[derive(Debug)]
struct TransportListenState {
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    peer_map: Mutex<FxHashMap<PeerId, Arc<TransportPeerState>>>,
}

#[derive(Debug)]
struct TransportPeerState {
    peer_id: PeerId,
    remote_addr: SocketAddr,
    send_action_tx: mpsc::UnboundedSender<PeerSendAction>,
    kicked: AtomicBool,
}

impl Transport {
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
            write_handle: TransportWriteHandle { listen_state },
            event_rx,
        }
    }

    pub fn process(&mut self) -> Option<TransportEvent> {
        while let Some(ev) = self.event_rx.try_recv().ok() {
            if matches!(
                ev,
                TransportEvent::DataReceived { peer, .. }
                    if !self.write_handle_ref().peer_alive(peer),
            ) {
                // (drop incoming packet from kicked peer)
                continue;
            }

            return Some(ev);
        }

        None
    }

    pub fn write_handle_ref(&self) -> &TransportWriteHandle {
        &self.write_handle
    }

    pub fn write_handle(&self) -> TransportWriteHandle {
        self.write_handle.clone()
    }
}

impl TransportListenState {
    fn send_event(&self, event: TransportEvent) {
        let _ = self.event_tx.send(event);
    }
}

#[derive(Debug, Clone)]
pub struct TransportWriteHandle {
    listen_state: Arc<TransportListenState>,
}

impl TransportWriteHandle {
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

    pub fn peer_remote_addr(&self, id: PeerId) -> Result<SocketAddr, PeerDisconnectError> {
        self.peer(id).map(|peer| peer.remote_addr)
    }

    pub fn peer_alive(&self, id: PeerId) -> bool {
        self.peer(id).is_ok()
    }

    pub fn peer_send(&self, id: PeerId, pre_framed: Bytes, task_guard: ErasedTaskGuard) {
        absorb_result_std::<_, PeerDisconnectError>("send a packet", || {
            self.peer(id)?
                .send_action_tx
                .send(PeerSendAction::Reliable {
                    pre_framed,
                    task_guard,
                })
                .map_err(|_| PeerDisconnectError)?;

            Ok(())
        });
    }

    pub fn peer_kick(&self, id: PeerId, data: Bytes) {
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

            listen_state.send_event(TransportEvent::Shutdown { cause });
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

                self.listen_state.send_event(TransportEvent::Disconnected {
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
        self.listen_state.send_event(TransportEvent::Connected {
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

            self.listen_state.send_event(TransportEvent::DataReceived {
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
                PeerSendAction::Reliable {
                    pre_framed: data,
                    task_guard,
                } => {
                    // TODO: parse error
                    tx.write_all(&data).await?;
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
