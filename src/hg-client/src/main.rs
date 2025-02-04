#![feature(arbitrary_self_types)]
#![feature(context_injection)]

use std::panic::{catch_unwind, AssertUnwindSafe};

use driver::{world_init, world_tick};
use hg_ecs::World;
use macroquad::{input::is_quit_requested, window::next_frame};

pub mod base;
pub mod driver;
pub mod game;
pub mod utils;

#[macroquad::main("Heat Gun")]
async fn main() {
    let Ok(mut world) = catch_unwind(AssertUnwindSafe(|| {
        let mut world = World::new();
        world_init(&mut world);
        world
    })) else {
        return;
    };

    while !is_quit_requested() {
        let crashed = catch_unwind(AssertUnwindSafe(|| {
            world_tick(&mut world);
        }))
        .is_err();

        if crashed {
            return;
        }

        next_frame().await;
    }
}
