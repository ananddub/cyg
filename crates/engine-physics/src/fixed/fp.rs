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

    pub fn sqrt(self) -> Self {
        if self.0 <= 0 {
            return FP::ZERO;
        }

        let val = (self.0 as u128) << Self::FRACTIONAL_BITS;
        let mut root = (val as f64).sqrt() as u128 + 1;

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
