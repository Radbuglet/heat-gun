use hg_common::{
    base::kinematic::Pos,
    utils::math::{Aabb, RgbaColor},
};
use hg_ecs::{component, Obj};
use macroquad::math::Vec2;

use crate::utils::macroquad_ext::MqAabbExt as _;

#[derive(Debug, Clone)]
pub struct SolidRenderer {
    pub color: RgbaColor,
    pub aabb: Aabb,
}

impl SolidRenderer {
    pub fn new_centered(color: RgbaColor, size: f32) -> Self {
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
