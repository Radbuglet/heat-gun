//! A run loop in the vane of [this classic post](https://gafferongames.com/post/fix_your_timestep/).

use std::{
    thread,
    time::{Duration, Instant},
};

use hg_ecs::component;

#[derive(Debug)]
pub struct RunLoop {
    should_exit: bool,
    last_tick: Instant,
    tick_dt: Duration,
}

component!(RunLoop);

impl RunLoop {
    pub fn new(tick_dt: Duration) -> Self {
        Self {
            should_exit: false,
            last_tick: Instant::now(),
            tick_dt,
        }
    }

    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    pub fn tick_dt(&self) -> Duration {
        self.tick_dt
    }

    pub fn set_tick_dt(&mut self, dt: Duration) {
        self.tick_dt = dt;
    }

    pub fn wait_for_tick(&mut self) {
        loop {
            let now = Instant::now();
            let elapsed = now - self.last_tick;

            if elapsed > self.tick_dt {
                self.last_tick = now;
                break;
            }

            thread::sleep(now - self.last_tick + self.tick_dt);
        }
    }
}

pub fn tps_to_dt(tps: f64) -> Duration {
    Duration::from_secs_f64(1. / tps)
}
