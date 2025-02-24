use std::io;

#[derive(Debug)]
pub struct WriteExtendAdapter<'a, V: ?Sized>(pub &'a mut V);

impl<V> io::Write for WriteExtendAdapter<'_, V>
where
    V: ?Sized + Extend<u8>,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend(buf.iter().copied());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ExtendMutAdapter<'a, V: ?Sized>(pub &'a mut V);

impl<V, I> Extend<I> for ExtendMutAdapter<'_, V>
where
    V: ?Sized + Extend<I>,
{
    fn extend<T: IntoIterator<Item = I>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}
