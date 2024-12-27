use std::{
    cell::Cell,
    context::{pack, unpack, Bundle, DerefCx, DerefCxMut},
    fmt,
    marker::PhantomData,
    ops::DerefMut,
    ptr::NonNull,
};

use derive_where::derive_where;

use hg_utils::hash::FxHashMap;
use thunderdome::{Arena, Index};

use crate::{entity::Entity, AccessRes, Resource, World, WORLD};

// === Component === //

pub type AccessComp<T> = AccessRes<<T as Component>::Arena>;
pub type AccessCompRef<'a, T> = (&'a WORLD, &'a AccessComp<T>);
pub type AccessCompMut<'a, T> = (&'a WORLD, &'a mut AccessComp<T>);

pub trait Component: 'static + Sized + fmt::Debug {
    type Arena: Resource + DerefMut<Target = Storage<Self>>;
}

#[derive(Debug)]
#[derive_where(Default)]
pub struct Storage<T> {
    pub arena: Arena<T>,
    pub entity_map: FxHashMap<Entity, Index>,
}

#[doc(hidden)]
pub mod component_internals {
    pub use {
        super::{Component, Storage},
        crate::resource,
        std::ops::{Deref, DerefMut},
    };
}

#[macro_export]
macro_rules! component {
    ($($ty:ty)*) => {$(
        const _: () = {
            #[derive(Default)]
            pub struct Storage($crate::obj::component_internals::Storage<$ty>);

            impl $crate::obj::component_internals::Deref for Storage {
                type Target = $crate::obj::component_internals::Storage<$ty>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl $crate::obj::component_internals::DerefMut for Storage {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }

            $crate::obj::component_internals::resource!(Storage);

            impl $crate::obj::component_internals::Component for $ty {
                type Arena = Storage;
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
            let storage = unsafe { &*world.as_mut().single::<T::Arena>() };

            if let Some(alive) = storage.arena.get(self.index) {
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
    pub fn from_raw(index: Index) -> Self {
        Self {
            _ty: PhantomData,
            index,
        }
    }

    pub fn raw(self) -> Index {
        self.index
    }

    pub fn debug<'a>(self, cx: Bundle<&'a mut WORLD>) -> ObjFmt<'a, T> {
        // FIXME: Increment `curr_origin` to avoid soundness bug

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
        &<T::Arena>::fetch(pack!(cx)).arena[self.index]
    }
}

impl<'i, 'o, T: Component> DerefCxMut<'i, 'o> for Obj<T> {
    type ContextMut = AccessCompMut<'o, T>;

    fn deref_cx_mut(&'i mut self, cx: Bundle<Self::ContextMut>) -> &'o mut Self::TargetCx {
        &mut <T::Arena>::fetch_mut(pack!(cx)).arena[self.index]
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
