use std::{
    fmt,
    net::SocketAddr,
    num::NonZeroU64,
    pin::pin,
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use bytes::Bytes;
use futures::{FutureExt, StreamExt as _};
use hg_common::{
    base::net::{
        back_pressure::{BackPressureAsync, ErasedTaskGuard},
        codec::FrameDecoder,
        protocol::SocketCloseReason,
        transport::filter_framed_read_failure,
    },
    utils::lang::{
        absorb_result_std, catch_termination_async, flatten_tokio_join_result, worker_panic_error,
        FusedFuture, MultiError, MultiResult,
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

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RawPeerId(NonZeroU64);

impl fmt::Display for RawPeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
pub enum TransportEvent {
    Connected {
        peer: RawPeerId,
    },
    Disconnected {
        peer: RawPeerId,
        cause: anyhow::Result<()>,
    },
    DataReceived {
        peer: RawPeerId,
        packet: Bytes,
        task: ErasedTaskGuard,
    },
    Shutdown {
        cause: anyhow::Result<()>,
    },
}

#[derive(Debug)]
pub enum PeerSendAction {
    Reliable {
        pre_framed: Bytes,
        task_guard: ErasedTaskGuard,
    },
    Disconnect(Bytes),
}

// === Transport === //

#[derive(Debug)]
pub struct Transport {
    event_rx: mpsc::UnboundedReceiver<TransportEvent>,
    listen_state: Arc<TransportListenState>,
}

#[derive(Debug)]
struct TransportListenState {
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    peer_map: Mutex<FxHashMap<RawPeerId, Arc<TransportPeerState>>>,
}

#[derive(Debug)]
struct TransportPeerState {
    peer_id: RawPeerId,
    remote_addr: SocketAddr,
    send_action_tx: mpsc::UnboundedSender<PeerSendAction>,
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
            event_rx,
            listen_state,
        }
    }

    fn peer(&self, id: RawPeerId) -> Result<Arc<TransportPeerState>, PeerDisconnectError> {
        self.listen_state
            .peer_map
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(PeerDisconnectError)
    }

    pub fn peer_remote_addr(&self, id: RawPeerId) -> Result<SocketAddr, PeerDisconnectError> {
        self.peer(id).map(|peer| peer.remote_addr)
    }

    pub fn peer_send(&self, id: RawPeerId, action: PeerSendAction) {
        absorb_result_std::<_, PeerDisconnectError>("send a packet", || {
            self.peer(id)?
                .send_action_tx
                .send(action)
                .map_err(|_| PeerDisconnectError)?;

            Ok(())
        });
    }

    pub fn peer_send_reliable(
        &self,
        id: RawPeerId,
        pre_framed: Bytes,
        task_guard: ErasedTaskGuard,
    ) {
        self.peer_send(
            id,
            PeerSendAction::Reliable {
                pre_framed,
                task_guard,
            },
        );
    }

    pub fn peer_disconnect(&self, id: RawPeerId, data: Bytes) {
        self.peer_send(id, PeerSendAction::Disconnect(data));
    }

    pub fn process_non_blocking(&mut self) -> Option<TransportEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn process_blocking(&mut self) -> Option<TransportEvent> {
        self.event_rx.blocking_recv()
    }

    pub async fn process_async(&mut self) -> Option<TransportEvent> {
        self.event_rx.recv().await
    }
}

impl TransportListenState {
    fn send_event(&self, event: TransportEvent) {
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

        while let Some(incoming) = endpoint.accept().await {
            let conn = incoming.accept()?.await?;
            let remote_addr = conn.remote_address();
            let peer_id = RawPeerId(self.next_peer_id);
            self.next_peer_id = self
                .next_peer_id
                .checked_add(1)
                .context("created too many peers")?;

            let (send_action_tx, send_action_rx) = mpsc::unbounded_channel();

            let peer_state = Arc::new(TransportPeerState {
                peer_id,
                remote_addr,
                send_action_tx,
            });

            let peer_worker = TransportPeerWorker {
                listen_state: self.listen_state.clone(),
                peer_state,
                conn,
            };

            tokio::spawn(peer_worker.run_conn(send_action_rx));
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
    async fn run_conn(self, send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>) {
        tracing::info!("Got connection from {}", self.peer_state.remote_addr);

        // Add the peer to the peer map
        self.listen_state
            .peer_map
            .lock()
            .unwrap()
            .insert(self.peer_state.peer_id, self.peer_state.clone());

        // Handle connections
        catch_termination_async(self.clone().run_conn_inner(send_action_rx), |cause| {
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
        })
        .await;
    }

    async fn run_conn_inner(
        self,
        send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>,
    ) -> MultiResult<()> {
        // Send connection event.
        self.listen_state.send_event(TransportEvent::Connected {
            peer: self.peer_state.peer_id,
        });

        // We ask the user to send the initial packet.
        let (tx, rx) = self
            .conn
            .accept_bi()
            .await
            .map_err(MultiError::new)
            .context("failed to open main stream")?;

        // Spawn two tasks to handle the read and write sides of this connection separately.
        let rx_task = self.clone().run_conn_rx(rx).in_current_span();
        let rx_task = pin!(tokio::spawn(rx_task)
            .map(|v| flatten_tokio_join_result(v).context("receiver task crashed")));

        let tx_task = self
            .clone()
            .run_conn_tx(tx, send_action_rx)
            .in_current_span();

        let tx_task = pin!(tokio::spawn(tx_task)
            .map(|v| flatten_tokio_join_result(v).context("transmission task crashed")));

        let mut rx_task = FusedFuture::new(rx_task);
        let mut tx_task = FusedFuture::new(tx_task);

        // Find the side which terminates first.
        let first = tokio::select! {
            first = rx_task.wait() => first.unwrap(),
            first = tx_task.wait() => first.unwrap(),
        };

        // Ensure that the other side also terminates
        if first.is_err() {
            // If `res` was not erroneous, we know the first task to finish must have encountered
            // a socket EOF, which occurs on both sides of the connection. Hence, there is no need
            // to do anything to stop the other task.

            // If it was erroneous, we need to close the socket ourselves.
            self.conn.close(SocketCloseReason::Crash.code().into(), &[]);
        }

        // Ensure that the other side terminates before cleaning up the task.
        let (lhs, rhs) = tokio::join!(rx_task.wait(), tx_task.wait());
        let second = lhs.or(rhs).unwrap();

        // Parse the connection error.
        let third = {
            use quinn::ConnectionError::*;

            let err = self.conn.close_reason().unwrap();
            #[rustfmt::skip]
            let is_err = match err {
                VersionMismatch
                | TransportError(_)
                | ConnectionClosed(_)
                | Reset
                | TimedOut
                | CidsExhausted => true,
                ApplicationClosed(_) | LocallyClosed => false,
            };

            if is_err {
                Err(anyhow::Error::new(err).context("error ocurred in connection"))
            } else {
                Ok(())
            }
        };

        MultiError::from_iter([first, second, third])
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
