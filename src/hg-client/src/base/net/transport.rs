use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context as _;
use bytes::Bytes;
use hg_common::{
    base::net::back_pressure::ErasedTaskGuard,
    try_async,
    utils::lang::{catch_termination_async, worker_panic_error},
};
use thiserror::Error;
use tokio::sync::mpsc;

// === Tidbits === //

#[derive(Debug, Clone, Error)]
#[error("peer disconnected")]
pub struct PeerDisconnectError;

#[derive(Debug)]
pub enum TransportEvent {
    Connected,
    Disconnected { cause: anyhow::Result<()> },
    DataReceived { packet: Bytes },
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
    state: Arc<TransportState>,
    event_rx: mpsc::UnboundedReceiver<TransportEvent>,
}

#[derive(Debug)]
struct TransportState {
    server_addr: SocketAddr,
    server_name: String,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
}

impl Transport {
    pub fn new(config: quinn::ClientConfig, server_addr: SocketAddr, server_name: &str) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let state = Arc::new(TransportState {
            server_addr,
            server_name: server_name.to_owned(),
            event_tx,
        });

        let worker = TransportWorker {
            state: state.clone(),
        };

        tokio::spawn(worker.run(config));

        Self { state, event_rx }
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
}

impl TransportWorker {
    async fn run(self, config: quinn::ClientConfig) {
        let state = self.state.clone();

        catch_termination_async(self.run_inner(config), |cause| {
            let cause = cause.unwrap_or_else(|| Err(worker_panic_error()));

            if let Err(err) = &cause {
                tracing::error!("server listener thread crashed:\n{err:?}");
            }

            state.send_event(TransportEvent::Disconnected { cause });
        })
        .await;
    }

    async fn run_inner(self, config: quinn::ClientConfig) -> anyhow::Result<()> {
        tracing::info!("Whee!");

        let mut endpoint = quinn::Endpoint::client(SocketAddr::from_str("[::]:0").unwrap())?;
        endpoint.set_default_client_config(config);

        let conn = try_async! {
            endpoint
                .connect(self.state.server_addr, &self.state.server_name)?
                .await?
        }
        .with_context(|| format!("failed to connect to {}", self.state.server_addr))?;

        tracing::info!("Woo!");

        let (mut tx, rx) = conn.open_bi().await?;

        tx.write_all(b"hello!").await?;

        tokio::time::sleep(Duration::from_millis(1000)).await;

        Ok(())
    }
}
