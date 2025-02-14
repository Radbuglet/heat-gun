#[derive(Debug)]
pub struct ExtendMutAdapter<'a, V: ?Sized>(pub &'a mut V);

impl<V: ?Sized, I> Extend<I> for ExtendMutAdapter<'_, V>
where
    V: Extend<I>,
{
    fn extend<T: IntoIterator<Item = I>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}
