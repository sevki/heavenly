//! Minimal, dependency-free 3D vector and rotation types.
//!
//! These are deliberately small so that the core stays `no_std`. Adapters
//! may convert to and from `nalgebra` types at the crate boundary.

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A Cartesian 3-vector. Units are given by the field or method that holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    pub const fn from_array(a: [f64; 3]) -> Self {
        Vec3::new(a[0], a[1], a[2])
    }

    pub const fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn norm_squared(self) -> f64 {
        self.dot(self)
    }

    pub fn norm(self) -> f64 {
        libm::sqrt(self.norm_squared())
    }

    /// Returns the unit vector in this direction, or `None` for a zero vector.
    pub fn normalized(self) -> Option<Vec3> {
        let n = self.norm();
        if n > 0.0 && n.is_finite() {
            Some(self / n)
        } else {
            None
        }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Mul<Vec3> for f64 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        v * self
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        Vec3::new(self.x / s, self.y / s, self.z / s)
    }
}

impl From<[f64; 3]> for Vec3 {
    fn from(a: [f64; 3]) -> Self {
        Vec3::from_array(a)
    }
}

impl From<Vec3> for [f64; 3] {
    fn from(v: Vec3) -> Self {
        v.to_array()
    }
}

/// A proper rotation stored as a unit quaternion `w + xi + yj + zk`.
///
/// `Rotation::apply` uses the active convention: it rotates a vector's
/// components. When used inside a [`crate::Transform`] from frame A to frame B
/// it is the direction-cosine matrix `R_BA`, i.e. it maps components expressed
/// along A's axes to components expressed along B's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

impl Default for Rotation {
    fn default() -> Self {
        Rotation::IDENTITY
    }
}

impl Rotation {
    pub const IDENTITY: Rotation = Rotation {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Builds a rotation from quaternion components, normalising them.
    ///
    /// Returns `None` if the quaternion has zero or non-finite norm.
    pub fn from_quaternion(w: f64, x: f64, y: f64, z: f64) -> Option<Rotation> {
        let n = libm::sqrt(w * w + x * x + y * y + z * z);
        if n > 0.0 && n.is_finite() {
            Some(Rotation {
                w: w / n,
                x: x / n,
                y: y / n,
                z: z / n,
            })
        } else {
            None
        }
    }

    /// Right-handed rotation of `angle_rad` about `axis`.
    ///
    /// A zero axis yields the identity.
    pub fn from_axis_angle(axis: Vec3, angle_rad: f64) -> Rotation {
        match axis.normalized() {
            None => Rotation::IDENTITY,
            Some(a) => {
                let h = 0.5 * angle_rad;
                let s = libm::sin(h);
                Rotation {
                    w: libm::cos(h),
                    x: a.x * s,
                    y: a.y * s,
                    z: a.z * s,
                }
            }
        }
    }

    /// Quaternion components `[w, x, y, z]`.
    pub fn quaternion(self) -> [f64; 4] {
        [self.w, self.x, self.y, self.z]
    }

    pub fn inverse(self) -> Rotation {
        Rotation {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Rotates `v`.
    pub fn apply(self, v: Vec3) -> Vec3 {
        // v' = v + 2w(q × v) + 2 q × (q × v)
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    /// `self ∘ other`: applies `other` first, then `self`.
    pub fn compose(self, other: Rotation) -> Rotation {
        let (a, b) = (self, other);
        Rotation {
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        }
        .renormalized()
    }

    /// Row-major 3×3 rotation matrix.
    pub fn to_matrix(self) -> [[f64; 3]; 3] {
        let c = |v| self.apply(v);
        let (cx, cy, cz) = (c(Vec3::X), c(Vec3::Y), c(Vec3::Z));
        [[cx.x, cy.x, cz.x], [cx.y, cy.y, cz.y], [cx.z, cy.z, cz.z]]
    }

    /// Angle of the rotation in `[0, π]` radians.
    pub fn angle_rad(self) -> f64 {
        let v = libm::sqrt(self.x * self.x + self.y * self.y + self.z * self.z);
        2.0 * libm::atan2(v, libm::fabs(self.w))
    }

    fn renormalized(self) -> Rotation {
        Rotation::from_quaternion(self.w, self.x, self.y, self.z).unwrap_or(Rotation::IDENTITY)
    }
}
