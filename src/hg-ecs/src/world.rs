use std::{
    any::{type_name, TypeId},
    cell::{Cell, UnsafeCell},
    context::{
        unpack, Bundle, BundleItemRequest, BundleItemResponse, BundleItemSetFor, ContextItem,
    },
    fmt,
    marker::PhantomData,
    num::NonZeroUsize,
    ptr::NonNull,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicPtr, Ordering::*},
        OnceLock,
    },
};

use hg_utils::hash::{hash_map::Entry, FxHashMap};
use linkme::distributed_slice;

// === World === //

// Public
#[context]
pub static WORLD: World;

static HAS_WORLD: AtomicBool = AtomicBool::new(false);

pub struct World {
    /// This is a single-threaded object.
    _no_send_sync: PhantomData<*const ()>,

    /// Represents the [`WorldBundle`] instance from which a given [`AccessToken`] was fetched.
    curr_origin: NonZeroUsize,

    /// This is the set of lazily-initialized resources that this world provides.
    resources: FxHashMap<TypeId, Rc<dyn ErasedResourceValue>>,
}

impl fmt::Debug for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World").finish_non_exhaustive()
    }
}

impl World {
    pub fn new() -> Self {
        let singleton_ok = HAS_WORLD
            .compare_exchange(false, true, Relaxed, Relaxed)
            .is_ok();

        assert!(singleton_ok, "process already has a `World`");

        Self {
            _no_send_sync: PhantomData,
            curr_origin: NonZeroUsize::new(1).unwrap(),
            resources: FxHashMap::default(),
        }
    }

    pub fn single<T: Resource>(&mut self) -> *mut T {
        let res = self
            .resources
            .entry(TypeId::of::<T::Cx>())
            .or_insert_with(|| Rc::<ResourceValue<T>>::default());

        // Okay because `ResourceValue<T>` is `repr(transparent)` around `T`
        Rc::as_ptr(res) as *const T as *mut T
    }

    pub fn reborrow(&mut self) -> WorldReborrow<'_> {
        // Invalidate all previous tokens.
        let prev_token = self.curr_origin;
        self.curr_origin = self
            .curr_origin
            .checked_add(1)
            .expect("too many nested bundle creations");

        WorldReborrow {
            world: self,
            prev_origin: prev_token,
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        HAS_WORLD.store(false, Relaxed);
    }
}

#[derive(Debug)]
pub struct WorldReborrow<'a> {
    world: &'a mut World,
    prev_origin: NonZeroUsize,
}

impl WorldReborrow<'_> {
    pub fn immutable<'a>(&'a self) -> ImmutableWorld<'a> {
        ImmutableWorld(unsafe { &*(self.world as *const World as *const UnsafeCell<World>) })
    }

    pub fn bundle<'a, T>(&'a mut self) -> Bundle<T>
    where
        T: BundleItemSetFor<'a>,
    {
        // We begin by determining the shape of our bundle. This frees up `self` to be injected later.
        enum Provider {
            World,
            Storage(*const dyn ErasedResourceValue),
        }

        let providers = Bundle::<T>::layout()
            .iter()
            .map(|req| {
                // Special case some of our providers.
                if req.marker_type_id() == TypeId::of::<WORLD>() {
                    return Provider::World;
                }

                // Otherwise, fetch the resource.
                let comp = self
                    .world
                    .resources
                    .entry(req.marker_type_id())
                    .or_insert_with(|| {
                        let info =
                            ResourceInfo::lookup(req.marker_type_id()).unwrap_or_else(|| {
                                panic!(
                                    "cannot provide `{}` (pointee `{}`): not a resource",
                                    req.marker_name(),
                                    req.pointee_name()
                                );
                            });

                        (info.ctor)()
                    });

                Provider::Storage(Rc::as_ptr(comp))
            })
            .collect::<Vec<_>>();

        // Now, we can build the actual bundle.
        let curr_origin = self.world.curr_origin;
        let mut world = Some(&mut *self.world);
        let mut borrows = FxHashMap::default();

        let mut providers = providers.into_iter();

        Bundle::new_auto(|req| match providers.next().unwrap() {
            Provider::World => {
                req.provide_mut(world.take().expect("cannot provide `WORLD` more than once"))
            }
            Provider::Storage(val) => {
                // Ensure that provision is valid.
                match borrows.entry(req.marker_type_id()) {
                    Entry::Occupied(entry) => {
                        if *entry.get() {
                            panic!(
                                "failed to borrow `{}` {}: was already borrowed mutably",
                                if req.is_mut() { "mutably" } else { "immutably" },
                                ResourceInfo::lookup(req.marker_type_id()).unwrap().name
                            );
                        } else {
                            if req.is_mut() {
                                panic!(
                                    "failed to borrow `{}` mutably: was already borrowed immutably",
                                    ResourceInfo::lookup(req.marker_type_id()).unwrap().name
                                );
                            }
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(req.is_mut());
                    }
                }

                // Provide the actual value
                unsafe { (*val).provide(req, curr_origin) }
            }
        })
    }
}

