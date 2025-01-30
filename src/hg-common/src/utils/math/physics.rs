use glam::Vec2;

pub const SAFETY_THRESHOLD: f32 = 0.001;

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
