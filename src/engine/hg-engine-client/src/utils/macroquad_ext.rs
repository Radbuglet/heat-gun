use hg_engine_common::utils::{
    lang::Extends,
    math::{Aabb, Circle, RgbaColor, Segment},
};
use macroquad::{
    color::Color,
    math::Rect,
    shapes::{
        draw_circle, draw_line, draw_rectangle, draw_rectangle_ex, draw_rectangle_lines,
        draw_rectangle_lines_ex, DrawRectangleParams,
    },
};

// === MqColorExt === //

pub trait MqColorExt: Extends<RgbaColor> {
    fn to_macroquad(self) -> Color;

    fn from_macroquad(color: Color) -> Self;
}

impl MqColorExt for RgbaColor {
    fn to_macroquad(self) -> Color {
        Color::from_vec(self.vec())
    }

    fn from_macroquad(color: Color) -> Self {
        color.to_vec().into()
    }
}

// === MqAabbExt === //

pub trait MqAabbExt: Extends<Aabb> {
    fn to_macroquad(self) -> Rect;

    fn from_macroquad(rect: Rect) -> Self;

    fn draw_solid(self, color: RgbaColor);

    fn draw_solid_ex(self, params: DrawRectangleParams);

    fn draw_lines(self, thickness: f32, color: RgbaColor);

    fn draw_lines_ex(self, thickness: f32, params: DrawRectangleParams);
}

impl MqAabbExt for Aabb {
    fn to_macroquad(self) -> Rect {
        Rect::new(self.x(), self.y(), self.w(), self.h())
    }

    fn from_macroquad(rect: Rect) -> Self {
        Self::new(rect.x, rect.y, rect.w, rect.h)
    }

    fn draw_solid(self, color: RgbaColor) {
        draw_rectangle(self.x(), self.y(), self.w(), self.h(), color.to_macroquad());
    }

    fn draw_solid_ex(self, params: DrawRectangleParams) {
        draw_rectangle_ex(self.x(), self.y(), self.w(), self.h(), params);
    }

    fn draw_lines(self, thickness: f32, color: RgbaColor) {
        draw_rectangle_lines(
            self.x(),
            self.y(),
            self.w(),
            self.h(),
            thickness,
            color.to_macroquad(),
        );
    }

    fn draw_lines_ex(self, thickness: f32, params: DrawRectangleParams) {
        draw_rectangle_lines_ex(self.x(), self.y(), self.w(), self.h(), thickness, params);
    }
}

// === MqSegmentExt === //

pub trait MqSegmentExt: Extends<Segment> {
    fn draw(self, thickness: f32, color: RgbaColor);
}

impl MqSegmentExt for Segment {
    fn draw(self, thickness: f32, color: RgbaColor) {
        draw_line(
            self.x1(),
            self.y1(),
            self.x2(),
            self.y2(),
            thickness,
            color.to_macroquad(),
        );
    }
}

// === MqCircleExt === //

pub trait MqCircleExt: Extends<Circle> {
    fn draw(self, color: RgbaColor);
}

impl MqCircleExt for Circle {
    fn draw(self, color: RgbaColor) {
        draw_circle(
            self.origin.x,
            self.origin.y,
            self.radius,
            color.to_macroquad(),
        );
    }
}
