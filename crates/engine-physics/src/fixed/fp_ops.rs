use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use super::FP;

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