impl<'a> Drop for WorldReborrow<'a> {
    fn drop(&mut self) {
        // The `AccessToken`s we lend out can only live for as long as this `WorldBundle` instance
        // is alive. Hence, it is safe to restore the previous state and allow this current state to
        // be reused.

        // Of course, we need to ensure that we're not being dropped out of order.
        assert_eq!(
            self.prev_origin.checked_add(1),
            Some(self.world.curr_origin)
        );

        self.world.curr_origin = self.prev_origin;
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ImmutableWorld<'a>(&'a UnsafeCell<World>);

impl<'a> ImmutableWorld<'a> {
    thread_local! {
        static TLS_WORLD: Cell<Option<NonNull<UnsafeCell<World>>>> = const { Cell::new(None) };
    }

    pub fn bind_tls<R>(self, f: impl FnOnce() -> R) -> R {
        let _restore = scopeguard::guard(Self::TLS_WORLD.get(), |old| {
            Self::TLS_WORLD.set(old);
        });

        Self::TLS_WORLD.set(Some(NonNull::from(self.0)));

        f()
    }

    pub fn try_use_tls<R>(f: impl FnOnce(Option<ImmutableWorld<'_>>) -> R) -> R {
        let world = Self::TLS_WORLD
            .get()
            .map(|world| ImmutableWorld(unsafe { world.as_ref() }));

        f(world)
    }

    pub fn read<T: Resource>(self) -> &'a T {
        unsafe {
            let world = &mut *self.0.get();
            &*world.single::<T>()
        }
    }
}

// ResourceValue
#[derive(Default)]
#[repr(transparent)]
struct ResourceValue<T: Resource> {
    value: UnsafeCell<T>,
}

trait ErasedResourceValue {
    unsafe fn provide<'a, 'm>(
        &'a self,
        req: BundleItemRequest<'a, 'm>,
        curr_token: NonZeroUsize,
    ) -> BundleItemResponse<'m>;
}

impl<T: Resource> ErasedResourceValue for ResourceValue<T> {
    unsafe fn provide<'a, 'm>(
        &'a self,
        req: BundleItemRequest<'a, 'm>,
        curr_token: NonZeroUsize,
    ) -> BundleItemResponse<'m> {
        // It's okay to clobber this value because it is either dangling and inaccessible or already
        // points to the same thing we just replaced it with.
        T::slot().store(self.value.get(), Relaxed);

        // Safety provided by caller.
        req.provide_mut(AccessToken::<T>::new(curr_token))
    }
}

// `bind!` macro

#[doc(hidden)]
pub mod bind_internals {
    pub use std::context::infer_bundle;
}

#[macro_export]
macro_rules! bind {
    ($world:expr) => {
        let mut cx = $world.reborrow();
        let static ..cx.bundle::<$crate::world::bind_internals::infer_bundle!('_)>();
    };
}

pub use bind;

// === Resource === //

// Core trait
pub type AccessRes<R> = <R as Resource>::Cx;
pub type AccessResRef<'a, R> = (&'a WORLD, &'a AccessRes<R>);
pub type AccessResMut<'a, R> = (&'a WORLD, &'a mut AccessRes<R>);

pub unsafe trait Resource: Sized + 'static + Default {
    type Cx: ContextItem<Item = AccessToken<Self>>;

    fn slot() -> &'static AtomicPtr<Self>;

    fn fetch<'a>(cx: Bundle<AccessResRef<'a, Self>>) -> &'a Self {
        let world = unpack!(cx => &WORLD);
        let token = unpack!(cx => &Self::Cx);

        assert_eq!(world.curr_origin, token.id());

        unsafe { &*Self::slot().load(Relaxed) }
    }

    fn fetch_mut<'a>(cx: Bundle<AccessResMut<'a, Self>>) -> &'a mut Self {
        let world = unpack!(cx => &WORLD);
        let token = unpack!(cx => &Self::Cx);

        assert_eq!(world.curr_origin, token.id());

        unsafe { &mut *Self::slot().load(Relaxed) }
    }
}

