use std::time::{Duration, Instant};

use hg_ecs::component;
use hg_engine_client::utils::macroquad_ext::{MqCircleExt, MqSegmentExt};
use hg_engine_common::utils::math::{Circle, RgbaColor, Segment};
use macroquad::math::{FloatExt, Vec2};

// === Components === //

#[derive(Debug, Default)]
pub struct BulletTrailRenderer {
    trails: Vec<Trail>,
    explosions: Vec<Explosion>,
}

#[derive(Debug)]
struct Trail {
    segment: Segment,
    start: Instant,
}

impl Trail {
    fn lerp_percent(&self, now: Instant) -> f32 {
        let duration = now - self.start;
        let ups = 10000.;

        duration.as_secs_f32() * ups / self.segment.len()
    }
}

#[derive(Debug)]
struct Explosion {
    pos: Vec2,
    start: Instant,
}

component!(BulletTrailRenderer);

impl BulletTrailRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self, now: Instant, segment: Segment) {
        self.trails.push(Trail {
            segment,
            start: now,
        });
    }

    pub fn render(&mut self, now: Instant) {
        self.trails.retain(|trail| {
            let lerp = trail.lerp_percent(now).min(1.);

            if now - trail.start < Duration::from_millis(10) {
                return true;
            }

            let start = trail.lerp_percent(now - Duration::from_millis(10)).max(0.);
            let start = trail.segment.lerp(start);
            let end = trail.segment.lerp(lerp);

            Segment::new_points(start, end).draw(10., RgbaColor::ORANGE);

            let still_alive = lerp < 1.;
            if !still_alive {
                self.explosions.push(Explosion {
                    pos: trail.segment.end,
                    start: Instant::now(),
                });
            }

            still_alive
        });

        self.explosions.retain(|explosion| {
            let lerp = ((now - explosion.start).as_secs_f32() * 10.).min(1.);

            Circle {
                origin: explosion.pos,
                radius: (30.).lerp(0., lerp),
            }
            .draw(RgbaColor::WHITE);

            lerp < 1.
        });
    }
}
