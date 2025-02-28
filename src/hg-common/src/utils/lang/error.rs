use std::{
    error::Error,
    fmt,
    future::Future,
    panic::{self, AssertUnwindSafe},
};

use scopeguard::ScopeGuard;
use smallvec::SmallVec;
use tokio::task;

// === Error catching === //

pub const PANIC_ERR_MSG: &str = "worker thread panicked";

pub fn worker_panic_error() -> anyhow::Error {
    anyhow::anyhow!(PANIC_ERR_MSG)
}

pub fn catch_termination<T>(f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
    match panic::catch_unwind(AssertUnwindSafe(|| f())) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(v)) => Err(v),
        Err(_panic) => Err(worker_panic_error()),
    }
}

pub async fn catch_termination_async<T>(
    fut: impl Future<Output = T>,
    handle: impl FnOnce(Option<T>),
) {
    let handle = scopeguard::guard_on_unwind(handle, |handle| {
        handle(None);
    });

    let res = fut.await;
    let handle = ScopeGuard::into_inner(handle);

    handle(Some(res));
}

pub fn absorb_result_std<T, E: Error>(op: &str, f: impl FnOnce() -> Result<T, E>) -> Option<T> {
    match f() {
        Ok(v) => Some(v),
        Err(err) => {
            tracing::debug!("failed to {op}: {err:?}");
            None
        }
    }
}

pub fn absorb_result_anyhow<T>(op: &str, f: impl FnOnce() -> anyhow::Result<T>) -> Option<T> {
    match f() {
        Ok(v) => Some(v),
        Err(err) => {
            tracing::debug!("failed to {op}: {err:?}");
            None
        }
    }
}

pub fn flatten_tokio_join_result<T>(
    res: Result<anyhow::Result<T>, task::JoinError>,
) -> anyhow::Result<T> {
    match res {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(anyhow::Error::new(err).context(PANIC_ERR_MSG)),
    }
}

// === try macros === //

#[macro_export]
macro_rules! try_sync {
    ($($body:tt)*) => {
        || -> ::anyhow::Result<_> { ::anyhow::Result::Ok({$($body)*}) }()
    };
}

pub use try_sync;

#[macro_export]
macro_rules! try_sync_opt {
    ($($body:tt)*) => {
        || -> ::core::option::Option<_> { ::core::option::Option::Some({$($body)*}) }()
    };
}

pub use try_sync_opt;

#[macro_export]
macro_rules! try_async {
    ($($body:tt)*) => {{
        let res: ::anyhow::Result<_> = async {
            Ok({ $($body)* })
        }.await;
        res
    }};
}

pub use try_async;

// === MultiError === //

pub type MultiResult<T> = Result<T, MultiError>;

const MULTI_ERR_CAP: usize = 1;

pub struct MultiError(SmallVec<[anyhow::Error; MULTI_ERR_CAP]>);

impl fmt::Display for MultiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !f.alternate() {
            return self.primary_err().fmt(f);
        }

        write!(f, "{:#}", self.primary_err())?;

        if !self.secondary_errs().is_empty() {
            writeln!(f, "Secondary errors:")?;
            writeln!(f)?;

            for secondary in self.secondary_errs() {
                writeln!(f, "{secondary:#}")?;
            }
        }

        Ok(())
    }
}

impl fmt::Debug for MultiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            // Alternate mode is inherited within the same formatter.
            return f.debug_tuple("MultiError").field(&self.0).finish();
        }

        write!(f, "{:?}", self.primary_err())?;

        if !self.secondary_errs().is_empty() {
            writeln!(f)?;
            writeln!(f)?;
            writeln!(f, "Secondary errors:")?;
            writeln!(f)?;

            for secondary in self.secondary_errs() {
                writeln!(f, "{secondary:?}")?;
            }
        }

        Ok(())
    }
}

impl Error for MultiError {}

impl MultiError {
    pub fn new<E>(error: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::wrap(anyhow::Error::new(error))
    }

    pub fn wrap(err: anyhow::Error) -> Self {
        Self(SmallVec::from_iter([err]))
    }

    pub fn from_iter(iter: impl IntoIterator<Item = anyhow::Result<()>>) -> MultiResult<()> {
        let list = iter
            .into_iter()
            .filter_map(|v| v.err())
            .collect::<SmallVec<[anyhow::Error; MULTI_ERR_CAP]>>();

        if list.is_empty() {
            Ok(())
        } else {
            Err(Self(list))
        }
    }

    pub fn push_err(&mut self, err: anyhow::Error) {
        self.0.push(err);
    }

    pub fn push<T>(&mut self, res: anyhow::Result<T>) -> Option<T> {
        match res {
            Ok(v) => Some(v),
            Err(e) => {
                self.push_err(e);
                None
            }
        }
    }

    pub fn with_err(mut self, err: anyhow::Error) -> Self {
        self.push_err(err);
        self
    }

    pub fn with<T>(mut self, err: anyhow::Result<T>) -> Self {
        self.push(err);
        self
    }

    pub fn errors(&self) -> &[anyhow::Error] {
        &self.0
    }

    pub fn primary_err(&self) -> &anyhow::Error {
        &self.0[0]
    }

    pub fn secondary_errs(&self) -> &[anyhow::Error] {
        &self.0[1..]
    }
}

impl From<anyhow::Error> for MultiError {
    fn from(err: anyhow::Error) -> Self {
        Self::wrap(err)
    }
}

impl IntoIterator for MultiError {
    type Item = anyhow::Error;
    type IntoIter = MultiErrorIter;

    fn into_iter(self) -> Self::IntoIter {
        MultiErrorIter(self.0.into_iter())
    }
}

pub struct MultiErrorIter(smallvec::IntoIter<[anyhow::Error; MULTI_ERR_CAP]>);

impl ExactSizeIterator for MultiErrorIter {}

impl Iterator for MultiErrorIter {
    type Item = anyhow::Error;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for MultiErrorIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}
