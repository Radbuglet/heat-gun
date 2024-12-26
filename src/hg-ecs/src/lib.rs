#![feature(context_injection)]

pub mod archetype;
pub mod entity;
pub mod obj;
pub mod world;

pub mod prelude {
    pub use crate::world::{
        bind, resource, AccessRes, AccessResMut, AccessResRef, Resource, World, WORLD,
    };
}

pub use prelude::*;
