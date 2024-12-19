use std::{
    any::{type_name, TypeId},
    context::ContextItem,
    rc::Rc,
    sync::OnceLock,
};

use linkme::distributed_slice;
use rustc_hash::FxHashMap;
use thunderdome::Arena;

use crate::{ErasedStorage, StorageOwner};

// === Storage === //

pub struct Storage<T> {
    pub(crate) arena: Arena<T>,
}

impl<T> Storage<T> {
    pub const fn new() -> Self {
        Self {
            arena: Arena::new(),
        }
    }
}

// === ComponentInfo === //

#[distributed_slice]
pub static COMPONENTS: [fn() -> ComponentInfo];

#[derive(Debug)]
pub struct ComponentInfo {
    pub marker_id: TypeId,
    pub ty_name: &'static str,
    pub ctor: fn() -> Rc<dyn ErasedStorage>,
}

impl ComponentInfo {
    pub fn de_novo<T: Component>() -> Self {
        Self {
            marker_id: TypeId::of::<T::Arena>(),
            ty_name: type_name::<T>(),
            ctor: || <Rc<StorageOwner<T>>>::default(),
        }
    }

    pub fn of(marker_id: TypeId) -> Option<&'static Self> {
        static COMPONENTS_MAP: OnceLock<FxHashMap<TypeId, ComponentInfo>> = OnceLock::new();

        COMPONENTS_MAP
            .get_or_init(|| {
                COMPONENTS
                    .iter()
                    .map(|v| {
                        let v = v();
                        (v.marker_id, v)
                    })
                    .collect()
            })
            .get(&marker_id)
    }
}

// === Component === //

#[doc(hidden)]
pub mod component_internals {
    pub use {
        super::{Component, ComponentInfo, Storage, COMPONENTS},
        linkme,
    };
}

pub trait Component: Sized + 'static {
    type Arena: ContextItem<Item = Storage<Self>>;
}

#[macro_export]
macro_rules! component {
    ($($ty:ty),*$(,)?) => {$(
        const _: () = {
            #[context]
            pub static ARENA: $crate::component_internals::Storage<$ty>;

            #[$crate::component_internals::linkme::distributed_slice($crate::component_internals::COMPONENTS)]
            #[linkme(crate = $crate::component_internals::linkme)]
            pub static COMPONENTS: fn() -> $crate::component_internals::ComponentInfo =
                $crate::component_internals::ComponentInfo::de_novo::<$ty>;

            impl $crate::component_internals::Component for $ty {
                type Arena = ARENA;
            }
        };
    )*};
}
