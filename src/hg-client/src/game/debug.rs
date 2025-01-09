use hg_ecs::{Entity, Obj, Query};
use macroquad::input::{is_key_pressed, KeyCode};

use super::player::PlayerController;

pub fn sys_update_debug() {
    if is_key_pressed(KeyCode::K) {
        for (player, _obj) in Query::<(Entity, Obj<PlayerController>)>::new() {
            player.destroy();
        }
    }
}
