#![feature(context_injection)]

pub mod world;

pub mod prelude {
    pub use crate::world::{bind, resource, AccessMut, AccessRef, CxOf, Resource, World, WORLD};
}

pub use prelude::*;
