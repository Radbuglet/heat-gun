use macroquad::{
    color::Color,
    math::Rect,
    shapes::{draw_rectangle, draw_rectangle_ex, DrawRectangleParams},
};

use crate::utils::lang::extension::Extends;

use super::Aabb;

pub trait MqAabbExt: Extends<Aabb> {
    fn to_rect(self) -> Rect;

    fn from_rect(rect: Rect) -> Self;

    fn draw_solid(self, color: Color);

    fn draw_solid_ex(self, params: DrawRectangleParams);
}

impl MqAabbExt for Aabb {
    fn to_rect(self) -> Rect {
        Rect::new(self.x(), self.y(), self.w(), self.h())
    }

    fn from_rect(rect: Rect) -> Self {
        Self::new(rect.x, rect.y, rect.w, rect.h)
    }

    fn draw_solid(self, color: Color) {
        draw_rectangle(self.x(), self.y(), self.w(), self.h(), color);
    }

    fn draw_solid_ex(self, params: DrawRectangleParams) {
        draw_rectangle_ex(self.x(), self.y(), self.w(), self.h(), params);
    }
}
