use hg_ecs::{bind, Entity, World};
use macroquad::{
    color::{GRAY, GREEN},
    math::{IVec2, Vec2},
};

use crate::{
    game::{
        collide::{
            bus::{register_collider, sys_flush_colliders, Collider, ColliderBus, ColliderMask},
            tile::TileCollider,
            update::sys_update_colliders,
        },
        debug::sys_update_debug,
        gfx::{
            bus::{find_gfx, register_gfx},
            camera::{sys_update_virtual_cameras, CameraKeepArea, VirtualCamera},
            sprite::SolidRenderer,
            tile::{PaletteVisuals, TileRenderer},
        },
        kinematic::{sys_apply_kinematics, sys_kinematic_start_of_frame, Pos},
        player::{spawn_player, sys_update_players},
        tile::{TileConfig, TileLayer, TilePalette},
    },
    utils::math::Aabb,
};

pub fn world_init(world: &mut World) {
    bind!(world);

    // Create level
    let level = Entity::new(Entity::root())
        .with(VirtualCamera::default())
        .with(Pos(Vec2::ZERO))
        .with(CameraKeepArea::new(Vec2::new(1920., 1080.)))
        .with(ColliderBus::default());

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
    let mut background = level.add(TileLayer::new(TileConfig::from_size(50.), palette));

    for x in 0..10 {
        background.map.set(IVec2::splat(x) - IVec2::Y, grass);
        background.map.set(IVec2::splat(x), stone);
    }

    // Create renderer
    level.add(TileRenderer::new(vec![background]));
    register_gfx(level);

    // Create collider
    let mut collider = level.add(Collider::new(ColliderMask::ALL, TileCollider::MATERIAL));
    level.with(TileCollider::new(vec![background]));

    collider.set_aabb(Aabb::EVERYWHERE);
    register_collider(collider);

    // Spawn the player
    let player = spawn_player(level);
    player.get::<Pos>().0 = Vec2::new(100., 200.);
}

pub fn world_tick(world: &mut World) {
    bind!(world);

    Entity::flush(world_flush);

    // Update
    sys_update_virtual_cameras();
    sys_kinematic_start_of_frame();
    sys_update_players();
    sys_apply_kinematics();
    sys_update_colliders();
    sys_update_debug();

    // Render
    for camera in &find_gfx::<VirtualCamera>(Entity::root()) {
        let camera_obj = camera.get::<VirtualCamera>();
        let _guard = camera_obj.bind();

        for layer in &find_gfx::<TileRenderer>(camera) {
            layer.get::<TileRenderer>().render(camera_obj.focus());
        }

        for solid in &find_gfx::<SolidRenderer>(camera) {
            solid.get::<SolidRenderer>().render();
        }
    }
}

pub fn world_flush(world: &mut World) {
    bind!(world);

    sys_flush_colliders();
}
