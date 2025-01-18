use macroquad::math::Vec2;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Circle {
    pub origin: Vec2,
    pub radius: f32,
}

impl Circle {
    pub const fn new(origin: Vec2, radius: f32) -> Self {
        Self { origin, radius }
    }
}
