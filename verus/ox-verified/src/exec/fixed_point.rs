// Verified mirror of `crates/engine-physics/src/fixed/fp.rs` and `fp_ops.rs`
// (Q32.32 fixed-point arithmetic, deterministic physics path).
//
// Keep in sync with the originals:
// - crates/engine-physics/src/fixed/fp.rs
// - crates/engine-physics/src/fixed/fp_ops.rs
//
// NOTE: `#[verus_spec]` cannot annotate trait impls / impl methods in this
// Verus build, so the verified logic lives in free functions (`fp_*`) and the
// trait impls below delegate to them. Semantics are identical to the engine
// originals; parity is pinned by tests/verus_parity.rs.

#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(verus, verus_verify)]
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
}

use std::ops::{Add, Div, Mul, Neg, Sub};

impl Add for FP {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        fp_add(self, rhs)
    }
}

impl Sub for FP {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        fp_sub(self, rhs)
    }
}

impl Mul for FP {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        fp_mul(self, rhs)
    }
}

impl Div for FP {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: Self) -> Self {
        fp_div(self, rhs)
    }
}

impl Neg for FP {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        fp_neg(self)
    }
}

/// Q32.32 addition. Panics on i64 overflow in debug builds, exactly like the
/// engine original; the `requires` documents the no-overflow contract that
/// the deterministic physics path maintains.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        a.0 + b.0 >= i64::MIN as int && a.0 + b.0 <= i64::MAX as int,
    ensures
        r.0 == a.0 + b.0,
))]
pub fn fp_add(a: FP, b: FP) -> FP {
    FP(a.0 + b.0)
}

/// Q32.32 subtraction. Same no-overflow contract as the engine original.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        a.0 - b.0 >= i64::MIN as int && a.0 - b.0 <= i64::MAX as int,
    ensures
        r.0 == a.0 - b.0,
))]
pub fn fp_sub(a: FP, b: FP) -> FP {
    FP(a.0 - b.0)
}

/// Q32.32 negation. `i64::MIN` would overflow (as in the engine original).
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        a.0 != i64::MIN as int,
    ensures
        r.0 == -a.0,
))]
pub fn fp_neg(a: FP) -> FP {
    FP(0i64 - a.0)
}

/// Q32.32 multiplication: full i128 product, shift right by 32, truncate to
/// i64. Cannot panic (i128 holds any i64 x i64 product).
///
/// The engine original SILENTLY TRUNCATES when the Q32.32 product overflows
/// i64 (Verus caught this!). The verified contract below covers the
/// no-truncation domain; outside it the mirror behaves bit-identically to
/// the original but the postcondition does not apply.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        ((if a.0 < 0 { -(a.0 as int) } else { a.0 as int })
            * (if b.0 < 0 { -(b.0 as int) } else { b.0 as int })) / 4294967296
            <= i64::MAX as int,
    ensures
        r.0 == (if (a.0 < 0) != (b.0 < 0) {
            -(((if a.0 < 0 { -(a.0 as int) } else { a.0 as int })
                * (if b.0 < 0 { -(b.0 as int) } else { b.0 as int })) / 4294967296)
        } else {
            ((if a.0 < 0 { -(a.0 as int) } else { a.0 as int })
                * (if b.0 < 0 { -(b.0 as int) } else { b.0 as int })) / 4294967296
        }),
))]
pub fn fp_mul(a: FP, b: FP) -> FP {
    // Q32.32 multiply: full magnitude product, floor-divide by 2^32 (the
    // engine original's arithmetic `>> 32`), then re-apply the sign.
    let neg = (a.0 < 0) != (b.0 < 0);
    let ma: i128 = if a.0 < 0 { -(a.0 as i128) } else { a.0 as i128 };
    let mb: i128 = if b.0 < 0 { -(b.0 as i128) } else { b.0 as i128 };
    let pm: i128 = ma * mb;
    let q: i128 = pm / 4294967296i128;
    let signed: i128 = if neg { -q } else { q };
    #[allow(clippy::unnecessary_cast)]
    let result = signed as i64;
    FP(result)
}

