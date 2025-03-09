use hg_common::{
    base::{
        collide::{
            bus::{register_collider, Collider, ColliderMask, ColliderMat},
            update::ColliderFollows,
        },
        kinematic::{KinematicProps, Pos, Vel},
        rpc::{RpcClientHandle, RpcClientKind, RpcClientQuery},
    },
    game::player::{
        PlayerOwnerRpcKind, PlayerOwnerRpcSb, PlayerPuppetRpcCb, PlayerPuppetRpcKind, PlayerRpcKind,
    },
    try_sync,
    utils::math::{Aabb, RgbaColor},
};
use hg_ecs::{component, Entity, Obj, Query};
use macroquad::{
    input::{is_key_down, is_key_pressed, KeyCode},
    math::{FloatExt, Vec2},
};

use crate::base::gfx::{bus::register_gfx, camera::VirtualCameraSelector, sprite::SolidRenderer};

// === PlayerController === //

#[derive(Debug, Clone)]
pub struct PlayerController {
    last_heading: f32,
    camera: Obj<Pos>,
    owned_rpc: RpcClientHandle<PlayerOwnerRpcKind>,
}

component!(PlayerController);

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
                })
                .with(Collider::new(ColliderMask::ALL, ColliderMat::Solid))
                .with(ColliderFollows {
                    target: target.get(),
                    aabb: Aabb::new_centered(Vec2::ZERO, Vec2::splat(50.)),
                });

            register_collider(target.get());

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

        if is_key_pressed(KeyCode::Space) {
            vel.physical.y = -2000.;
        }

        heading *= 2000.;

        // Compute actual heading
        player.last_heading = player.last_heading.lerp(heading, 0.9);

        // Apply heading
        vel.artificial += player.last_heading * Vec2::X;

        // Send position to server.
        // TODO: Do this somewhere else after the position has been updated.
        player.owned_rpc.send(&PlayerOwnerRpcSb::SetPos(pos.0));
    }
}

pub fn sys_update_player_camera() {
    for (pos, mut player) in Query::<(Obj<Pos>, Obj<PlayerController>)>::new() {
        // Update camera
        player.camera.0 = pos.0;
    }
}