// ResourceInfo
#[distributed_slice]
pub static RESOURCES: [fn() -> ResourceInfo];

pub struct ResourceInfo {
    name: &'static str,
    ctx_ty: TypeId,
    ctor: fn() -> Rc<dyn ErasedResourceValue>,
}

impl ResourceInfo {
    pub fn de_novo<T: Resource>() -> Self {
        Self {
            name: type_name::<T>(),
            ctx_ty: TypeId::of::<T::Cx>(),
            ctor: || Rc::<ResourceValue<T>>::default(),
        }
    }

    pub fn lookup(ctx_ty: TypeId) -> Option<&'static Self> {
        static MAP: OnceLock<FxHashMap<TypeId, ResourceInfo>> = OnceLock::new();

        MAP.get_or_init(|| {
            RESOURCES
                .iter()
                .map(|v| {
                    let v = v();
                    (v.ctx_ty, v)
                })
                .collect()
        })
        .get(&ctx_ty)
    }
}

// Definition Macro
#[doc(hidden)]
pub mod resource_internals {
    pub use {
        super::{AccessToken, Resource, ResourceInfo, RESOURCES},
        linkme::{self, distributed_slice},
        std::{ptr::null_mut, sync::atomic::AtomicPtr},
    };
}

#[macro_export]
macro_rules! resource {
    ($($ty:ty),*$(,)?) => {$(
        const _: () = {
            #[context]
            pub static CX: $crate::world::resource_internals::AccessToken<$ty>;

            unsafe impl $crate::world::resource_internals::Resource for $ty {
                type Cx = CX;

                fn slot() -> &'static $crate::world::resource_internals::AtomicPtr<$ty> {
                    static SLOT: $crate::world::resource_internals::AtomicPtr<$ty> =
                        $crate::world::resource_internals::AtomicPtr::new(
                            $crate::world::resource_internals::null_mut(),
                        );

                    &SLOT
                }
            }

            #[$crate::world::resource_internals::distributed_slice($crate::world::resource_internals::RESOURCES)]
            #[linkme(crate = $crate::world::resource_internals::linkme)]
            static RES: fn() -> $crate::world::resource_internals::ResourceInfo
                = $crate::world::resource_internals::ResourceInfo::de_novo::<$ty>;
        };
    )*};
}

pub use resource;

// === AccessToken === //

#[repr(align(1))]
pub struct AccessToken<T: Resource> {
    _no_send_sync: PhantomData<*const ()>,
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: Resource> fmt::Debug for AccessToken<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AccessToken").field(&self.id()).finish()
    }
}

impl<T: Resource> AccessToken<T> {
    pub unsafe fn new<'a>(id: NonZeroUsize) -> &'a mut Self {
        unsafe { NonNull::new_unchecked(id.get() as *mut Self).as_mut() }
    }

    pub fn id(&self) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(self as *const Self as usize) }
    }
}

// === WorldFmt === //

pub type WorldFmtRef<'a, T> = WorldFmt<'a, &'a T>;

pub struct WorldFmt<'a, T> {
    pub world: WorldReborrow<'a>,
    pub value: T,
}

impl<'a, T> WorldFmt<'a, T> {
    pub fn new(value: T, cx: Bundle<&'a mut WORLD>) -> Self {
        Self {
            world: unpack!(cx => &mut WORLD).reborrow(),
            value,
        }
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for WorldFmt<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.world.immutable().bind_tls(|| self.value.fmt(f))
    }
}

impl<'a, T: fmt::Display> fmt::Display for WorldFmt<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.world.immutable().bind_tls(|| self.value.fmt(f))
    }
}
