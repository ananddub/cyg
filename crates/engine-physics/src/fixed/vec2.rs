use std::fmt;
use std::ops::{Add, Mul, Sub};

use super::FP;

#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct FPVec2 {
    pub x: FP,
    pub y: FP,
}

impl FPVec2 {
    pub const ZERO: FPVec2 = FPVec2 {
        x: FP::ZERO,
        y: FP::ZERO,
    };

    #[inline(always)]
    pub const fn new(x: FP, y: FP) -> Self {
        Self { x, y }
    }

    #[inline(always)]
    pub fn from_f64(x: f64, y: f64) -> Self {
        Self {
            x: FP::from_f64(x),
            y: FP::from_f64(y),
        }
    }

    #[inline(always)]
    pub fn dot(self, rhs: Self) -> FP {
        self.x * rhs.x + self.y * rhs.y
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

impl Add for FPVec2 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for FPVec2 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<FP> for FPVec2 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: FP) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl fmt::Debug for FPVec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FPVec2({:.3}, {:.3})", self.x.to_f64(), self.y.to_f64())
    }
}
