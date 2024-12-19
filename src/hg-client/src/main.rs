#![feature(arbitrary_self_types)]
#![feature(context_injection)]

use actor::Player;
use hg_ecs::{bind_world, World, ROOT};

use macroquad::prelude::*;

mod actor;

#[macroquad::main("Heat Gun")]
async fn main() {
    let mut world = World::new();

    init_world(&mut world);

    while !is_quit_requested() {
        tick_world(&mut world);
        next_frame().await;
    }
}

fn init_world(world: &mut World) {
    bind_world!(*world);

    let mut player = ROOT.add(Player::new(ROOT));
    player.pos = Vec2::new(100., 100.);
}

fn tick_world(world: &mut World) {
    bind_world!(*world);

    let player = ROOT.get::<Player>();

    // Update phase
    player.update();

    // Render phase
    player.render();
}
