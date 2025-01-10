use hg_ecs::{component, Obj, Query};
use macroquad::{
    camera::{pop_camera_state, push_camera_state, set_camera, Camera},
    math::{Mat4, Vec2},
    miniquad::window::screen_size,
    texture::RenderPass,
};

use crate::{base::kinematic::Pos, utils::math::Aabb};

// === VirtualCameraSelector === //

#[derive(Debug, Clone, Default)]
pub struct VirtualCameraSelector {
    current: Option<Obj<VirtualCamera>>,
}

component!(VirtualCameraSelector);

impl VirtualCameraSelector {
    pub fn current(&mut self) -> Option<Obj<VirtualCamera>> {
        let current = self.current?;

        if !Obj::is_alive(current) {
            self.current = None;
            return None;
        }

        Some(current)
    }

    pub fn set_current(&mut self, camera: Obj<VirtualCamera>) {
        self.current = Some(camera);
    }
}

// === VirtualCamera === //

#[derive(Debug, Clone, Default)]
pub struct VirtualCamera {
    focus: Option<Aabb>,
}

component!(VirtualCamera);

impl VirtualCamera {
    pub fn reset(&mut self) {
        self.focus = None;
    }

    pub fn focus(&self) -> Aabb {
        self.focus.expect("focus never set")
    }

    pub fn set_focus(&mut self, aabb: Aabb) {
        assert!(self.focus.is_none(), "focus already set");
        self.focus = Some(aabb);
    }

    pub fn bind(&self) -> VirtualCameraGuard {
        let guard = VirtualCameraGuard::new();
        set_camera(self);
        guard
    }
}

impl Camera for VirtualCamera {
    fn matrix(&self) -> Mat4 {
        let focus = self.focus();
        let center = focus.center();
        let size = focus.size() * Vec2::new(1., -1.);

        (Mat4::from_translation(center.extend(0.)) * Mat4::from_scale(size.extend(1.))).inverse()
    }

    fn depth_enabled(&self) -> bool {
        true
    }

    fn render_pass(&self) -> Option<RenderPass> {
        None
    }

    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct VirtualCameraGuard;

impl VirtualCameraGuard {
    pub fn new() -> Self {
        push_camera_state();
        Self
    }
}

impl Drop for VirtualCameraGuard {
    fn drop(&mut self) {
        pop_camera_state();
    }
}

// === VirtualCameraKeepArea === //

#[derive(Debug, Copy, Clone, Default)]
pub struct CameraKeepArea(pub f32);

impl CameraKeepArea {
    pub fn new(size: Vec2) -> Self {
        Self(size.x * size.y)
    }
}

component!(CameraKeepArea);

// === Systems === //

pub fn sys_update_virtual_cameras() {
    let screen_size = Vec2::from(screen_size());

    // Reset all cameras
    for mut camera in Query::<Obj<VirtualCamera>>::new() {
        camera.reset();
    }

    // Apply keep-area constraints
    for (pos, keep_area, mut camera) in
        Query::<(Obj<Pos>, Obj<CameraKeepArea>, Obj<VirtualCamera>)>::new()
    {
        let center = pos.0;

        // (screen_size.x * screen_scale) * (screen_size.y * screen_scale) = keep_area.0
        // screen_scale = sqrt(keep_area.0 / (screen_size.x * screen_size.y))
        let screen_scale = (keep_area.0 / (screen_size.x * screen_size.y)).sqrt();

        let size = screen_size * screen_scale;

        camera.set_focus(Aabb::new_centered(center, size));
    }
}
