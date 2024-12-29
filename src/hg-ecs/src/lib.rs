#![feature(context_injection)]

pub mod archetype;
pub mod entity;
pub mod world;

pub mod prelude {
    pub use crate::{
        entity::{component, AccessComp, AccessCompMut, AccessCompRef, Entity, Obj},
        world::{bind, resource, AccessRes, AccessResMut, AccessResRef, Resource, World, WORLD},
    };
}

pub use prelude::*;
