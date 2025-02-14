use std::{
    future::{poll_fn, Future},
    pin::Pin,
    task::Poll,
};

/// A wrapper around a `Future` which returns `Some(F::Output)` the first time the future is polled
/// to completion by proxy of [`FusedFuture::wait`] and returns `None` all subsequent times.
pub enum FusedFuture<'a, F>
where
    F: ?Sized + Future,
{
    Pending(Pin<&'a mut F>),
    Completed,
}

impl<'a, F> FusedFuture<'a, F>
where
    F: ?Sized + Future,
{
    pub fn new(future: Pin<&'a mut F>) -> Self {
        Self::Pending(future)
    }

    pub async fn wait(&mut self) -> Option<F::Output> {
        poll_fn(|cx| {
            let FusedFuture::Pending(future) = self else {
                return Poll::Ready(None);
            };

            match future.as_mut().poll(cx) {
                Poll::Ready(value) => {
                    *self = FusedFuture::Completed;
                    Poll::Ready(Some(value))
                }
                Poll::Pending => Poll::Pending,
            }
        })
        .await
    }
}
