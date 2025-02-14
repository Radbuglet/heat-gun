use std::{
    fmt,
    sync::{
        atomic::{AtomicIsize, Ordering::*},
        Arc,
    },
};

use smallbox::{smallbox, SmallBox};
use thiserror::Error;

// === ErasedTaskGuard === //

#[expect(dead_code)] // (we intentionally keep this field around just to drop it)
pub struct ErasedTaskGuard(SmallBox<dyn Send + Sync, [usize; 2]>);

impl fmt::Debug for ErasedTaskGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErasedTaskGuard").finish_non_exhaustive()
    }
}

impl ErasedTaskGuard {
    pub fn new(obj: impl 'static + Sized + Send + Sync) -> Self {
        Self(smallbox!(obj))
    }

    pub fn noop() -> Self {
        Self(smallbox!(()))
    }
}

// === BackPressure === //

#[derive(Debug, Clone, Error)]
#[error("back-pressure violated")]
pub struct BackPressureViolated;

#[derive(Debug, Clone)]
pub struct BackPressure(Arc<BackPressureInner>);

#[derive(Debug)]
struct BackPressureInner {
    /// Represents the number of remaining things (e.g. packets, bytes) that can be queued to send
    /// over a channel. A start operation subtracts from this count and a stop operations add that
    /// number back.
    ///
    /// It is only a violation if a packet is sent while this value is strictly negative. This
    /// allows us to guarantee that observing a non-negative `count` value before running a `start`
    /// operation will never cause a hard violation.
    ///
    /// If a violation ever occurs, this value is forced to always be negative.
    count: AtomicIsize,
}

impl BackPressure {
    pub fn new(capacity: usize) -> Self {
        Self(Arc::new(BackPressureInner {
            count: AtomicIsize::new(isize::try_from(capacity).expect("capacity too large")),
        }))
    }

    pub fn pressure(&self) -> isize {
        self.0.count.load(Relaxed)
    }

    pub fn can_send(&self) -> bool {
        self.0.count.load(Relaxed) >= 0
    }

    pub fn start(&self, size: usize) -> Result<BackPressureTask, BackPressureViolated> {
        let size_signed = isize::try_from(size).expect("packet too large");

        let prev_value = self.0.count.fetch_sub(size_signed, Relaxed);

        if prev_value < 0 {
            // `count` stays negative because we never construct the task guard.
            return Err(BackPressureViolated);
        }

        Ok(BackPressureTask {
            inner: self.0.clone(),
            size,
        })
    }
}

#[derive(Debug)]
pub struct BackPressureTask {
    inner: Arc<BackPressureInner>,
    size: usize,
}

impl BackPressureTask {
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for BackPressureTask {
    fn drop(&mut self) {
        self.inner.count.fetch_add(self.size as isize, Relaxed);
    }
}
