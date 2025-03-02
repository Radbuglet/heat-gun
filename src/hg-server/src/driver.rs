use hg_common::base::{
    mp::MpServer,
    net::quic_server::QuicServerTransport,
    rpc::{sys_flush_rpc_groups, sys_flush_rpc_server, RpcGroup, RpcServer},
    time::{tps_to_dt, RunLoop},
};
use hg_ecs::{bind, Entity, Obj, World};

use crate::game::player::{spawn_player, PlayerOwner};

pub fn world_init(world: &mut World, transport: QuicServerTransport) {
    bind!(world);

    let rpc = Entity::root().add(RpcServer::new());

    Entity::root()
        .with(RpcGroup::new())
        .with(MpServer::new(Box::new(transport), rpc))
        .with(RunLoop::new(tps_to_dt(60.)));
}

pub fn world_main_loop(world: &mut World) {
    bind!(world);

    let mut rl = Entity::service::<RunLoop>();

    loop {
        // Process tick
        world_tick();
        Entity::flush(|world| {
            bind!(world);
            world_flush();
        });

        // Wait for next tick
        if rl.should_exit() {
            break;
        }

        rl.wait_for_tick();
    }
}

fn world_tick() {
    let group = Entity::service::<RpcGroup>();
    let mp = Entity::service::<MpServer>();
    mp.process();

    for &sess in &mp.on_join() {
        let mut owner = sess.entity().add(PlayerOwner {
            peer: sess.peer(),
            sess,
            player: Obj::DANGLING,
        });
        let player = spawn_player(Entity::root(), owner);
        owner.player = player.get();
    }

    for &sess in &mp.on_join() {
        group.add_peer(sess.peer());
    }

    for &sess in &mp.on_quit() {
        PlayerOwner::downcast(sess.peer()).player.entity().destroy();
    }

    for &sess in &mp.on_quit() {
        group.remove_peer(sess.peer());
    }
}

fn world_flush() {
    sys_flush_rpc_server();
    sys_flush_rpc_groups();
}
