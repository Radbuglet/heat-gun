use std::time::{Duration, Instant};

use hg_common::utils::math::{RgbaColor, Segment};
use hg_ecs::component;

use crate::utils::macroquad_ext::MqSegmentExt;

// === Components === //

#[derive(Debug, Default)]
pub struct BulletTrailRenderer {
    trails: Vec<Trail>,
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
            let curr_lerp = trail.lerp_percent(now);

            if now - trail.start < Duration::from_millis(10) {
                return true;
            }

            let start = trail.lerp_percent(now - Duration::from_millis(10)).max(0.);
            let end = curr_lerp.min(1.);
            let start = trail.segment.lerp(start);
            let end = trail.segment.lerp(end);

            Segment::new_points(start, end).draw(10., RgbaColor::ORANGE);
            curr_lerp <= 1.
        });
    }
}
