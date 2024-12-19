use std::{
    context::{unpack, Bundle, DerefCx, DerefCxMut},
    marker::PhantomData,
};

use derive_where::derive_where;
use thunderdome::Index;

use crate::Component;

#[derive_where(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Obj<T: Component> {
    _ty: PhantomData<fn(T) -> T>,
    index: Index,
}

impl<T: Component> Obj<T> {
    pub const DANGLING: Self = Self::from_raw(Index::DANGLING);

    pub fn new(value: T, cx: Bundle<&mut T::Arena>) -> Self {
        let storage = unpack!(cx => &mut T::Arena);
        let index = storage.arena.insert(value);

        Self {
            _ty: PhantomData,
            index,
        }
    }

    pub const fn raw(me: Self) -> Index {
        me.index
    }

    pub const fn from_raw(index: Index) -> Self {
        Self {
            _ty: PhantomData,
            index,
        }
    }

    pub fn destroy(me: Self, cx: Bundle<&mut T::Arena>) {
        let storage = unpack!(cx => &mut T::Arena);
        storage.arena.remove(me.index);
    }
}

impl<'i, 'o, T: Component> DerefCx<'i, 'o> for Obj<T> {
    type ContextRef = &'o T::Arena;
    type TargetCx = T;

    fn deref_cx(&'i self, cx: Bundle<Self::ContextRef>) -> &'o Self::TargetCx {
        &unpack!(cx => &T::Arena).arena[self.index]
    }
}

impl<'i, 'o, T: Component> DerefCxMut<'i, 'o> for Obj<T> {
    type ContextMut = &'o mut T::Arena;

    fn deref_cx_mut(&'i mut self, cx: Bundle<Self::ContextMut>) -> &'o mut Self::TargetCx {
        &mut unpack!(cx => &mut T::Arena).arena[self.index]
    }
}