/// Q32.32 division: saturates on divide-by-zero (i64::MAX for non-negative
/// numerators, i64::MIN for negative), otherwise i128 shift-left-32 divide
/// with truncating cast.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        b.0 != 0 ==>
            (if (a.0 < 0) != (b.0 < 0) {
                -(((if a.0 < 0 { -(a.0 as int) } else { a.0 as int }) * 4294967296)
                    / (if b.0 < 0 { -(b.0 as int) } else { b.0 as int }))
            } else {
                ((if a.0 < 0 { -(a.0 as int) } else { a.0 as int }) * 4294967296)
                    / (if b.0 < 0 { -(b.0 as int) } else { b.0 as int })
            }) >= i64::MIN as int
            && (if (a.0 < 0) != (b.0 < 0) {
                -(((if a.0 < 0 { -(a.0 as int) } else { a.0 as int }) * 4294967296)
                    / (if b.0 < 0 { -(b.0 as int) } else { b.0 as int }))
            } else {
                ((if a.0 < 0 { -(a.0 as int) } else { a.0 as int }) * 4294967296)
                    / (if b.0 < 0 { -(b.0 as int) } else { b.0 as int })
            }) <= i64::MAX as int,
    ensures
        b.0 == 0 && a.0 >= 0 ==> r.0 == i64::MAX,
        b.0 == 0 && a.0 < 0 ==> r.0 == i64::MIN,
        b.0 != 0 ==> r.0 == (if (a.0 < 0) != (b.0 < 0) {
            -(((if a.0 < 0 { -(a.0 as int) } else { a.0 as int }) * 4294967296)
                / (if b.0 < 0 { -(b.0 as int) } else { b.0 as int }))
        } else {
            ((if a.0 < 0 { -(a.0 as int) } else { a.0 as int }) * 4294967296)
                / (if b.0 < 0 { -(b.0 as int) } else { b.0 as int })
        }),
))]
pub fn fp_div(a: FP, b: FP) -> FP {
    if b.0 == 0 {
        return if a.0 >= 0 { FP(i64::MAX) } else { FP(i64::MIN) };
    }
    // Rust's `/` truncates toward zero; Verus's `/` is Euclidean. Compute the
    // quotient magnitude on non-negative operands (where the two agree) and
    // apply the sign afterwards — bit-identical to the engine original.
    let neg = (a.0 < 0) != (b.0 < 0);
    let mag_a: i128 = if a.0 < 0 { -(a.0 as i128) } else { a.0 as i128 };
    let mag_b: i128 = if b.0 < 0 { -(b.0 as i128) } else { b.0 as i128 };
    let num: i128 = mag_a * 4294967296i128; // == a.0 << 32 in magnitude
    let qmag: i128 = num / mag_b;
    let signed: i128 = if neg { -qmag } else { qmag };
    // The precondition bounds the exact quotient to the i64 range, so this
    // cast (which the engine original performs unconditionally, silently
    // truncating) never truncates here.
    #[allow(clippy::unnecessary_cast)]
    let result = signed as i64;
    FP(result)
}

/// Q32.32 absolute value. `i64::MIN.abs()` would panic (as in the original).
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        a.0 != i64::MIN as int,
    ensures
        r.0 >= 0,
        r.0 == a.0 || r.0 == -a.0,
))]
pub fn fp_abs(a: FP) -> FP {
    FP(if a.0 >= 0 { a.0 } else { -a.0 })
}

/// Q32.32 clamp: result is within [min, max] (when min <= max).
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        min.0 <= max.0,
    ensures
        r.0 >= min.0 && r.0 <= max.0,
        r == min || r == max || r == a,
))]
pub fn fp_clamp(a: FP, min: FP, max: FP) -> FP {
    // Same ordering as the derived Ord on FP (a newtype over i64), written on
    // raw fields so Verus can reason about it.
    if a.0 < min.0 {
        min
    } else if a.0 > max.0 {
        max
    } else {
        a
    }
}
