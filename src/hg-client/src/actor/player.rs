use hg_ecs::{component, Entity, Obj};
use macroquad::{
    color::RED,
    input::{is_key_pressed, KeyCode},
    math::Vec2,
    shapes::draw_rectangle,
    time::get_frame_time,
};

pub struct Player {
    pub world: Entity,
    pub pos: Vec2,
    pub vel: Vec2,
}

component!(Player);

impl Player {
    pub fn new(owner: Entity) -> Self {
        Self {
            world: owner,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
        }
    }

    pub fn update(mut self: Obj<Self>) {
        let me = &mut *self;
        let dt = get_frame_time();

        me.vel += Vec2::Y * 1000. * dt;
        me.pos += me.vel * dt;

        if is_key_pressed(KeyCode::Space) {
            me.vel = Vec2::NEG_Y * 500.;
        }
    }

    pub fn render(self: Obj<Self>) {
        draw_rectangle(self.pos.x, self.pos.y, 100., 100., RED);
    }
}
