use std::{
    cell::Cell,
    context::{pack, unpack, Bundle, DerefCx, DerefCxMut},
    fmt,
    marker::PhantomData,
    ops::DerefMut,
    ptr::NonNull,
};

use derive_where::derive_where;

use thunderdome::{Arena, Index};

use crate::{AccessRes, Resource, World, WORLD};

// === Component === //

pub type AccessComp<T> = AccessRes<<T as Component>::Arena>;
pub type AccessCompRef<'a, T> = (&'a WORLD, &'a AccessComp<T>);
pub type AccessCompMut<'a, T> = (&'a WORLD, &'a mut AccessComp<T>);

pub trait Component: 'static + Sized + fmt::Debug {
    type Arena: Resource + DerefMut<Target = Arena<Self>>;
}

#[doc(hidden)]
pub mod component_internals {
    pub use {
        super::Component,
        crate::resource,
        std::ops::{Deref, DerefMut},
        thunderdome::Arena,
    };
}

#[macro_export]
macro_rules! component {
    ($($ty:ty)*) => {$(
        const _: () = {
            #[derive(Default)]
            pub struct Arena($crate::obj::component_internals::Arena<$ty>);

            impl $crate::obj::component_internals::Deref for Arena {
                type Target = $crate::obj::component_internals::Arena<$ty>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl $crate::obj::component_internals::DerefMut for Arena {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }

            $crate::obj::component_internals::resource!(Arena);

            impl $crate::obj::component_internals::Component for $ty {
                type Arena = Arena;
            }
        };
    )*};
}

pub use component;

// === Obj === //

thread_local! {
    static FMT_WORLD: Cell<Option<NonNull<World>>> = const { Cell::new(None) };
}

#[derive_where(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Obj<T: Component> {
    _ty: PhantomData<fn(T) -> T>,
    index: Index,
}

impl<T: Component> fmt::Debug for Obj<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let index = self.index.to_bits();

        if let Some(mut world) = FMT_WORLD.get() {
            let arena = unsafe { &*world.as_mut().single::<T::Arena>() };

            if let Some(alive) = arena.get(self.index) {
                f.debug_tuple("Obj")
                    .field(&format_args!("0x{index:x}"))
                    .field(alive)
                    .finish()
            } else {
                struct Dead;

                impl fmt::Debug for Dead {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("<dead>")
                    }
                }

                f.debug_tuple("Obj")
                    .field(&format_args!("0x{index:x}"))
                    .field(&Dead)
                    .finish()
            }
        } else {
            f.debug_tuple("Obj")
                .field(&format_args!("0x{index:x}"))
                .finish()
        }
    }
}

impl<T: Component> Obj<T> {
    pub fn new(value: T, cx: Bundle<AccessCompMut<'_, T>>) -> Self {
        let arena = <T::Arena>::fetch_mut(pack!(cx));

        Self {
            _ty: PhantomData,
            index: arena.insert(value),
        }
    }

    pub fn destroy(self, cx: Bundle<AccessCompMut<'_, T>>) {
        <T::Arena>::fetch_mut(pack!(cx)).remove(self.index);
    }

    pub fn debug<'a>(self, cx: Bundle<&'a mut WORLD>) -> ObjFmt<'a, T> {
        ObjFmt {
            _ty: PhantomData,
            world: NonNull::from(unpack!(cx => &mut WORLD)),
            obj: self,
        }
    }
}

impl<'i, 'o, T: Component> DerefCx<'i, 'o> for Obj<T> {
    type ContextRef = AccessCompRef<'o, T>;
    type TargetCx = T;

    fn deref_cx(&'i self, cx: Bundle<Self::ContextRef>) -> &'o Self::TargetCx {
        &<T::Arena>::fetch(pack!(cx))[self.index]
    }
}

impl<'i, 'o, T: Component> DerefCxMut<'i, 'o> for Obj<T> {
    type ContextMut = AccessCompMut<'o, T>;

    fn deref_cx_mut(&'i mut self, cx: Bundle<Self::ContextMut>) -> &'o mut Self::TargetCx {
        &mut <T::Arena>::fetch_mut(pack!(cx))[self.index]
    }
}

pub struct ObjFmt<'a, T: Component> {
    _ty: PhantomData<&'a mut World>,
    world: NonNull<World>,
    obj: Obj<T>,
}

impl<'a, T: Component> fmt::Debug for ObjFmt<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _restore = scopeguard::guard(FMT_WORLD.get(), |old| {
            FMT_WORLD.set(old);
        });

        FMT_WORLD.set(Some(self.world));

        self.obj.fmt(f)
    }
}
