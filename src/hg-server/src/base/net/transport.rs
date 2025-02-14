use std::{
    net::SocketAddr,
    num::NonZeroU64,
    pin::pin,
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use bytes::Bytes;
use futures::StreamExt as _;
use hg_common::{
    base::net::{back_pressure::ErasedTaskGuard, codec::FrameDecoder, protocol::SocketCloseReason},
    utils::lang::{
        absorb_result_std, catch_termination_async, worker_panic_error, FusedFuture, MultiError,
        MultiResult, PANIC_ERR_MSG,
    },
};
use hg_utils::hash::FxHashMap;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::codec::FramedRead;

// === Tidbits === //

#[derive(Debug, Clone, Error)]
#[error("peer disconnected")]
pub struct PeerDisconnectError;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RawPeerId(NonZeroU64);

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
    },
    Shutdown {
        cause: anyhow::Result<()>,
    },
}

#[derive(Debug)]
pub enum PeerSendAction {
    Reliable {
        data: Bytes,
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
    pub async fn new(config: quinn::ServerConfig, bind_addr: SocketAddr) -> anyhow::Result<Self> {
        let endpoint = quinn::Endpoint::server(config, bind_addr)
            .with_context(|| format!("failed to create endpoint on `{bind_addr}`"))?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let listen_state = Arc::new(TransportListenState {
            event_tx,
            peer_map: Mutex::default(),
        });

        let listen_worker = TransportListenWorker {
            listen_state: listen_state.clone(),
            next_peer_id: NonZeroU64::new(1).unwrap(),
        };

        tokio::spawn(listen_worker.run_listen(endpoint));

        Ok(Self {
            event_rx,
            listen_state,
        })
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

    pub fn peer_send_reliable(&self, id: RawPeerId, data: Bytes, task_guard: ErasedTaskGuard) {
        self.peer_send(id, PeerSendAction::Reliable { data, task_guard });
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
    async fn run_listen(self, endpoint: quinn::Endpoint) {
        let listen_state = self.listen_state.clone();

        catch_termination_async(self.run_listen_inner(endpoint), |cause| {
            let cause = cause.unwrap_or_else(|| Err(worker_panic_error()));

            if let Err(err) = &cause {
                tracing::error!("server listener thread crashed:\n{err:?}");
            }

            listen_state.send_event(TransportEvent::Shutdown { cause });
        })
        .await;
    }

    async fn run_listen_inner(mut self, endpoint: quinn::Endpoint) -> anyhow::Result<()> {
        tracing::info!("Listening on `{}`!", endpoint.local_addr().unwrap());

        while let Some(incoming) = endpoint.accept().await {
            let conn = incoming.accept()?.await?;
            let remote_addr = conn.remote_address();
            let peer_id = RawPeerId(self.next_peer_id);
            self.next_peer_id = self
                .next_peer_id
                .checked_add(1)
                .context("created too many peers")?;

            tracing::info!("Got connection from {remote_addr}, {peer_id:?}");

            let (send_action_tx, framed_send_rx) = mpsc::unbounded_channel();

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

            tokio::spawn(peer_worker.run_conn(framed_send_rx));
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
    async fn run_conn(self, send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>) {
        // Add the peer to the peer map
        self.listen_state
            .peer_map
            .lock()
            .unwrap()
            .insert(self.peer_state.peer_id, self.peer_state.clone());

        // Handle connections
        catch_termination_async(self.clone().run_conn_inner(send_action_rx), |cause| {
            let cause = cause.unwrap_or_else(|| Err(worker_panic_error().into()));

            if let Err(error) = &cause {
                tracing::error!(
                    "Socket handler thread for {:?} ({}) crashed:\n{error:?}",
                    self.peer_state.peer_id,
                    self.peer_state.remote_addr,
                );
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
        let (tx, rx) = self.conn.accept_bi().await.map_err(MultiError::new)?;

        // Spawn two threads to handle the read and write sides of this connection separately.
        let tx_thread = pin!(tokio::spawn(self.clone().run_conn_rx(rx)));
        let rx_thread = pin!(tokio::spawn(self.clone().run_conn_tx(tx, send_action_rx)));

        let mut tx_thread = FusedFuture::new(tx_thread);
        let mut rx_thread = FusedFuture::new(rx_thread);

        // Find the side which terminates first.
        let first = tokio::select! {
            first = tx_thread.wait() => first.unwrap(),
            first = rx_thread.wait() => first.unwrap(),
        };

        // Ensure that the other side also terminates
        let first = flatten_join_result(first);

        if first.is_err() {
            // If `res` was not erroneous, we know the first thread to finish must have encountered
            // a socket EOF, which occurs on both sides of the connection. Hence, there is no need
            // to do anything to stop the other thread.

            // If it was erroneous, we need to close the socket ourselves.
            self.conn.close(SocketCloseReason::Crash.code().into(), &[]);
        }

        // Ensure that the other side terminates before cleaning up the thread.
        let (lhs, rhs) = tokio::join!(tx_thread.wait(), rx_thread.wait());
        let second = flatten_join_result(lhs.or(rhs).unwrap());

        MultiError::from_iter([first, second])
    }

    async fn run_conn_rx(self, rx: quinn::RecvStream) -> anyhow::Result<()> {
        let mut rx = pin!(FramedRead::new(
            rx,
            FrameDecoder {
                max_packet_size: 1024,
            },
        ));

        while let Some(packet) = rx.next().await {
            let packet = packet?;

            self.listen_state.send_event(TransportEvent::DataReceived {
                peer: self.peer_state.peer_id,
                packet,
            });
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
                    // this connection and that object isn't destroyed until this thread exists.
                    send_action.unwrap()
                },
                err = self.conn.closed() => {
                    if let quinn::ConnectionError::LocallyClosed = err {
                        return Ok(());
                    }

                    return Err(err.into());
                }
            };

            // Process it!
            match send_action {
                PeerSendAction::Reliable { data, task_guard } => {
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

fn flatten_join_result<T>(
    res: Result<anyhow::Result<T>, tokio::task::JoinError>,
) -> anyhow::Result<T> {
    match res {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(anyhow::Error::new(err).context(PANIC_ERR_MSG)),
    }
}
