use hg_ecs::{bind, Entity, World};
use macroquad::{
    color::{GRAY, GREEN},
    math::{IVec2, Vec2},
};

use crate::game::{
    graphics::{find_gfx, register_gfx},
    kinematic::{sys_apply_kinematics, sys_kinematic_start_of_frame, Pos},
    player::{spawn_player, sys_update_players},
    sprite::SolidRenderer,
    tile::{PaletteVisuals, TileConfig, TileLayer, TilePalette, TileRenderer},
};

pub fn world_init(world: &mut World) {
    bind!(world);

    // Create level
    let level = Entity::new(Entity::root());

    // Create palette
    let mut palette = level.add(TilePalette::default());
    let _air = palette.register("air", Entity::new(level).with(PaletteVisuals::Air));
    let stone = palette.register(
        "stone",
        Entity::new(level).with(PaletteVisuals::Solid(GRAY)),
    );
    let grass = palette.register(
        "grass",
        Entity::new(level).with(PaletteVisuals::Solid(GREEN)),
    );

    // Create background layer
    let mut background = level.add(TileLayer::new(
        TileConfig::new(Vec2::ZERO, Vec2::splat(50.)),
        palette,
    ));

    for x in 0..10 {
        background.map.set(IVec2::splat(x) - IVec2::Y, grass);
        background.map.set(IVec2::splat(x), stone);
    }

    // Create renderer
    level.add(TileRenderer::new(vec![background]));
    register_gfx(level);

    // Spawn the player
    let player = spawn_player(Entity::root());
    player.get::<Pos>().0 = Vec2::new(100., 200.);
}

pub fn world_tick(world: &mut World) {
    bind!(world);

    Entity::flush();

    // Update
    sys_kinematic_start_of_frame();
    sys_update_players();
    sys_apply_kinematics();

    // Render
    for layer in &find_gfx::<TileRenderer>(Entity::root()) {
        layer.get::<TileRenderer>().render();
    }

    for solid in &find_gfx::<SolidRenderer>(Entity::root()) {
        solid.get::<SolidRenderer>().render();
    }
}
