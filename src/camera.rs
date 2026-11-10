//! A camera circling the board: dragging turns it, the mouse wheel moves it closer or away.

use crate::chess::Side;
use crate::math::{Mat4, PI, Vec3, vec3};

/// The vertical field of view, in radians.
const FIELD_OF_VIEW: f32 = 0.72;
const NEAR: f32 = 0.05;
const FAR: f32 = 90.0;
const START_PITCH: f32 = 0.68;
const START_DISTANCE: f32 = 10.8;
const RADIANS_PER_PIXEL: f32 = 0.008;
const TARGET: Vec3 = vec3(0.0, 0.45, 0.0);

pub struct Camera {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Camera {
    /// A camera on White's side of the board.
    pub fn new() -> Camera {
        Camera { yaw: PI, pitch: START_PITCH, distance: START_DISTANCE }
    }

    /// Looks at the board from the side of the given player.
    pub fn reset(&mut self, side: Side) {
        *self = Camera { yaw: if side == Side::White { PI } else { 0.0 }, ..Camera::new() };
    }

    /// Turns by a drag of the mouse, in logical pixels.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * RADIANS_PER_PIXEL;
        self.pitch = (self.pitch + dy * RADIANS_PER_PIXEL).clamp(0.18, 1.32);
    }

    /// A notch of the mouse wheel forward moves the camera a tenth closer, a notch back a tenth away; a wheel
    /// that turns smoothly reports parts of a notch.
    pub fn zoom(&mut self, notches: f32) {
        let step: f32 = if notches > 0.0 { 0.9 } else { 1.1 };
        self.distance = (self.distance * step.powf(notches.abs())).clamp(6.2, 20.0);
    }

    pub fn eye(&self) -> Vec3 {
        let horizontal = self.pitch.cos();
        let direction = vec3(self.yaw.sin() * horizontal, self.pitch.sin(), self.yaw.cos() * horizontal);
        TARGET + direction * self.distance
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.eye(), TARGET, Vec3::Y)
    }

    pub fn projection(aspect: f32) -> Mat4 {
        Mat4::perspective(FIELD_OF_VIEW, aspect, NEAR, FAR)
    }

    /// Where the ray through a point of a window of the given size meets the horizontal plane at the height
    /// `plane_y`.
    pub fn pick(&self, x: f32, y: f32, width: f32, height: f32, plane_y: f32) -> Option<Vec3> {
        let eye = self.eye();
        let forward = (TARGET - eye).normalized();
        let right = forward.cross(Vec3::Y).normalized();
        let up = right.cross(forward);

        let tan = (FIELD_OF_VIEW / 2.0).tan();
        let (width, height) = (width.max(1.0), height.max(1.0));
        let across = (2.0 * x / width - 1.0) * (width / height) * tan;
        let upward = (1.0 - 2.0 * y / height) * tan;
        let ray = (forward + right * across + up * upward).normalized();

        if ray.y.abs() < 1e-4 {
            return None;
        }
        let distance = (plane_y - eye.y) / ray.y;
        (distance > 0.0).then(|| eye + ray * distance)
    }
}
