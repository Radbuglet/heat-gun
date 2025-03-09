#![feature(arbitrary_self_types)]
#![feature(context_injection)]

use anyhow::Context;
use driver::{world_init, world_main_loop};
use hg_ecs::World;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

pub mod base;
pub mod driver;
pub mod game;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_span_events(FmtSpan::CLOSE)
        .init();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok()
        .context("failed to install crypto provider")?;

    // Create world
    let mut world = World::new();
    world_init(&mut world)?;
    world_main_loop(&mut world);

    Ok(())
}
