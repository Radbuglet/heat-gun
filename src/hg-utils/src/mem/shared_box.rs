use std::{fmt, ptr::NonNull};

pub struct SharedBox<T: ?Sized> {
    value: NonNull<T>,
}

impl<T: ?Sized> fmt::Debug for SharedBox<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedBox").finish_non_exhaustive()
    }
}

impl<T: ?Sized> SharedBox<T> {
    pub fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self::from_box(Box::new(value))
    }

    pub fn from_box(value: Box<T>) -> Self {
        Self {
            value: unsafe { NonNull::new_unchecked(Box::into_raw(value)) },
        }
    }

    pub fn get(&self) -> NonNull<T> {
        self.value
    }

    pub fn get_ptr(&self) -> *mut T {
        self.value.as_ptr()
    }

    pub fn into_box(self) -> Box<T> {
        unsafe { Box::from_raw(self.value.as_ptr()) }
    }
}

impl<T: ?Sized> Drop for SharedBox<T> {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.value.as_ptr()) });
    }
}
