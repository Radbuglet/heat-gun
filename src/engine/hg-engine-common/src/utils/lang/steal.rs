use std::{
    context::{pack, unpack, Bundle, DerefCxMut},
    fmt, mem,
    ops::{Deref, DerefMut},
    panic::Location,
};

use hg_ecs::{bind, entity::Component, AccessComp, Obj, World, WORLD};

use super::{Dropper, Field, NameableGuard};

// === Core === //

pub enum Steal<T> {
    Present(T),
    Stolen(&'static Location<'static>),
}

pub use Steal::*;

impl<T: fmt::Debug> fmt::Debug for Steal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Present(v) => v.fmt(f),
            Stolen(v) => f.debug_tuple("Stolen").field(&v).finish(),
        }
    }
}

impl<T: Default> Default for Steal<T> {
    fn default() -> Self {
        Self::Present(T::default())
    }
}

impl<T: Clone> Clone for Steal<T> {
    fn clone(&self) -> Self {
        Present((&**self).clone())
    }
}

impl<T> Steal<T> {
    #[track_caller]
    pub fn steal(me: &mut Self) -> T {
        // Ensure the value wasn't already stolen.
        let _ = &**me;

        // Steal it!
        let Present(v) = mem::replace(me, Stolen(Location::caller())) else {
            unreachable!()
        };

        v
    }

    pub fn un_steal(me: &mut Self, value: T) {
        *me = Present(value);
    }
}

impl<T> Deref for Steal<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Present(v) => v,
            Stolen(l) => panic!("value stolen at {l}"),
        }
    }
}

impl<T> DerefMut for Steal<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Present(v) => v,
            Stolen(l) => panic!("value stolen at {l}"),
        }
    }
}

// === ECS === //

pub type EcsSteal<'a, T, S> = NameableGuard<(&'a mut World, S), EcsStealDropper<T, S>>;

pub fn steal_from_ecs<'a, T: Component, S>(
    mut victim: Obj<T>,
    field: Field<T, Steal<S>>,
    cx: Bundle<(&'a mut WORLD, &mut AccessComp<T>)>,
) -> EcsSteal<'a, T, S> {
    let stolen = Steal::steal(field.apply_mut(victim.deref_cx_mut(pack!(cx))));

    NameableGuard::new(
        (unpack!(cx => &mut WORLD), stolen),
        EcsStealDropper { victim, field },
    )
}

pub struct EcsStealDropper<T: Component, S> {
    victim: Obj<T>,
    field: Field<T, Steal<S>>,
}

impl<'a, T: Component, S> Dropper<(&'a mut World, S)> for EcsStealDropper<T, S> {
    fn drop(mut self, (world, stolen): (&'a mut World, S)) {
        bind!(world, let cx: &mut AccessComp<T>);

        Steal::un_steal(
            self.field
                .apply_mut(&mut self.victim.deref_cx_mut(pack!(@env, cx))),
            stolen,
        );
    }
}
