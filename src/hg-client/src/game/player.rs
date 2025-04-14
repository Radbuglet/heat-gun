use std::time::Instant;

use hg_common::game::player::{
    PlayerOwnerRpcKind, PlayerOwnerRpcSb, PlayerPuppetRpcCb, PlayerPuppetRpcKind, PlayerRpcKind,
};
use hg_ecs::{component, Entity, Obj, Query};
use hg_engine_client::base::gfx::{
    bus::register_gfx,
    camera::{VirtualCamera, VirtualCameraSelector},
    sprite::SolidRenderer,
};
use hg_engine_common::{
    base::{
        collide::{
            bus::{collide_everything, ColliderMask, ColliderMat},
            group::{collide_no_group, spawn_collider, ColliderGroup},
        },
        kinematic::{spawn_collision_checker, CollisionChecker, KinematicProps, Pos, Vel},
        rpc::{RpcClientHandle, RpcClientKind, RpcClientQuery},
    },
    try_sync,
    utils::math::{Aabb, HullCastRequest, RgbaColor, Segment},
};
use macroquad::{
    input::{
        is_key_down, is_key_pressed, is_mouse_button_pressed, mouse_position, KeyCode, MouseButton,
    },
    math::{FloatExt, Vec2},
};

use super::bullet::BulletTrailRenderer;

// === PlayerController === //

#[derive(Debug, Clone)]
pub struct PlayerController {
    last_heading: f32,
    camera: Obj<VirtualCamera>,
    owned_rpc: RpcClientHandle<PlayerOwnerRpcKind>,
    collider_group: Obj<ColliderGroup>,
    ground_checker: Obj<CollisionChecker>,
    on_ground_coyote_time: u8,
    on_jump_coyote_time: u8,
    jump_extend_time: u8,
}

component!(PlayerController);

impl PlayerController {
    pub fn is_on_ground(&self) -> bool {
        self.ground_checker.is_touching
    }
}

// === PlayerReplicator === //

#[derive(Debug)]
pub struct PlayerReplicator {
    rpc: RpcClientHandle<PlayerRpcKind>,
    rpc_kind: Option<PlayerReplicatorKind>,
    pos: Obj<Pos>,
}

component!(PlayerReplicator);

#[derive(Debug, Copy, Clone)]
enum PlayerReplicatorKind {
    Owner(RpcClientHandle<PlayerOwnerRpcKind>),
    Puppet(RpcClientHandle<PlayerPuppetRpcKind>),
}

impl RpcClientKind<PlayerRpcKind> for PlayerReplicator {}
impl RpcClientKind<PlayerOwnerRpcKind> for PlayerReplicator {}
impl RpcClientKind<PlayerPuppetRpcKind> for PlayerReplicator {}

// === Systems === //

