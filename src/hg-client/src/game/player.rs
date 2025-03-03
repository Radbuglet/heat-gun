use hg_common::{
    base::{
        collide::{
            bus::{register_collider, Collider, ColliderMask, ColliderMat},
            update::ColliderFollows,
        },
        kinematic::{KinematicProps, Pos, Vel},
        rpc::{RpcClientHandle, RpcClientReplicator, RpcNodeId},
    },
    game::player::{
        PlayerOwnerRpcCb, PlayerOwnerRpcKind, PlayerPuppetRpcCb, PlayerPuppetRpcKind,
        PlayerRpcCatchup, PlayerRpcCb, PlayerRpcKind,
    },
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

impl RpcClientReplicator<PlayerRpcKind> for PlayerReplicator {
    fn create(
        world: &mut World,
        rpc: RpcClientHandle<PlayerRpcKind>,
        packet: PlayerRpcCatchup,
    ) -> anyhow::Result<Obj<Self>> {
        bind!(world);

        let me = Entity::new(rpc.client().entity());
        let pos = me.add(Pos(packet.pos));
        let state = me.add(PlayerReplicator {
            rpc,
            rpc_kind: None,
            pos,
        });

        Ok(state)
    }

    fn process(self: Obj<Self>, world: &mut World, packet: PlayerRpcCb) -> anyhow::Result<()> {
        bind!(world);

        match packet {}
    }

    fn destroy(self: Obj<Self>, world: &mut World) -> anyhow::Result<()> {
        bind!(world);
        self.entity().destroy();
        Ok(())
    }
}

impl RpcClientReplicator<PlayerOwnerRpcKind> for PlayerReplicator {
    fn create(
        world: &mut World,
        rpc: RpcClientHandle<PlayerOwnerRpcKind>,
        packet: RpcNodeId,
    ) -> anyhow::Result<Obj<Self>> {
        bind!(world);

        let mut me = rpc.client().lookup_node::<Self>(packet)?;
        anyhow::ensure!(me.rpc_kind.is_none(), "player already has kind replicator");
        me.rpc_kind = Some(PlayerReplicatorKind::Owner(rpc));

        tracing::info!("became owner of {:?}", me.entity().debug());

        Ok(me)
    }

    fn process(self: Obj<Self>, world: &mut World, packet: PlayerOwnerRpcCb) -> anyhow::Result<()> {
        bind!(world);

        match packet {}
    }

    fn destroy(self: Obj<Self>, _world: &mut World) -> anyhow::Result<()> {
        Ok(())
    }
}

impl RpcClientReplicator<PlayerPuppetRpcKind> for PlayerReplicator {
    fn create(
        world: &mut World,
        rpc: RpcClientHandle<PlayerPuppetRpcKind>,
        packet: RpcNodeId,
    ) -> anyhow::Result<Obj<Self>> {
        bind!(world);

        let mut me = rpc.client().lookup_node::<Self>(packet)?;
        anyhow::ensure!(me.rpc_kind.is_none(), "player already has kind replicator");
        me.rpc_kind = Some(PlayerReplicatorKind::Puppet(rpc));

        Ok(me)
    }

    fn process(
        mut self: Obj<Self>,
        world: &mut World,
        packet: PlayerPuppetRpcCb,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {
            PlayerPuppetRpcCb::SetPos(pos) => {
                self.pos.0 = pos;
            }
        }

        Ok(())
    }

    fn destroy(self: Obj<Self>, _world: &mut World) -> anyhow::Result<()> {
        Ok(())
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
