mod sealed {
    pub trait Sealed<T: ?Sized> {}
}

use sealed::Sealed;

pub trait Extends<T: ?Sized>: Sealed<T> {}

impl<T: ?Sized> Sealed<T> for T {}

impl<T: ?Sized> Extends<T> for T {}
