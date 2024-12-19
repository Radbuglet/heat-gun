use std::{
    any::{type_name, TypeId},
    context::{pack, Bundle},
};

use rustc_hash::FxHashMap;
use thunderdome::Index;

use crate::{component, Component, Obj};

#[derive(Copy, Clone)]
pub struct Entity(Obj<EntityInner>);

#[derive(Default)]
struct EntityInner {
    components: FxHashMap<TypeId, Index>,
}

component!(EntityInner);

impl Entity {
    pub const DANGLING: Self = Self(Obj::DANGLING);

    pub fn new() -> Self {
        Self(Obj::new(EntityInner::default()))
    }

    pub fn add<T: Component>(mut self, value: T, cx: Bundle<&mut T::Arena>) -> Obj<T> {
        let value = Obj::new(value, pack!(cx));
        self.0.components.insert(TypeId::of::<T>(), Obj::raw(value));
        value
    }

    pub fn with<T: Component>(self, value: T, cx: Bundle<&mut T::Arena>) -> Self {
        self.add(value, pack!(cx));
        self
    }

    pub fn try_get<T: Component>(self) -> Option<Obj<T>> {
        self.0
            .components
            .get(&TypeId::of::<T>())
            .copied()
            .map(Obj::from_raw)
    }

    pub fn get<T: Component>(self) -> Obj<T> {
        self.try_get()
            .unwrap_or_else(|| panic!("entity does not have component `{}`", type_name::<T>()))
    }
}
