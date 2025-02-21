use std::{
    fmt, future, mem, ptr,
    sync::{
        atomic::{self, AtomicIsize, AtomicPtr, Ordering::*},
        Arc,
    },
    task,
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
    pub fn new(obj: impl 'static + Send + Sync) -> Self {
        Self(smallbox!(obj))
    }

    pub fn new_fn(f: impl 'static + Send + Sync + FnOnce()) -> Self {
        Self::new(scopeguard::guard((), move |()| f()))
    }

    pub fn noop() -> Self {
        Self(smallbox!(()))
    }
}

// === BackPressureSync === //

#[derive(Debug, Clone, Error)]
#[error("back-pressure violated")]
pub struct BackPressureSyncViolated;

#[derive(Debug, Clone)]
pub struct BackPressureSync(Arc<BackPressureSyncInner>);

#[derive(Debug)]
struct BackPressureSyncInner {
    /// Represents the number of remaining things (e.g. packets, bytes) that can be queued to send
    /// over a channel. A start operation subtracts from this count and a stop operations add that
    /// number back.
    ///
    /// It is only a violation if a packet is sent while this value is strictly negative. This
    /// allows us to guarantee that observing a non-negative `count` value before running a `start`
    /// operation will never cause a hard violation.
    ///
    /// If a violation ever occurs, this value is forced to always be negative.
    pressure: AtomicIsize,
}

impl BackPressureSync {
    pub fn new(capacity: usize) -> Self {
        Self(Arc::new(BackPressureSyncInner {
            pressure: AtomicIsize::new(isize::try_from(capacity).expect("capacity too large")),
        }))
    }

    pub fn pressure(&self) -> isize {
        self.0.pressure.load(Relaxed)
    }

    pub fn can_send(&self) -> bool {
        self.pressure() >= 0
    }

    pub fn start(&self, size: usize) -> Result<BackPressureSyncTask, BackPressureSyncViolated> {
        let size_signed = isize::try_from(size).expect("packet too large");

        let prev_value = self.0.pressure.fetch_sub(size_signed, Relaxed);

        if prev_value < 0 {
            // `count` stays negative because we never construct the task guard.
            return Err(BackPressureSyncViolated);
        }

        Ok(BackPressureSyncTask {
            inner: self.0.clone(),
            size,
        })
    }
}

#[derive(Debug)]
pub struct BackPressureSyncTask {
    inner: Arc<BackPressureSyncInner>,
    size: usize,
}

impl BackPressureSyncTask {
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for BackPressureSyncTask {
    fn drop(&mut self) {
        self.inner.pressure.fetch_add(self.size as isize, Relaxed);
    }
}

// === BackPressureAsync === //

#[derive(Debug)]
pub struct BackPressureAsync(Arc<BackPressureAsyncInner>);

#[derive(Debug)]
struct BackPressureAsyncInner {
    pressure: AtomicIsize,
    waker_data: AtomicPtr<()>,
    waker_vtable: AtomicPtr<task::RawWakerVTable>,
}

impl BackPressureAsync {
    pub fn new(capacity: usize) -> Self {
        Self(Arc::new(BackPressureAsyncInner {
            pressure: AtomicIsize::new(isize::try_from(capacity).expect("capacity too large")),
            waker_data: AtomicPtr::new(ptr::null_mut()),
            waker_vtable: AtomicPtr::new(ptr::null_mut()),
        }))
    }

    pub fn pressure(&self) -> isize {
        self.0.pressure.load(Relaxed)
    }

    pub fn start(&self, size: usize) -> BackPressureAsyncTask {
        let size_signed = isize::try_from(size).expect("packet too large");

        self.0.pressure.fetch_sub(size_signed, Relaxed);

        BackPressureAsyncTask {
            inner: self.0.clone(),
            size,
        }
    }

    pub async fn wait(&mut self) {
        let mut waker_destroy_guard = None;

        future::poll_fn(|cx| {
            // Hot-path check.
            if self.0.pressure.load(Relaxed) >= 0 {
                return task::Poll::Ready(());
            }

            // Destroy the previous waker.
            drop(waker_destroy_guard.take());

            // Publish this future's waker. We use release semantics on the `waker_data` pointer
            // because we want our vtable to be made visible to those who observe our new
            // `waker_data`.
            let waker = cx.waker().clone();
            let waker_data = waker.data();
            let waker_vtable = waker.vtable();
            mem::forget(waker);

            self.0
                .waker_vtable
                .store(waker_vtable as *const _ as *mut _, Relaxed);

            self.0
                .waker_data
                .store(waker_data as *const _ as *mut _, Release);

            // Set up a guard to attempt to re-acquire this waker if this future gets dropped.
            waker_destroy_guard = Some(scopeguard::guard((), |()| {
                let waker_data = self.0.waker_data.swap(ptr::null_mut(), Relaxed);
                if waker_data.is_null() {
                    // (someone has already taken this waker)
                    return;
                }

                let waker = unsafe { task::Waker::new(waker_data, waker_vtable) };
                drop(waker);
            }));

            // Check the current back-pressure after publishing our waker but before pending.
            // We use a fence to enforce this ordering.
            atomic::fence(SeqCst);

            match self.0.pressure.load(Relaxed) < 0 {
                true => task::Poll::Pending,
                false => task::Poll::Ready(()),
            }
        })
        .await
    }
}

#[derive(Debug)]
pub struct BackPressureAsyncTask {
    inner: Arc<BackPressureAsyncInner>,
    size: usize,
}

impl BackPressureAsyncTask {
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for BackPressureAsyncTask {
    fn drop(&mut self) {
        // Increment the pressure first.
        let old_state = self.inner.pressure.fetch_add(self.size as isize, Relaxed);
        let new_state = old_state + self.size as isize;

        if old_state >= 0 || new_state < 0 {
            // We haven't yet met the conditions for waking.
            return;
        }

        // Prevent re-ordering between back-pressure increment and notification.
        atomic::fence(SeqCst);

        // Attempt to take ownership of whichever waker is currently waiting for this task to
        // complete. We pair `Acquire` ordering for the `waker_data` load with the `Release`-ordered
        // `waker_data` store in the `wait` routine to ensure that we see the appropriate
        // `waker_vtable`.
        let waker_vtable = self.inner.waker_vtable.load(Relaxed);
        let waker_data = self.inner.waker_data.swap(ptr::null_mut(), Acquire);

        if waker_data.is_null() {
            // (nothing to wake)
            return;
        }

        let waker = unsafe { task::Waker::new(waker_data, &*waker_vtable) };
        waker.wake();
    }
}
