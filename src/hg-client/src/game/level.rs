use std::context::{infer_bundle, Bundle};

use hg_ecs::Entity;
use macroquad::{color::{GRAY, GREEN}, math::{IVec2, Vec2}};

use crate::{
    base::{
        collide::{
            bus::{register_collider, Collider, ColliderBus, ColliderMask},
            tile::{PaletteCollider, TileCollider}
        },
        debug::{set_debug_draw, DebugDraw},
        gfx::{
            bus::register_gfx,
            camera::{CameraKeepArea, VirtualCamera, VirtualCameraSelector},
            tile::{PaletteVisuals, TileRenderer},
        },
        kinematic::Pos,
        tile::{TileConfig, TileLayer, TileLayerSet, TilePalette},
    },
    utils::math::Aabb,
};

// === Prefabs === //

pub fn spawn_level(parent: Entity) -> Entity {
    let level = Entity::new(parent)
        .with(ColliderBus::default())
        .with(DebugDraw::default());

    set_debug_draw(level.get());

    // Setup camera
    let mut camera_selector = level.add(VirtualCameraSelector::default());
    let camera = spawn_camera(level);
    camera_selector.set_current(camera.get());

    // Setup tile map
    attach_palette(level);
    spawn_tile_map(level);

    // Register with services
    register_gfx(level);

    level
}

fn attach_palette(target: Entity, cx: Bundle<infer_bundle!('_)>) {
    let static ..cx;

    let mut palette = target.add(TilePalette::default());
    palette.register(
        "air",
        Entity::new(target)
            .with(PaletteCollider::Disabled)
            .with(PaletteVisuals::Air),
    );
    palette.register(
        "grass",
        Entity::new(target)
            .with(PaletteCollider::Solid)
            .with(PaletteVisuals::Solid(GREEN)),
    );
    palette.register(
        "stone",
        Entity::new(target)
            .with(PaletteCollider::Solid)
            .with(PaletteVisuals::Solid(GRAY)),
    );
}

fn spawn_camera(parent: Entity) -> Entity {
    Entity::new(parent)
        .with(Pos::default())
        .with(VirtualCamera::default())
        .with(CameraKeepArea::new(Vec2::new(1920., 1080.)))
}

fn spawn_tile_map(parent: Entity) -> Entity {
    let map = Entity::new(parent);

    // Setup layers
    let background = spawn_layer(map);
    let foreground = spawn_layer(map);

    let layers = map.add(TileLayerSet::new(vec![background.get(), foreground.get()]));

    // Setup renderer
    map.add(TileRenderer::new(layers));

    // Setup collider
    map.add(TileCollider::new(layers));

    let mut collider = map.add(Collider::new(ColliderMask::ALL, TileCollider::MATERIAL));
    collider.set_aabb(Aabb::EVERYWHERE);

    // Initialize map
    {
        let mut background = background.get::<TileLayer>();

        let grass = background.palette.lookup_by_name("grass");
        let stone = background.palette.lookup_by_name("stone");

        for x in 0..10 {
            background.map.set(IVec2::splat(x) - IVec2::Y, grass);
            background.map.set(IVec2::splat(x), stone);
        }
    }

    // Register with services
    register_collider(collider);
    register_gfx(map);

    map
}

fn spawn_layer(parent: Entity) -> Entity {
    Entity::new(parent).with(TileLayer::new(
        TileConfig::from_size(100.),
        parent.deep_get::<TilePalette>(),
    ))
}
