use core::fmt;
use std::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use super::field;

pub struct NameableGuard<T, D: Dropper<T>>((ManuallyDrop<T>, ManuallyDrop<D>));

pub trait Dropper<T> {
    fn drop(self, value: T);
}

impl<T, D: Dropper<T>> NameableGuard<T, D> {
    pub fn new(target: T, dropper: D) -> Self {
        Self((ManuallyDrop::new(target), ManuallyDrop::new(dropper)))
    }

    pub fn into_inner_pair(self) -> (T, D) {
        let (target, dropper) = field!(Self, 0).smuggle(self);

        (
            ManuallyDrop::into_inner(target),
            ManuallyDrop::into_inner(dropper),
        )
    }

    pub fn into_inner(self) -> T {
        self.into_inner_pair().0
    }
}

impl<T: fmt::Debug, D: Dropper<T>> fmt::Debug for NameableGuard<T, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (&**self).fmt(f)
    }
}

impl<T, D: Dropper<T>> Deref for NameableGuard<T, D> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &(self.0).0
    }
}

impl<T, D: Dropper<T>> DerefMut for NameableGuard<T, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut (self.0).0
    }
}

impl<T, D> Drop for NameableGuard<T, D>
where
    D: Dropper<T>,
{
    fn drop(&mut self) {
        let target = unsafe { ManuallyDrop::take(&mut (self.0).0) };
        let dropper = unsafe { ManuallyDrop::take(&mut (self.0).1) };

        dropper.drop(target);
    }
}
