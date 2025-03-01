use std::{io, pin::pin};

use anyhow::Context as _;
use futures::FutureExt as _;
use tokio::task;

use crate::{
    base::net::protocol::SocketCloseReason,
    utils::lang::{flatten_tokio_join_result, FusedFuture, MultiError},
};

pub async fn run_transport_data_handler(
    conn: quinn::Connection,
    rx_task: task::JoinHandle<anyhow::Result<()>>,
    tx_task: task::JoinHandle<anyhow::Result<()>>,
) -> Result<(), MultiError> {
    // Spawn two tasks to handle the read and write sides of this connection separately.
    let rx_task =
        pin!(rx_task.map(|v| flatten_tokio_join_result(v).context("receiver task crashed")));

    let tx_task =
        pin!(tx_task.map(|v| flatten_tokio_join_result(v).context("transmission task crashed")));

    let mut rx_task = FusedFuture::new(rx_task);
    let mut tx_task = FusedFuture::new(tx_task);

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
        conn.close(SocketCloseReason::Crash.code().into(), &[]);
    }

    // Ensure that the other side terminates before cleaning up the task.
    let (lhs, rhs) = tokio::join!(rx_task.wait(), tx_task.wait());
    let second = lhs.or(rhs).unwrap();

    // Parse the connection error.
    let third = {
        use quinn::ConnectionError::*;

        let err = conn.close_reason().unwrap();
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

pub fn filter_framed_read_failure(e: anyhow::Error) -> anyhow::Result<()> {
    use quinn::ReadError::*;

    return match e
        .downcast_ref::<io::Error>()
        .and_then(|v| v.get_ref())
        .and_then(|v| v.downcast_ref::<quinn::ReadError>())
    {
        Some(e) => match e {
            // These will already be reported by `self.conn.close_reason()`.
            Reset(_) | ConnectionLost(_) => Ok(()),

            // Basically an EOF.
            ClosedStream => Ok(()),

            // We don't use these features.
            IllegalOrderedRead | ZeroRttRejected => unreachable!(),
        },
        None => Err(e),
    };
}
