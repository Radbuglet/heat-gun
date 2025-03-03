use hg_common::{
    base::{
        collide::{
            bus::{register_collider, Collider, ColliderMask, ColliderMat},
            update::ColliderFollows,
        },
        kinematic::{KinematicProps, Pos, Vel},
        rpc::{RpcClientCb, RpcClientCup, RpcClientHandle, RpcClientReplicator},
    },
    game::player::{PlayerOwnerRpcKind, PlayerPuppetRpcCb, PlayerPuppetRpcKind, PlayerRpcKind},
    utils::math::{Aabb, RgbaColor},
};
use hg_ecs::{bind, component, Entity, Obj, Query, World};
use macroquad::{
    input::{is_key_down, is_key_pressed, KeyCode},
    math::{FloatExt, Vec2},
};

use crate::base::gfx::{bus::register_gfx, sprite::SolidRenderer};

// === PlayerController === //

#[derive(Debug, Clone)]
pub struct PlayerController {
    last_heading: f32,
    camera: Obj<Pos>,
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

impl RpcClientReplicator for PlayerReplicator {
    type Kind = PlayerRpcKind;

    fn create(
        world: &mut World,
        rpc: RpcClientHandle<Self::Kind>,
        packet: RpcClientCup<Self>,
    ) -> anyhow::Result<Obj<Self>> {
        bind!(world);

        let me = rpc.entity();

        let pos = me.add(Pos(packet.pos));
        me.add(SolidRenderer::new_centered(RgbaColor::RED, 50.));

        register_gfx(me);

        Ok(me.add(Self {
            rpc,
            rpc_kind: None,
            pos,
        }))
    }

    fn process(
        self: Obj<Self>,
        world: &mut World,
        packet: RpcClientCb<Self>,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {}
    }
}

#[derive(Debug)]
pub struct PlayerPuppetReplicator {
    target: Obj<PlayerReplicator>,
}

component!(PlayerPuppetReplicator);

impl RpcClientReplicator for PlayerPuppetReplicator {
    type Kind = PlayerPuppetRpcKind;

    fn create(
        world: &mut World,
        rpc: RpcClientHandle<Self::Kind>,
        packet: RpcClientCup<Self>,
    ) -> anyhow::Result<Obj<Self>> {
        bind!(world);

        let mut target = rpc.client().lookup_node::<PlayerReplicator>(packet)?;

        anyhow::ensure!(
            target.rpc_kind.is_none(),
            "parent already has a kind replicator"
        );

        target.rpc_kind = Some(PlayerReplicatorKind::Puppet(rpc));

        Ok(rpc.entity().add(Self { target }))
    }

    fn process(
        mut self: Obj<Self>,
        world: &mut World,
        packet: RpcClientCb<Self>,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {
            PlayerPuppetRpcCb::SetPos(pos) => {
                self.target.pos.0 = pos;
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct PlayerOwnerReplicator {
    target: Obj<PlayerReplicator>,
}

component!(PlayerOwnerReplicator);

impl RpcClientReplicator for PlayerOwnerReplicator {
    type Kind = PlayerOwnerRpcKind;

    fn create<'t>(
        world: &mut World,
        rpc: RpcClientHandle<Self::Kind>,
        packet: RpcClientCup<Self>,
    ) -> anyhow::Result<Obj<Self>> {
        bind!(world);

        let mut target = rpc.client().lookup_node::<PlayerReplicator>(packet)?;

        anyhow::ensure!(
            target.rpc_kind.is_none(),
            "parent already has a kind replicator"
        );

        target.rpc_kind = Some(PlayerReplicatorKind::Owner(rpc));

        Ok(rpc.entity().add(Self { target }))
    }

    fn process(
        self: Obj<Self>,
        world: &mut World,
        packet: RpcClientCb<Self>,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {}
    }
}

// === Prefabs === //

pub fn spawn_player(parent: Entity, camera: Entity) -> Entity {
    let player = Entity::new(parent)
        .with(Pos::default())
        .with(Vel::default())
        .with(KinematicProps {
            gravity: Vec2::Y * 4000.,
            friction: 0.98,
        })
        .with(PlayerController {
            last_heading: 0.,
            camera: camera.get(),
        })
        .with(SolidRenderer::new_centered(RgbaColor::RED, 50.))
        .with(Collider::new(ColliderMask::ALL, ColliderMat::Solid));

    player.with(ColliderFollows {
        target: player.get(),
        aabb: Aabb::new_centered(Vec2::ZERO, Vec2::splat(50.)),
    });

    register_gfx(player);
    register_collider(player.get());

    player
}

// === Systems === //

pub fn sys_update_players() {
    for (mut vel, mut player) in Query::<(Obj<Vel>, Obj<PlayerController>)>::new() {
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
    }
}

pub fn sys_update_player_camera() {
    for (pos, mut player) in Query::<(Obj<Pos>, Obj<PlayerController>)>::new() {
        // Update camera
        player.camera.0 = pos.0;
    }
}
