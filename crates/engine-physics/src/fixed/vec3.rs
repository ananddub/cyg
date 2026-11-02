use std::fmt;
use std::ops::{Add, Mul, Sub};

use super::FP;

#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct FPVec3 {
    pub x: FP,
    pub y: FP,
    pub z: FP,
}

impl FPVec3 {
    pub const ZERO: FPVec3 = FPVec3 {
        x: FP::ZERO,
        y: FP::ZERO,
        z: FP::ZERO,
    };

    #[inline(always)]
    pub const fn new(x: FP, y: FP, z: FP) -> Self {
        Self { x, y, z }
    }

    #[inline(always)]
    pub fn from_f64(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: FP::from_f64(x),
            y: FP::from_f64(y),
            z: FP::from_f64(z),
        }
    }

    #[inline(always)]
    pub fn dot(self, rhs: Self) -> FP {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[inline(always)]
    pub fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }

    #[inline(always)]
    pub fn length_squared(self) -> FP {
        self.dot(self)
    }

    #[inline(always)]
    pub fn length(self) -> FP {
        self.length_squared().sqrt()
    }

    #[inline(always)]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len.0 > 0 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            Self::ZERO
        }
    }

    #[inline(always)]
    pub fn distance(self, rhs: Self) -> FP {
        (self - rhs).length()
    }

    #[inline(always)]
    pub fn lerp(self, rhs: Self, t: FP) -> Self {
        self + (rhs - self) * t
    }
}

impl Add for FPVec3 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for FPVec3 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<FP> for FPVec3 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: FP) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl fmt::Debug for FPVec3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FPVec3({:.3}, {:.3}, {:.3})",
            self.x.to_f64(),
            self.y.to_f64(),
            self.z.to_f64()
        )
    }
}
