#![feature(context_injection)]

pub mod archetype;
pub mod entity;
pub mod query;
pub mod world;

pub use thunderdome::Index;

pub mod prelude {
    pub use crate::{
        entity::{component, AccessComp, AccessCompMut, AccessCompRef, Entity, Obj},
        query::Query,
        world::{bind, resource, AccessRes, AccessResMut, AccessResRef, Resource, World, WORLD},
    };
}

pub use prelude::*;
