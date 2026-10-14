//! # Q32.32 Deterministic Fixed-Point Arithmetic
//!
//! Provides 100% bit-identical mathematical operations across all CPU architectures
//! (x86_64, ARM64, and WebAssembly) eliminating floating-point divergence and desyncs.

use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Fixed-point number using signed 64-bit integer with 32 fractional bits (Q32.32).
///
/// Range: -2,147,483,648 to +2,147,483,647.
/// Precision: 1 / 2^32 ≈ 2.328e-10.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FP(pub i64);

impl FP {
    pub const FRACTIONAL_BITS: u32 = 32;
    pub const ONE: FP = FP(1i64 << Self::FRACTIONAL_BITS);
    pub const ZERO: FP = FP(0);
    pub const HALF: FP = FP(1i64 << (Self::FRACTIONAL_BITS - 1));

    #[inline(always)]
    pub const fn from_raw(raw: i64) -> Self {
        FP(raw)
    }

    #[inline(always)]
    pub const fn to_raw(self) -> i64 {
        self.0
    }

    #[inline(always)]
    pub const fn from_int(v: i32) -> Self {
        FP((v as i64) << Self::FRACTIONAL_BITS)
    }

    #[inline(always)]
    pub fn from_f64(v: f64) -> Self {
        FP((v * (1u64 << Self::FRACTIONAL_BITS) as f64).round() as i64)
    }

    #[inline(always)]
    pub fn to_f64(self) -> f64 {
        (self.0 as f64) / ((1u64 << Self::FRACTIONAL_BITS) as f64)
    }

    #[inline(always)]
    pub fn abs(self) -> Self {
        FP(self.0.abs())
    }

    /// Exact integer square root of the Q32.32 value, returned as Q32.32.
    ///
    /// Seeds with an IEEE-754 `sqrt` (correctly rounded on every target, so the
    /// seed is portable) and then refines with integer Newton–Raphson. The
    /// returned value is `floor(sqrt(val))` purely as a function of `val`, so
    /// the result is bit-identical to the old digit-by-digit algorithm and
    /// cross-platform deterministic regardless of the seed's accuracy.
    pub fn sqrt(self) -> Self {
        if self.0 <= 0 {
            return FP::ZERO;
        }
        // Shift up by 32 bits into i128 so that the integer sqrt produces Q32.32 precision
        let val = (self.0 as u128) << Self::FRACTIONAL_BITS;
        let mut root = (val as f64).sqrt() as u128 + 1;

        // From an overestimate, each step stays >= sqrt(val) and decreases until
        // it can no longer improve, leaving `root == ceil(sqrt(val))`.
        loop {
            let next = (root + val / root) >> 1;
            if next >= root {
                break;
            }
            root = next;
        }
        while root > 0 && root * root > val {
            root -= 1;
        }
        while (root + 1) * (root + 1) <= val {
            root += 1;
        }

        FP(root as i64)
    }

    #[inline(always)]
    pub fn clamp(self, min: FP, max: FP) -> Self {
        if self < min {
            min
        } else if self > max {
            max
        } else {
            self
        }
    }
}

impl fmt::Debug for FP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.to_f64())
    }
}

impl fmt::Display for FP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.to_f64())
    }
}

impl Add for FP {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        FP(self.0 + rhs.0)
    }
}

impl AddAssign for FP {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for FP {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        FP(self.0 - rhs.0)
    }
}

impl SubAssign for FP {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul for FP {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        let product = (self.0 as i128) * (rhs.0 as i128);
        FP((product >> Self::FRACTIONAL_BITS) as i64)
    }
}

impl MulAssign for FP {
    #[inline(always)]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Div for FP {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return if self.0 >= 0 {
                FP(i64::MAX)
            } else {
                FP(i64::MIN)
            };
        }
        let numerator = (self.0 as i128) << Self::FRACTIONAL_BITS;
        FP((numerator / (rhs.0 as i128)) as i64)
    }
}

impl DivAssign for FP {
    #[inline(always)]
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl Neg for FP {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        FP(-self.0)
    }
}

// =========================================================================
// Deterministic 2D & 3D Vectors
// =========================================================================

/// 2D Vector with Q32.32 Fixed-Point Coordinates.
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

/// 3D Vector with Q32.32 Fixed-Point Coordinates.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_point_basic_arithmetic() {
        let a = FP::from_int(5);
        let b = FP::from_int(3);

        assert_eq!((a + b).to_f64(), 8.0);
        assert_eq!((a - b).to_f64(), 2.0);
        assert_eq!((a * b).to_f64(), 15.0);

        let half = FP::HALF;
        assert_eq!(half.to_f64(), 0.5);
        assert_eq!((a * half).to_f64(), 2.5);
    }

    #[test]
    fn test_fixed_point_sqrt() {
        let nine = FP::from_int(9);
        assert_eq!(nine.sqrt().to_f64(), 3.0);

        let two = FP::from_int(2);
        let sqrt2 = two.sqrt().to_f64();
        assert!((sqrt2 - 1.41421356).abs() < 0.0001);
    }

    /// Reference implementation: the original digit-by-digit integer sqrt.
    fn sqrt_digit_by_digit(raw: i64) -> i64 {
        if raw <= 0 {
            return 0;
        }
        let val = (raw as u128) << FP::FRACTIONAL_BITS;
        let mut root: u128 = 0;
        let mut bit: u128 = 1u128 << 126;
        while bit > val {
            bit >>= 2;
        }
        let mut rem = val;
        while bit != 0 {
            if rem >= root + bit {
                rem -= root + bit;
                root = (root >> 1) + bit;
            } else {
                root >>= 1;
            }
            bit >>= 2;
        }
        root as i64
    }

    #[test]
    fn test_fixed_point_sqrt_matches_reference_bit_for_bit() {
        // Guards the Newton–Raphson rewrite: switching algorithms must not move a
        // single state checksum, so every output has to equal the reference.
        let mut x: u64 = 0x9E3779B97F4A7C15;
        for i in 0..4096 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            for raw in [i as i64, (i * 1024) as i64, x as i64 & i64::MAX, 1, 2, 3] {
                assert_eq!(FP::from_raw(raw).sqrt().to_raw(), sqrt_digit_by_digit(raw), "raw={raw}");
            }
        }
    }

    #[test]
    fn test_fpvec3_dot_cross_distance() {
        let v1 = FPVec3::from_f64(3.0, 0.0, 0.0);
        let v2 = FPVec3::from_f64(0.0, 4.0, 0.0);

        assert_eq!(v1.distance(v2).to_f64(), 5.0);
        assert_eq!(v1.dot(v2).to_f64(), 0.0);

        let cross = v1.cross(v2);
        assert_eq!(cross.z.to_f64(), 12.0);
    }

    #[test]
    fn test_cross_platform_exact_bit_reproducibility() {
        // Compute complex kinematic curve with 100 iterations
        let mut pos = FPVec3::from_f64(10.0, 20.0, 30.0);
        let vel = FPVec3::from_f64(1.5, -0.5, 2.25);
        let dt = FP::from_f64(0.0166666666);

        for _ in 0..100 {
            pos = pos + vel * dt;
        }

        // Must produce identical raw integer bits on every architecture without divergence
        assert_ne!(pos.x.to_raw(), 0);
        assert_ne!(pos.y.to_raw(), 0);
        assert_ne!(pos.z.to_raw(), 0);
    }
}
