use std::f32;

pub fn lerp_f32(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

pub fn ilerp_f32(a: f32, b: f32, v: f32) -> f32 {
    (v - a) / (b - a)
}

#[derive(Debug, Copy, Clone)]
pub struct LogisticCurve {
    pub max_value: f32,
    pub midpoint: f32,
    pub steepness: f32,
}

impl LogisticCurve {
    pub fn compute(self, value: f32) -> f32 {
        self.max_value / (1.0 + f32::consts::E.powf(-self.steepness * (value - self.midpoint)))
    }
}
