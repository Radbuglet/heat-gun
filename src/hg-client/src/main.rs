#![feature(arbitrary_self_types)]
#![feature(context_injection)]

use std::panic::{catch_unwind, AssertUnwindSafe};

use hg_ecs::World;
use macroquad::{input::is_quit_requested, window::next_frame};
use driver::{world_init, world_tick};

pub mod driver;
pub mod game;
pub mod utils;

#[macroquad::main("Heat Gun")]
async fn main() {
    let mut world = World::new();

    world_init(&mut world);

    while !is_quit_requested() {
        if catch_unwind(AssertUnwindSafe(|| {
            world_tick(&mut world);
        })).is_err() {
            return;
        }
        next_frame().await;
    }
}
