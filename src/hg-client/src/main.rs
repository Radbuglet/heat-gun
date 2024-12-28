#![feature(context_injection)]

use std::ops::Deref;

use hg_ecs::{bind, component, Entity, World, WORLD};
use macroquad::{color::RED, input::is_quit_requested, math::Vec2, shapes::draw_rectangle, time::get_frame_time, window::next_frame};

#[macroquad::main("Heat Gun")]
async fn main() {
    let mut world = World::new();

    let root = world_init(&mut world);

    while !is_quit_requested() {
        world_tick(&mut world, root);
        next_frame().await;
    }
}

fn world_init(world: &mut World) -> Entity {
    bind!(world);

    spawn_player()
}

fn world_tick(world: &mut World, root: Entity) {
    bind!(world);

    root.get::<UpdateHandler>()(&mut WORLD, root);
    root.get::<RenderHandler>()(&mut WORLD, root);
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Pos(Vec2);

component!(Pos);

#[derive(Debug, Copy, Clone, Default)]
pub struct Vel(Vec2);

component!(Vel);

#[derive(Debug, Copy, Clone)]
pub struct UpdateHandler(pub fn(&mut World, Entity));

impl Deref for UpdateHandler {
    type Target = fn(&mut World, Entity);

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

component!(UpdateHandler);

#[derive(Debug, Copy, Clone)]
pub struct RenderHandler(pub fn(&mut World, Entity));

impl Deref for RenderHandler {
    type Target = fn(&mut World, Entity);

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

component!(RenderHandler);

pub fn spawn_player() -> Entity {
    let player = Entity::new()
        .with(Pos::default())
        .with(Vel::default())
        .with(UpdateHandler(sys_player_update))
        .with(RenderHandler(sys_player_render));

    player.get::<Pos>().0 = Vec2::new(100., 500.);
    player.get::<Vel>().0 = Vec2::new(100., -200.);
    player
}

fn sys_player_update(world: &mut World, entity: Entity) {
    bind!(world);

    let dt = get_frame_time();
    let mut pos = entity.get::<Pos>();
    let mut vel = entity.get::<Vel>();

    pos.0 += vel.0 * dt;
    vel.0 += Vec2::Y * dt * 100.;
}

fn sys_player_render(world: &mut World, entity: Entity) {
    bind!(world);

    let pos = entity.get::<Pos>().0;

    draw_rectangle(pos.x - 10., pos.y - 10., 20., 20., RED);
}
