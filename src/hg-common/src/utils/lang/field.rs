use std::{any::type_name, fmt, marker::PhantomData, mem};

use derive_where::derive_where;

// === Field === //

#[derive_where(Copy, Clone)]
pub struct Field<I, O> {
    _ty: PhantomData<fn((I, O)) -> (I, O)>,
    offset: usize,
}

impl<I, O> fmt::Debug for Field<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(&format!(
            "Field({} -> {})",
            type_name::<I>(),
            type_name::<O>()
        ))
        .field(&self.offset)
        .finish()
    }
}

impl<I, O> Field<I, O> {
    pub const unsafe fn new_unchecked(offset: usize) -> Self {
        Self {
            _ty: PhantomData,
            offset,
        }
    }

    pub const fn offset(self) -> usize {
        self.offset
    }

    pub const fn apply_ptr(self, ptr: *const I) -> *const O {
        unsafe { ptr.cast::<u8>().add(self.offset).cast::<O>() }
    }

    pub const fn apply_ptr_mut(self, ptr: *mut I) -> *mut O {
        self.apply_ptr(ptr) as *mut O
    }

    pub const fn apply(self, val: &I) -> &O {
        unsafe { &*self.apply_ptr(val) }
    }

    pub const fn apply_mut(self, val: &mut I) -> &mut O {
        unsafe { &mut *self.apply_ptr_mut(val) }
    }

    pub const fn chain<P>(self, next: Field<O, P>) -> Field<I, P> {
        unsafe { Field::new_unchecked(self.offset + next.offset) }
    }

    pub fn smuggle(self, input: I) -> O {
        let sub = self.apply_ptr(&input);
        let sub = unsafe { sub.read() };
        mem::forget(input);
        sub
    }
}

// === Macro === //

#[doc(hidden)]
pub mod field_macro_internals {
    pub use {super::Field, std::mem::offset_of};

    pub const fn new<I, O>(offset: usize, _dummy: fn(&I) -> &O) -> Field<I, O> {
        unsafe { Field::new_unchecked(offset) }
    }
}

#[macro_export]
macro_rules! field {
    ($Container:ty, $($fields:tt)+ $(,)?) => {
        $crate::utils::lang::field_macro_internals::new::<$Container, _>(
            $crate::utils::lang::field_macro_internals::offset_of!($Container, $($fields)*),
            |val| { &val.$($fields)* },
        )
    };
}

pub use field;
