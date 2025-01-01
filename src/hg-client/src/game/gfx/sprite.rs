use hg_ecs::{component, Obj};
use macroquad::{color::Color, math::Vec2};

use crate::{
    game::kinematic::Pos,
    utils::math::{Aabb, MqAabbExt},
};

#[derive(Debug, Clone)]
pub struct SolidRenderer {
    pub color: Color,
    pub aabb: Aabb,
}

impl SolidRenderer {
    pub fn new_centered(color: Color, size: f32) -> Self {
        Self {
            color,
            aabb: Aabb::new_centered(Vec2::ZERO, Vec2::splat(size)),
        }
    }

    pub fn render(self: Obj<Self>) {
        let pos = self.entity().get::<Pos>();

        self.aabb.translated(pos.0).draw_solid(self.color);
    }
}

component!(SolidRenderer);
