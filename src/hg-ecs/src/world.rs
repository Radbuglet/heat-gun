use std::{
    any::TypeId,
    cell::{Ref, RefCell, RefMut},
    context::{
        Bundle, BundleItemLayout, BundleItemRequest, BundleItemResponse, BundleItemSet,
        BundleItemSetFor,
    },
    fmt,
    marker::PhantomData,
    mem,
    rc::Rc,
};

use rustc_hash::FxHashMap;

use crate::{ComponentInfo, Entity, Storage};

// === World === //

#[context]
pub static ROOT: Entity;

pub struct World {
    map: FxHashMap<TypeId, Rc<dyn ErasedStorage>>,
    root: Entity,
}

impl Default for World {
    fn default() -> Self {
        let mut world = Self {
            map: FxHashMap::default(),
            root: Entity::DANGLING,
        };

        world.root = {
            crate::bind_world!(world);
            Entity::new()
        };

        world
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bundle<T>(&mut self) -> DynBundle<'_, T>
    where
        T: BundleItemSet,
    {
        DynBundle::new(|layout| {
            if layout.marker_type_id() == TypeId::of::<ROOT>() {
                return Box::new(DummyProvider(self.root));
            }

            self.map
                .entry(layout.marker_type_id())
                .or_insert_with(|| {
                    let info = ComponentInfo::of(layout.marker_type_id()).unwrap_or_else(|| {
                        panic!(
                            "context item `{}` (pointee `{}`) cannot be provided by the world",
                            layout.marker_name(),
                            layout.pointee_name(),
                        )
                    });
                    (info.ctor)()
                })
                .clone()
                .fetch(layout.is_mut())
        })
    }

    pub fn root(&self) -> Entity {
        self.root
    }
}

pub trait ErasedStorage {
    fn fetch(self: Rc<Self>, is_mut: bool) -> Box<dyn DynBundleProvider>;
}

pub struct StorageOwner<T: 'static> {
    value: RefCell<Storage<T>>,
}

impl<T: 'static> Default for StorageOwner<T> {
    fn default() -> Self {
        Self {
            value: RefCell::new(Storage::new()),
        }
    }
}

impl<T: 'static> ErasedStorage for StorageOwner<T> {
    fn fetch(self: Rc<Self>, is_mut: bool) -> Box<dyn DynBundleProvider> {
        struct RefProvider<T: 'static> {
            storage: Ref<'static, Storage<T>>,
            _guard: Rc<StorageOwner<T>>,
        }

        impl<T: 'static> DynBundleProvider for RefProvider<T> {
            fn provide<'a, 'm>(
                &'a mut self,
                req: BundleItemRequest<'a, 'm>,
            ) -> BundleItemResponse<'m> {
                req.provide_ref(&*self.storage)
            }
        }

        struct MutProvider<T: 'static> {
            storage: RefMut<'static, Storage<T>>,
            _guard: Rc<StorageOwner<T>>,
        }

        impl<T: 'static> DynBundleProvider for MutProvider<T> {
            fn provide<'a, 'm>(
                &'a mut self,
                req: BundleItemRequest<'a, 'm>,
            ) -> BundleItemResponse<'m> {
                req.provide_mut(&mut *self.storage)
            }
        }

        if is_mut {
            Box::new(MutProvider {
                storage: unsafe { mem::transmute(self.value.borrow_mut()) },
                _guard: self,
            })
        } else {
            Box::new(RefProvider {
                storage: unsafe { mem::transmute(self.value.borrow()) },
                _guard: self,
            })
        }
    }
}

struct DummyProvider<T>(T);

impl<T: 'static> DynBundleProvider for DummyProvider<T> {
    fn provide<'a, 'm>(&'a mut self, req: BundleItemRequest<'a, 'm>) -> BundleItemResponse<'m> {
        req.provide_mut(&mut self.0)
    }
}

// === `world!` Macro === //

#[doc(hidden)]
pub mod world_internals {
    pub use {super::World, std::context::infer_bundle};
}

#[macro_export]
macro_rules! bind_world {
    ($world:expr) => {
        let mut bundle = $crate::world_internals::World::bundle::<$crate::world_internals::infer_bundle!('_)>(&mut $world);
        let static ..bundle.get();
    };
}

// === DynBundle === //

pub struct DynBundle<'p, T>
where
    T: BundleItemSet,
{
    _ty: PhantomData<fn(T) -> T>,
    values: Vec<Box<dyn DynBundleProvider + 'p>>,
}

impl<T: BundleItemSet> fmt::Debug for DynBundle<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynBundle").finish_non_exhaustive()
    }
}

impl<'p, T: BundleItemSet> DynBundle<'p, T> {
    pub fn new<F>(mut f: F) -> Self
    where
        F: FnMut(&'static BundleItemLayout) -> Box<dyn DynBundleProvider + 'p>,
    {
        Self::try_new::<_, ()>(|layout| Ok(f(layout))).unwrap()
    }

    pub fn try_new<F, E>(mut f: F) -> Result<Self, E>
    where
        F: FnMut(&'static BundleItemLayout) -> Result<Box<dyn DynBundleProvider + 'p>, E>,
    {
        let layouts = Bundle::<T>::layout();
        let mut values = Vec::with_capacity(layouts.len());

        for layout in layouts {
            values.push(f(layout)?);
        }

        Ok(Self {
            _ty: PhantomData,
            values,
        })
    }

    pub fn get<'r>(&'r mut self) -> Bundle<T>
    where
        T: BundleItemSetFor<'r>,
    {
        let mut values = self.values.iter_mut();

        Bundle::new_auto(|req| values.next().unwrap().provide(req))
    }
}

pub trait DynBundleProvider {
    fn provide<'a, 'm>(&'a mut self, req: BundleItemRequest<'a, 'm>) -> BundleItemResponse<'m>;
}
