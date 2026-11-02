//! Orbit camera (legacy `Renderer::Move*/Rotate*/Zoom*`).

use macroquad::prelude::*;

pub struct OrbitCamera {
    /// Point looked at, in tile units (x = w, y = height, z = l).
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl OrbitCamera {
    pub fn new(width: u32, length: u32) -> Self {
        let span = width.max(length) as f32;
        Self { target: vec3(width as f32 / 2.0, 0.0, length as f32 / 2.0), yaw: 45f32.to_radians(), pitch: 45f32.to_radians(), distance: span * 0.6 }
    }

    pub fn camera(&self) -> Camera3D {
        let offset = vec3(self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), self.pitch.cos() * self.yaw.cos()) * self.distance;
        Camera3D { position: self.target + offset, target: self.target, up: Vec3::Y, fovy: 45f32.to_radians(), ..Default::default() }
    }

    /// Pans relative to the view direction.
    pub fn pan(&mut self, right: f32, forward: f32, bounds: Vec2) {
        let fwd = vec3(-self.yaw.sin(), 0.0, -self.yaw.cos());
        let side = vec3(fwd.z, 0.0, -fwd.x) * -1.0;
        let speed = self.distance * 0.02;
        self.target += (side * right + fwd * forward) * speed;
        self.target.x = self.target.x.clamp(0.0, bounds.x);
        self.target.z = self.target.z.clamp(0.0, bounds.y);
    }

    pub fn rotate(&mut self, yaw: f32, pitch: f32) {
        self.yaw += yaw;
        self.pitch = (self.pitch + pitch).clamp(10f32.to_radians(), 89f32.to_radians());
    }

    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(4.0, 300.0);
    }

    /// Keyboard and mouse navigation. Returns true when the view moved.
    pub fn handle_input(&mut self, bounds: Vec2, mouse_delta: Vec2, ui_has_mouse: bool) -> bool {
        let dt = get_frame_time().min(0.1) * 60.0;
        let fast = if is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::LeftShift) { 4.0 } else { 1.0 };
        let mut moved = false;
        let mut key = |k: KeyCode| is_key_down(k).then(|| moved = true).is_some();
        let (mut right, mut fwd, mut turn, mut tilt, mut zoom) = (0.0, 0.0, 0.0, 0.0, 1.0);
        if key(KeyCode::Left) || key(KeyCode::A) {
            right -= 1.0;
        }
        if key(KeyCode::Right) || key(KeyCode::D) {
            right += 1.0;
        }
        if key(KeyCode::Up) || key(KeyCode::W) {
            fwd += 1.0;
        }
        if key(KeyCode::Down) || key(KeyCode::S) {
            fwd -= 1.0;
        }
        if key(KeyCode::PageUp) || key(KeyCode::Q) {
            turn -= 0.03;
        }
        if key(KeyCode::PageDown) || key(KeyCode::E) {
            turn += 0.03;
        }
        if key(KeyCode::Home) {
            tilt += 0.02;
        }
        if key(KeyCode::End) {
            tilt -= 0.02;
        }
        if key(KeyCode::Insert) || key(KeyCode::Equal) || key(KeyCode::KpAdd) {
            zoom *= 0.97;
        }
        if key(KeyCode::Delete) || key(KeyCode::Minus) || key(KeyCode::KpSubtract) {
            zoom *= 1.03;
        }
        self.pan(right * fast * dt, fwd * fast * dt, bounds);
        self.rotate(turn * dt, tilt * dt);
        self.zoom(zoom);
        if !ui_has_mouse {
            let wheel = mouse_wheel().1;
            if wheel != 0.0 {
                self.zoom(if wheel > 0.0 { 0.9 } else { 1.1 });
                moved = true;
            }
            if is_mouse_button_down(MouseButton::Right) {
                self.rotate(-mouse_delta.x * 0.008, mouse_delta.y * 0.006);
                moved = true;
            }
            if is_mouse_button_down(MouseButton::Middle) {
                self.pan(-mouse_delta.x * 0.08, mouse_delta.y * 0.08, bounds);
                moved = true;
            }
        }
        moved
    }
}
