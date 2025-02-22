use std::{net::SocketAddr, pin::pin, str::FromStr, sync::Arc};

use anyhow::Context as _;
use bytes::Bytes;
use futures::StreamExt as _;
use hg_common::{
    base::net::{
        back_pressure::{BackPressureAsync, ErasedTaskGuard},
        codec::FrameDecoder,
        protocol::SocketCloseReason,
        transport::{filter_framed_read_failure, run_transport_data_handler},
    },
    try_async,
    utils::lang::{absorb_result_std, catch_termination_async, worker_panic_error},
};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::codec::FramedRead;
use tracing::{instrument, Instrument as _};

// === Tidbits === //

#[derive(Debug, Clone, Error)]
#[error("peer disconnected")]
pub struct ShutdownError;

#[derive(Debug)]
pub enum TransportEvent {
    Connected,
    Disconnected {
        cause: anyhow::Result<()>,
    },
    DataReceived {
        packet: Bytes,
        task: ErasedTaskGuard,
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
    state: Arc<TransportState>,
    event_rx: mpsc::UnboundedReceiver<TransportEvent>,
}

#[derive(Debug)]
struct TransportState {
    server_addr: SocketAddr,
    server_name: String,
    config: quinn::ClientConfig,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    send_action_tx: mpsc::UnboundedSender<PeerSendAction>,
}

impl Transport {
    pub fn new(config: quinn::ClientConfig, server_addr: SocketAddr, server_name: &str) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (send_action_tx, send_action_rx) = mpsc::unbounded_channel();

        let state = Arc::new(TransportState {
            server_addr,
            server_name: server_name.to_owned(),
            config,
            event_tx,
            send_action_tx,
        });

        tokio::spawn(TransportWorker::run(state.clone(), send_action_rx));

        Self { state, event_rx }
    }

    pub fn send(&self, action: PeerSendAction) {
        absorb_result_std::<_, _>("send a packet", || self.state.send_action_tx.send(action));
    }

    pub fn send_reliable(&self, pre_framed: Bytes, task_guard: ErasedTaskGuard) {
        self.send(PeerSendAction::Reliable {
            pre_framed,
            task_guard,
        });
    }

    pub fn disconnect(&self, data: Bytes) {
        self.send(PeerSendAction::Disconnect(data));
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

impl TransportState {
    fn send_event(&self, event: TransportEvent) {
        let _ = self.event_tx.send(event);
    }
}

// === TransportWorker === //

#[derive(Debug)]
struct TransportWorker {
    state: Arc<TransportState>,
    conn: quinn::Connection,
}

impl TransportWorker {
    #[instrument(skip_all, name = "peer worker")]
    async fn run(
        state: Arc<TransportState>,
        send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>,
    ) {
        catch_termination_async(Self::run_inner(state.clone(), send_action_rx), |cause| {
            let cause = cause.unwrap_or_else(|| Err(worker_panic_error()));

            if let Err(err) = &cause {
                tracing::error!("client listener thread crashed:\n{err:?}");
            }

            state.send_event(TransportEvent::Disconnected { cause });
        })
        .await;
    }

    async fn run_inner(
        state: Arc<TransportState>,
        send_action_rx: mpsc::UnboundedReceiver<PeerSendAction>,
    ) -> anyhow::Result<()> {
        tracing::info!("Connecting to {:?}...", state.server_addr);

        // Create endpoint
        let mut endpoint = quinn::Endpoint::client(SocketAddr::from_str("[::]:0").unwrap())?;
        endpoint.set_default_client_config(state.config.clone());

        // Connect to peer
        let conn = try_async! {
            endpoint
                .connect(state.server_addr, &state.server_name)?
                .await?
        }
        .with_context(|| format!("failed to connect to {}", state.server_addr))?;

        tracing::info!("Connected!");

        state.send_event(TransportEvent::Connected);

        let worker = Arc::new(TransportWorker { state, conn });

        // Open the main stream
        let (tx, rx) = worker
            .conn
            .open_bi()
            .await
            .context("failed to open main stream")?;

        // Process the stream!
        run_transport_data_handler(
            worker.conn.clone(),
            tokio::spawn(worker.clone().run_conn_rx(rx).in_current_span()),
            tokio::spawn(
                worker
                    .clone()
                    .run_conn_tx(tx, send_action_rx)
                    .in_current_span(),
            ),
        )
        .await?;

        Ok(())
    }

    async fn run_conn_rx(self: Arc<Self>, rx: quinn::RecvStream) -> anyhow::Result<()> {
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

            self.state
                .send_event(TransportEvent::DataReceived { packet, task });

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
        self: Arc<Self>,
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
