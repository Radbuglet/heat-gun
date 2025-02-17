#![feature(arbitrary_self_types)]
#![feature(context_injection)]

use std::panic::{catch_unwind, AssertUnwindSafe};

use anyhow::Context as _;
use driver::{world_init, world_tick};
use hg_common::utils::lang::catch_termination;
use hg_ecs::World;
use macroquad::{input::is_quit_requested, window::next_frame};
use tokio::runtime::Runtime;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

pub mod base;
pub mod driver;
pub mod game;
pub mod utils;

#[macroquad::main("Heat Gun")]
async fn main() {
    let runtime = match catch_termination(early_init) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Early-initialization failed:\n{err:?}");
            return;
        }
    };

    let _runtime_guard = runtime.enter();

    let Ok(mut world) = catch_unwind(AssertUnwindSafe(|| {
        let mut world = World::new();
        world_init(&mut world);
        world
    })) else {
        return;
    };

    while !is_quit_requested() {
        let crashed = catch_unwind(AssertUnwindSafe(|| {
            world_tick(&mut world);
        }))
        .is_err();

        if crashed {
            return;
        }

        next_frame().await;
    }
}

fn early_init() -> anyhow::Result<Runtime> {
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

    let runtime = Runtime::new()?;

    Ok(runtime)
}
