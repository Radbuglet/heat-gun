use hg_ecs::{bind, Entity, World};
use macroquad::{
    color::GRAY,
    math::{IVec2, Vec2},
};

use crate::game::{
    kinematic::{sys_apply_kinematics, sys_kinematic_start_of_frame, Pos},
    player::{spawn_player, sys_update_players},
    sprite::sys_render_sprites,
    tile::{
        sys_render_tiles, PaletteVisuals, TileConfig, TileLayer, TileLayerRenderer, TilePalette,
    },
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

    // Create layer
    let mut layer = level.add(TileLayer::new(
        TileConfig {
            offset: Vec2::ZERO,
            size: Vec2::splat(10.),
        },
        palette,
    ));

    for x in 0..10 {
        layer.map.set(IVec2::splat(x), stone);
    }

    // Create renderer
    level.add(TileLayerRenderer::new(layer));

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
    sys_render_tiles();
    sys_render_sprites();
}
