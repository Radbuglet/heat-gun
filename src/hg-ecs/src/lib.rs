#![feature(context_injection)]

pub mod archetype;
pub mod entity;
pub mod obj;
pub mod world;

pub mod prelude {
    pub use crate::{
        entity::Entity,
        obj::{component, AccessComp, AccessCompMut, AccessCompRef, Obj},
        world::{bind, resource, AccessRes, AccessResMut, AccessResRef, Resource, World, WORLD},
    };
}

pub use prelude::*;
