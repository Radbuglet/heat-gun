use std::{net::SocketAddr, str::FromStr as _, sync::Arc};

use anyhow::Context as _;
use hg_ecs::{bind, Entity, Obj, World};
use hg_engine_common::base::{
    mp::MpServer,
    net::{generate_dev_priv_key, quic_server::QuicServerTransport},
    rpc::{sys_flush_rpc_groups, sys_flush_rpc_server, RpcServer},
    time::{tps_to_dt, RunLoop},
};
use quinn::crypto::rustls::QuicServerConfig;

use crate::game::player::{spawn_player, PlayerOwner};

pub fn world_init(world: &mut World) -> anyhow::Result<()> {
    bind!(world);

    // Setup crypto
    let (dev_key, dev_cert) = generate_dev_priv_key()?;
    let crypto = rustls::ServerConfig::builder()
        // Clients do not identify themselves through certificates.
        .with_no_client_auth()
        // Identify ourselves with the private key and self-signed certificate we just generated.
        .with_single_cert(vec![dev_cert], dev_key)
        .context("failed to create server TLS config")?;

    let crypto = QuicServerConfig::try_from(crypto).context("failed to create QUIC crypto")?;
    let crypto = Arc::new(crypto);

    // Setup server
    let bind_addr = SocketAddr::from_str("127.0.0.1:8080").unwrap();
    let config = quinn::ServerConfig::with_crypto(crypto);
    let transport = QuicServerTransport::new(config, bind_addr);

    // Setup engine root
    let rpc = Entity::root().add(RpcServer::new());

    Entity::root()
        .with(MpServer::new(Entity::root(), Box::new(transport), rpc))
        .with(RunLoop::new(tps_to_dt(60.)));

    Ok(())
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

    for &sess in &mp.on_quit() {
        PlayerOwner::downcast(sess.peer()).player.entity().destroy();
    }
}

fn world_flush() {
    sys_flush_rpc_server();
    sys_flush_rpc_groups();
}
