//! Vectors and matrices. A matrix multiplies column vectors and is stored by columns, as OpenGL reads it.

use std::ops::{Add, Mul, Neg, Sub};

pub const PI: f32 = std::f32::consts::PI;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const X: Vec3 = vec3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = vec3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = vec3(0.0, 0.0, 1.0);

    pub fn dot(self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Vec3) -> Vec3 {
        vec3(self.y * other.z - self.z * other.y, self.z * other.x - self.x * other.z, self.x * other.y - self.y * other.x)
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// The same direction at a length of one; a vector without a length stays as it is.
    pub fn normalized(self) -> Vec3 {
        let length = self.length();
        if length > 0.0 { self * (1.0 / length) } else { self }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, other: Vec3) -> Vec3 {
        vec3(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, other: Vec3) -> Vec3 {
        vec3(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, scale: f32) -> Vec3 {
        vec3(self.x * scale, self.y * scale, self.z * scale)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [f32; 16],
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 { m: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };

    const fn set(&mut self, row: usize, column: usize, value: f32) {
        self.m[column * 4 + row] = value;
    }

    pub const fn translation(t: Vec3) -> Mat4 {
        let mut result = Mat4::IDENTITY;
        result.set(0, 3, t.x);
        result.set(1, 3, t.y);
        result.set(2, 3, t.z);
        result
    }

    pub const fn scale(s: Vec3) -> Mat4 {
        let mut result = Mat4::IDENTITY;
        result.set(0, 0, s.x);
        result.set(1, 1, s.y);
        result.set(2, 2, s.z);
        result
    }

    /// A turn about the vertical axis, counterclockwise seen from above.
    pub fn rotation_y(angle: f32) -> Mat4 {
        let (sin, cos) = angle.sin_cos();
        let mut result = Mat4::IDENTITY;
        result.set(0, 0, cos);
        result.set(0, 2, sin);
        result.set(2, 0, -sin);
        result.set(2, 2, cos);
        result
    }

    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let forward = (target - eye).normalized();
        let right = forward.cross(up).normalized();
        let true_up = right.cross(forward);
        let mut result = Mat4::IDENTITY;
        result.set(0, 0, right.x);
        result.set(0, 1, right.y);
        result.set(0, 2, right.z);
        result.set(1, 0, true_up.x);
        result.set(1, 1, true_up.y);
        result.set(1, 2, true_up.z);
        result.set(2, 0, -forward.x);
        result.set(2, 1, -forward.y);
        result.set(2, 2, -forward.z);
        result.set(0, 3, -right.dot(eye));
        result.set(1, 3, -true_up.dot(eye));
        result.set(2, 3, forward.dot(eye));
        result
    }

    /// The perspective projection, for the OpenGL depth range of -1 to 1.
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y / 2.0).tan();
        let mut result = Mat4 { m: [0.0; 16] };
        result.set(0, 0, f / aspect);
        result.set(1, 1, f);
        result.set(2, 2, (far + near) / (near - far));
        result.set(2, 3, 2.0 * far * near / (near - far));
        result.set(3, 2, -1.0);
        result
    }

    pub const fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        let mut result = Mat4::IDENTITY;
        result.set(0, 0, 2.0 / (right - left));
        result.set(1, 1, 2.0 / (top - bottom));
        result.set(2, 2, -2.0 / (far - near));
        result.set(0, 3, -(right + left) / (right - left));
        result.set(1, 3, -(top + bottom) / (top - bottom));
        result.set(2, 3, -(far + near) / (far - near));
        result
    }
}

/// `a * b` applies `b` first, then `a`.
impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, other: Mat4) -> Mat4 {
        let mut result = Mat4 { m: [0.0; 16] };
        for column in 0..4 {
            for row in 0..4 {
                result.m[column * 4 + row] = (0..4).map(|k| self.m[k * 4 + row] * other.m[column * 4 + k]).sum();
            }
        }
        result
    }
}
