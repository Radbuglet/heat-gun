#![feature(context_injection)]

use std::ops::Deref;

use hg_ecs::{archetype::ComponentId, bind, component, Entity, World, WORLD};
use macroquad::{
    color::RED,
    input::is_quit_requested,
    math::Vec2,
    shapes::draw_rectangle,
    time::get_frame_time,
    window::next_frame,
};

#[macroquad::main("Heat Gun")]
async fn main() {
    let mut world = World::new();

    world_init(&mut world);

    while !is_quit_requested() {
        world_tick(&mut world);
        next_frame().await;
    }
}

fn world_init(world: &mut World) {
    bind!(world);
    spawn_player();
}

fn world_tick(world: &mut World) {
    bind!(world);

    for obj in Entity::query([
        ComponentId::of::<UpdateHandler>(),
        ComponentId::of::<RenderHandler>(),
    ]) {
        obj.get::<UpdateHandler>()(&mut WORLD, obj);
        obj.get::<RenderHandler>()(&mut WORLD, obj);
    }

    Entity::flush();
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
    let player = Entity::new(Entity::root())
        .with(Pos::default())
        .with(Vel::default())
        .with(UpdateHandler(sys_player_update))
        .with(RenderHandler(sys_player_render));

    let foo = Entity::new(player).with(DropInspector("foo"));
    let bar = Entity::new(foo).with(DropInspector("bar"));

    foo.destroy_now();
    dbg!(bar.is_alive());

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

#[derive(Debug)]
pub struct DropInspector(pub &'static str);

component!(DropInspector);

impl Drop for DropInspector {
    fn drop(&mut self) {
        eprintln!("Dropped: {}", self.0);
    }
}
