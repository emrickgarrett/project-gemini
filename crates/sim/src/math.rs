use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl PartialEq for Vec3 {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y && self.z == other.z
    }
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len <= 1e-6 {
            Self::ZERO
        } else {
            self * (1.0 / len)
        }
    }

    pub fn xz_normalized(self) -> Self {
        Self::new(self.x, 0.0, self.z).normalized()
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl std::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

/// Yaw 0 looks down -Z, matching a typical first-person camera.
pub fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    let cp = pitch.cos();
    Vec3::new(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp)
}

pub fn flat_forward(yaw: f32) -> Vec3 {
    look_dir(yaw, 0.0)
}

pub fn flat_right(yaw: f32) -> Vec3 {
    Vec3::new(yaw.cos(), 0.0, -yaw.sin())
}

/// `Quat::from_rotation_y(yaw)` applied to a local offset.
pub fn rotate_y(yaw: f32, v: Vec3) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn overlaps_xz_circle(self, center: Vec3, radius: f32) -> bool {
        let cx = center.x.clamp(self.min.x, self.max.x);
        let cz = center.z.clamp(self.min.z, self.max.z);
        let dx = center.x - cx;
        let dz = center.z - cz;
        dx * dx + dz * dz < radius * radius
    }

    /// Push `center` out of this box in the XZ plane. Returns the corrected point.
    pub fn push_circle_xz(self, center: Vec3, radius: f32) -> Vec3 {
        if center.y > self.max.y {
            return center;
        }
        let closest_x = center.x.clamp(self.min.x, self.max.x);
        let closest_z = center.z.clamp(self.min.z, self.max.z);
        let dx = center.x - closest_x;
        let dz = center.z - closest_z;
        let d2 = dx * dx + dz * dz;
        if d2 >= radius * radius {
            return center;
        }
        if d2 > 1e-8 {
            let d = d2.sqrt();
            let scale = (radius - d) / d;
            return Vec3::new(center.x + dx * scale, center.y, center.z + dz * scale);
        }
        let left = center.x - self.min.x;
        let right = self.max.x - center.x;
        let down = center.z - self.min.z;
        let up = self.max.z - center.z;
        let min_pen = left.min(right).min(down).min(up);
        if min_pen == left {
            Vec3::new(self.min.x - radius, center.y, center.z)
        } else if min_pen == right {
            Vec3::new(self.max.x + radius, center.y, center.z)
        } else if min_pen == down {
            Vec3::new(center.x, center.y, self.min.z - radius)
        } else {
            Vec3::new(center.x, center.y, self.max.z + radius)
        }
    }
}

/// Closest point on segment p0→p1 to `center`, as a t in 0..=1, if within `radius`.
pub fn segment_hits_sphere(p0: Vec3, p1: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let d = p1 - p0;
    let len2 = d.length_squared();
    if len2 <= 1e-10 {
        return ((center - p0).length_squared() <= radius * radius).then_some(0.0);
    }
    let t = ((center - p0).dot(d) / len2).clamp(0.0, 1.0);
    let closest = p0 + d * t;
    if (closest - center).length_squared() <= radius * radius {
        Some(t)
    } else {
        None
    }
}

pub fn segment_hits_aabb(p0: Vec3, p1: Vec3, box_: Aabb) -> bool {
    let d = p1 - p0;
    let mut tmin = 0.0_f32;
    let mut tmax = 1.0_f32;
    for (origin, dir, bmin, bmax) in [
        (p0.x, d.x, box_.min.x, box_.max.x),
        (p0.y, d.y, box_.min.y, box_.max.y),
        (p0.z, d.z, box_.min.z, box_.max.z),
    ] {
        if dir.abs() <= 1e-8 {
            if origin < bmin || origin > bmax {
                return false;
            }
        } else {
            let inv = 1.0 / dir;
            let mut t1 = (bmin - origin) * inv;
            let mut t2 = (bmax - origin) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return false;
            }
        }
    }
    true
}

/// Integer power so the wave-budget exponent does not call libm `powf`.
pub fn powi(base: f32, exp: u32) -> f32 {
    let mut acc = 1.0_f64;
    let b = base as f64;
    for _ in 0..exp {
        acc *= b;
    }
    acc as f32
}
