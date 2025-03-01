use std::{fmt, net::SocketAddr, num::NonZeroU64};

use bytes::Bytes;
use thiserror::Error;

use super::back_pressure::ErasedTaskGuard;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PeerId(pub NonZeroU64);

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Error)]
#[error("peer disconnected")]
pub struct ShutdownError;

#[derive(Debug, Clone, Error)]
#[error("peer disconnected")]
pub struct PeerDisconnectError;

#[derive(Debug)]
pub enum ClientTransportEvent {
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
pub enum ServerTransportEvent {
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

pub trait ClientTransport: fmt::Debug {
    fn process(&mut self) -> Option<ClientTransportEvent>;

    fn send(&mut self, framed: Bytes, task_guard: ErasedTaskGuard);

    fn disconnect(&mut self, data: Bytes);
}

pub trait ServerTransport: fmt::Debug {
    fn process(&mut self) -> Option<ServerTransportEvent>;

    fn peer_remote_addr(&mut self, id: PeerId) -> Result<SocketAddr, PeerDisconnectError>;

    fn peer_alive(&mut self, id: PeerId) -> bool;

    fn peer_send(&mut self, id: PeerId, framed: Bytes, task_guard: ErasedTaskGuard);

    fn peer_kick(&mut self, id: PeerId, data: Bytes);
}
