use super::Extends;

pub trait AnyhowExt: Sized + Extends<anyhow::Result<Self::Output>> {
    type Output;

    fn unwrap_pretty(self) -> Self::Output;
}

impl<T> AnyhowExt for anyhow::Result<T> {
    type Output = T;

    fn unwrap_pretty(self) -> Self::Output {
        self.unwrap_or_else(|err| panic!("{err:?}"))
    }
}
