use glam::Vec2;

use super::HullCastResult;

pub const SAFETY_THRESHOLD: f32 = 0.001;

#[derive(Debug)]
pub struct MoveAndSlide {
    pub steps: u32,
    pub max_steps: u32,
    pub remaining_delta: Vec2,
}

impl MoveAndSlide {
    pub fn new(steps: u32, delta: Vec2) -> Self {
        Self {
            steps,
            max_steps: steps,
            remaining_delta: delta,
        }
    }

    pub fn should_step(&self) -> bool {
        self.steps > 0 && self.remaining_delta.length_squared() > 0.001_f32.powi(2)
    }

    pub fn next_delta(&self) -> Option<Vec2> {
        self.should_step().then_some(self.remaining_delta)
    }

    pub fn update(&mut self, result: HullCastResult) {
        self.remaining_delta *= 1.0 - result.percent;

        if let Some(normal) = result.normal {
            self.remaining_delta = cancel_normal(self.remaining_delta, normal);
        }
    }
}

pub fn cancel_normal(vel: Vec2, normal: Vec2) -> Vec2 {
    // We would like to add some multiple of `normal` to `vel` to make it such that the dot-product
    // between `vel` and `normal` is zero or positive since a negative dot-product would indicate
    // that `vel` is pointing into the surface of interest.

    // (vel + k * normal) • normal >= 0
    // (vel • normal) + (k * normal • normal) >= 0
    // (vel • normal) + k * (normal • normal) >= 0
    // (vel • normal) + k >= 0
    // k >= -(vel • normal)
    //
    // See: https://www.desmos.com/calculator/4xxytkoxqn

    let k = -vel.dot(normal);
    let k = k.max(0.);

    vel + k * normal
}