pub fn sys_update_players() {
    // Handle RPCs
    for req in RpcClientQuery::<PlayerRpcKind>::new().added() {
        let me = Entity::new(req.client_ent());

        let pos = me.add(Pos(req.packet().pos));
        let collider_group = me.add(ColliderGroup::new());

        spawn_collider(
            collider_group,
            pos,
            Aabb::new_centered(Vec2::ZERO, Vec2::splat(50.0)),
            ColliderMask::ALL,
            ColliderMat::Solid,
        );

        let state = me.add(PlayerReplicator {
            rpc: req.rpc(),
            rpc_kind: None,
            pos,
        });

        me.add(SolidRenderer::new_centered(RgbaColor::RED, 50.));
        register_gfx(me);

        req.bind_userdata(state);
    }

    for req in RpcClientQuery::<PlayerOwnerRpcKind>::new().added() {
        let res = try_sync! {
            let mut camera = req.client_ent().get::<VirtualCameraSelector>();

            let mut replicator = req.packet_target::<PlayerReplicator>()?;
            let target = replicator.entity();
            anyhow::ensure!(replicator.rpc_kind.is_none(), "player already has a kind");
            replicator.rpc_kind = Some(PlayerReplicatorKind::Owner(req.rpc()));

            target
                .with(Vel::default())
                .with(KinematicProps {
                    gravity: Vec2::Y * 4000.,
                    friction: 0.98,
                })
                .with(PlayerController {
                    last_heading: 0.,
                    camera: camera.current().unwrap().entity().get(),
                    owned_rpc: req.rpc(),
                    collider_group: target.get(),
                    ground_checker: spawn_collision_checker(target.get(), Vec2::Y),
                    on_ground_coyote_time: 0,
                    on_jump_coyote_time: 0,
                    jump_extend_time: 0,
                });

            tracing::info!("{:?} is an owned player", req.packet());

            req.bind_userdata(replicator);
        };
        req.client().report_result(res);
    }

    for req in RpcClientQuery::<PlayerPuppetRpcKind>::new().added() {
        let res = try_sync! {
            let mut replicator = req.packet_target::<PlayerReplicator>()?;
            anyhow::ensure!(replicator.rpc_kind.is_none(), "player already has a kind");
            replicator.rpc_kind = Some(PlayerReplicatorKind::Puppet(req.rpc()));

            tracing::info!("{:?} is a puppet player", req.packet());

            req.bind_userdata(replicator);
        };
        req.client().report_result(res);
    }

    for req in RpcClientQuery::<PlayerRpcKind>::new().msgs() {
        match *req.packet() {}
    }

    for req in RpcClientQuery::<PlayerOwnerRpcKind>::new().msgs() {
        match *req.packet() {}
    }

    for req in RpcClientQuery::<PlayerPuppetRpcKind>::new().msgs() {
        let mut me = req.userdata::<PlayerReplicator>();

        match *req.packet() {
            PlayerPuppetRpcCb::SetPos(pos) => me.pos.0 = pos,
        }
    }

    for req in RpcClientQuery::<PlayerRpcKind>::new().removed() {
        req.userdata::<PlayerReplicator>().entity().destroy();
    }

    // Handle owned player updates
    for (pos, mut vel, mut player) in Query::<(Obj<Pos>, Obj<Vel>, Obj<PlayerController>)>::new() {
        // Determine desired heading
        let mut heading = 0.;

        if is_key_down(KeyCode::A) {
            heading -= 1.;
        }

        if is_key_down(KeyCode::D) {
            heading += 1.;
        }

        if player.is_on_ground() {
            player.on_ground_coyote_time = 8;
        } else {
            player.on_ground_coyote_time = player.on_ground_coyote_time.saturating_sub(1);
        }

        if is_key_pressed(KeyCode::Space) {
            player.on_jump_coyote_time = 8;
        } else {
            player.on_jump_coyote_time = player.on_jump_coyote_time.saturating_sub(1);
        }

        if player.on_jump_coyote_time > 0 && player.on_ground_coyote_time > 0 {
            player.on_jump_coyote_time = 0;
            player.on_ground_coyote_time = 0;
            player.jump_extend_time = 16;
        }

        if player.jump_extend_time > 0 && is_key_down(KeyCode::Space) {
            vel.physical.y = -1500.;
            player.jump_extend_time -= 1;
        } else {
            player.jump_extend_time = 0;
        }

        heading *= 2000.;

        // Compute actual heading
        let heading_strength = if player.is_on_ground() { 0.9 } else { 0.2 };
        player.last_heading = player.last_heading.lerp(heading, heading_strength);

        if player
            .collider_group
            .cast_hull(
                Vec2::X * player.last_heading.signum(),
                &mut collide_everything(),
            )
            .is_obstructed()
        {
            player.last_heading = 0.;
        }

        // Apply heading
        vel.artificial += player.last_heading * Vec2::X;

        // Send position to server.
        // TODO: Do this somewhere else after the position has been updated.
        player.owned_rpc.send(&PlayerOwnerRpcSb::SetPos(pos.0));

        if is_mouse_button_pressed(MouseButton::Left) {
            let start = pos.0;
            let end = player
                .camera
                .screen_to_world()
                .transform_point2(Vec2::from(mouse_position()));

            let bus = player.collider_group.expect_bus();
            let dir = (end - start).normalize_or_zero();
            let res = bus.cast_hull(
                HullCastRequest::new(Aabb::new_centered(pos.0, Vec2::splat(5.)), dir * 5000.),
                collide_no_group(player.collider_group),
            );

            player
                .entity()
                .deep_get::<BulletTrailRenderer>()
                .spawn(Instant::now(), Segment::new_delta(start, dir * res.dist));
        }
    }
}

pub fn sys_update_player_camera() {
    for (pos, player) in Query::<(Obj<Pos>, Obj<PlayerController>)>::new() {
        // Update camera
        player.camera.entity().get::<Pos>().0 = pos.0;
    }
}
