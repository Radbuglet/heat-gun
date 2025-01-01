use hg_ecs::{component, Obj};
use macroquad::{color::Color, math::Rect, shapes::draw_rectangle};

use super::kinematic::Pos;

#[derive(Debug, Clone)]
pub struct SolidRenderer {
    pub color: Color,
    pub rect: Rect,
}

impl SolidRenderer {
    pub fn new_centered(color: Color, size: f32) -> Self {
        Self {
            color,
            rect: Rect::new(-size / 2., -size / 2., size, size),
        }
    }

    pub fn render(self: Obj<Self>) {
        let pos = self.entity().get::<Pos>();
        let rect = self.rect.offset(pos.0);

        draw_rectangle(rect.x, rect.y, rect.w, rect.h, self.color);
    }
}

component!(SolidRenderer);
