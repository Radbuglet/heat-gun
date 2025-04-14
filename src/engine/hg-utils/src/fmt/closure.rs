use std::fmt;

#[derive(Copy, Clone)]
pub struct FmtFn<F>(pub F)
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result;

impl<F> fmt::Debug for FmtFn<F>
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0(f)
    }
}

impl<F> fmt::Display for FmtFn<F>
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0(f)
    }
}
