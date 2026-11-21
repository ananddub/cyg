//! How a field is encoded on the bit-packed lane.
//!
//! A schema field has a [`WireKind`] (what the value *is*) and an encoding (how many bits it gets
//! on the wire, and — for a float — the range those bits cover). Keeping the two apart matters:
//! the kind decides how the value behaves in the room, the encoding only decides its precision on
//! the wire. The same `f32` can be 8 bits of a small angle or 20 bits of a world position, and a
//! schema that adds precision must not look like a schema that changed types.
//!
//! Two rules keep this from becoming a source of desync:
//!
//! - **The encoding travels in the descriptor.** A client cannot guess that `x` is 20 bits over
//!   `-512..512`; if it guesses wrong it decodes every position off by a scale factor. So the
//!   quantizer is data, not code, and client and server run the same [`FieldEncoding::quantize`].
//! - **Changing an encoding changes the wire hash.** Two peers that agree on the type but not the
//!   encoding would silently corrupt every value; the hash mismatch turns that into a loud error
//!   instead.
//!
//! Integers are *exact*: their bits are the width of the value, so quantize is the identity for
//! every value the type can hold, and only the wire size changes. `I64`, strings and arrays stay
//! unpacked (`bits == 0`) — a 64-bit integer cannot survive a quantizer that speaks `f64`.

use crate::wire::WireKind;

/// Smallest bit width worth a descriptor entry. `0` means "not packed".
pub const UNPACKED: u8 = 0;

/// One field's wire encoding.
///
/// `min`/`max` are stored as raw `f32` bit patterns so the struct stays `Eq` and a descriptor round
/// trip is byte-exact. Use [`FieldEncoding::min`] / [`FieldEncoding::max`] to read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct FieldEncoding {
    /// Bits on the wire; `0` = the field is not packed at all.
    pub bits: u8,
    /// `true` when the range is an integer offset rather than a float quantization.
    ///
    /// The distinction is not cosmetic: a 32-bit integer spread over `f32` endpoints loses the
    /// bottom bits (2147483647 is not an `f32`), so integers are encoded as `value - min` and get
    /// back exactly the value the type can hold. Floats need the scale-and-round path.
    pub integer: bool,
    min_bits: u32,
    max_bits: u32,
}

impl FieldEncoding {
    /// Not packed: the field keeps its fixed-width little-endian form.
    pub const NONE: Self = Self {
        bits: 0,
        integer: false,
        min_bits: 0,
        max_bits: 0,
    };

    /// `bits` bits evenly covering `[min, max]`.
    ///
    /// A reversed or empty range collapses to [`FieldEncoding::NONE`] rather than producing a
    /// quantizer that divides by zero; the schema validator rejects those cases up front, so this
    /// is only the last line of defence.
    pub fn range(bits: u8, min: f32, max: f32) -> Self {
        if bits == 0 || bits > 32 || !(max > min) {
            return Self::NONE;
        }
        Self {
            bits,
            integer: false,
            min_bits: min.to_bits(),
            max_bits: max.to_bits(),
        }
    }

    /// Treats this encoding as an exact integer range: the code is `value - min`, not a scaled
    /// fraction. Used for every integer kind, and for an integer field that declares a sub-range.
    pub fn as_integer(self) -> Self {
        Self {
            integer: self.is_packed(),
            ..self
        }
    }

    /// The exact encoding of a kind: integer width for integers, one bit for a bool, unpacked for
    /// everything that has no natural bit width.
    pub fn exact(kind: WireKind) -> Self {
        match kind {
            WireKind::Bool => Self::range(1, 0.0, 1.0).as_integer(),
            WireKind::U8 => Self::range(8, 0.0, 255.0).as_integer(),
            WireKind::I8 => Self::range(8, -128.0, 127.0).as_integer(),
            WireKind::U16 => Self::range(16, 0.0, 65_535.0).as_integer(),
            WireKind::I16 => Self::range(16, -32_768.0, 32_767.0).as_integer(),
            WireKind::U32 => Self::range(32, 0.0, 4_294_967_295.0).as_integer(),
            // The upper bound is documentation: an `f32` rounds it to 2147483648, so the integer
            // path takes the ceiling from the bit width instead (see `integer_bounds`).
            WireKind::I32 => Self::range(32, -2_147_483_648.0, 2_147_483_648.0).as_integer(),
            // No exact quantizer: leave these in their fixed-width form until a schema asks.
            WireKind::I64 | WireKind::F32 | WireKind::F64 | WireKind::String | WireKind::Array => {
                Self::NONE
            }
        }
    }

    pub fn min(&self) -> f32 {
        f32::from_bits(self.min_bits)
    }

    pub fn max(&self) -> f32 {
        f32::from_bits(self.max_bits)
    }

    pub fn is_packed(&self) -> bool {
        self.bits > 0
    }

    /// Smallest and largest integer a code can represent, derived from the bit width.
    fn integer_bounds(&self) -> (i64, i64) {
        let floor = self.min().round() as i64;
        let levels = (1u64 << self.bits) - 1;
        (floor, floor + levels as i64)
    }

    /// Distance between two quantized values, i.e. the worst-case resolution.
    pub fn step(&self) -> f32 {
        if !self.is_packed() {
            return f32::INFINITY;
        }
        if self.integer {
            return 1.0;
        }
        let levels = ((1u64 << self.bits) - 1) as f32;
        (self.max() - self.min()) / levels
    }

    /// Worst-case error introduced by a round trip: half a step.
    pub fn error(&self) -> f32 {
        if self.integer {
            // Integers are exact; there is no error to report.
            return 0.0;
        }
        self.step() / 2.0
    }

    /// Maps `value` onto its quantized code, clamping to the declared range.
    ///
    /// Computed in `f64` so a 32-bit integer range is exact rather than "exact up to 2^24".
    pub fn quantize(&self, value: f32) -> u64 {
        if !self.is_packed() {
            return 0;
        }
        if self.integer {
            // Exact: the code is the offset from the floor, so no scale factor can drift. The
            // ceiling comes from the bit width, not from `max()` — an `f32` cannot hold
            // 2147483647, and rounding it up would put an out-of-range code on the wire.
            let (floor, ceiling) = self.integer_bounds();
            let value = (value.round() as i64).clamp(floor, ceiling);
            return (value - floor) as u64;
        }
        let (min, max) = (self.min() as f64, self.max() as f64);
        let levels = ((1u64 << self.bits) - 1) as f64;
        if !(max > min) {
            return 0;
        }
        let t = ((value as f64 - min) / (max - min)).clamp(0.0, 1.0);
        (t * levels).round() as u64
    }

    /// Inverse of [`FieldEncoding::quantize`]. Integer kinds land back on whole numbers.
    pub fn dequantize(&self, code: u64) -> f32 {
        if !self.is_packed() {
            return 0.0;
        }
        if self.integer {
            return (code as i64 + self.min().round() as i64) as f32;
        }
        let (min, max) = (self.min() as f64, self.max() as f64);
        let levels = ((1u64 << self.bits) - 1) as f64;
        let code = code.min(levels as u64) as f64;
        (min + (code / levels) * (max - min)) as f32
    }

    /// Quantizes an integer exactly.
    ///
    /// `quantize` takes `f32`, which silently loses the bottom bits of a `u32` above 2^24. Callers
    /// that hold an integer use this instead, so an exact encoding really is exact.
    pub fn quantize_int(&self, value: i64) -> u64 {
        if !self.is_packed() {
            return 0;
        }
        if self.integer {
            let (floor, ceiling) = self.integer_bounds();
            return (value.clamp(floor, ceiling) - floor) as u64;
        }
        self.quantize(value as f32)
    }

    /// True when `value` sits inside the declared range — the write-side range check.
    pub fn in_range(&self, value: f32) -> bool {
        value >= self.min() && value <= self.max()
    }

    /// Quantizes and rounds to the nearest whole number — the correct inverse for integer kinds,
    /// where `dequantize` can land a hair under (e.g. `126.99999`).
    pub fn dequantize_int(&self, code: u64) -> i64 {
        if self.integer {
            return code as i64 + self.min().round() as i64;
        }
        self.dequantize(code).round() as i64
    }
}

/// The encoding a field gets when its schema does not ask for one.
///
/// Integer and bool fields keep their exact width (no behaviour change, just a smaller frame once
/// they travel in the bit stream); floats stay unpacked until someone declares a range, because
/// guessing a range is how you get a client that decodes world positions to the nearest metre.
pub fn default_encoding(kind: WireKind) -> FieldEncoding {
    FieldEncoding::exact(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_exact_at_their_own_width() {
        let cases = [
            (WireKind::U8, 0.0f32, 255.0f32),
            (WireKind::I8, -128.0, 127.0),
            (WireKind::U16, 0.0, 65_535.0),
            (WireKind::I16, -32_768.0, 32_767.0),
        ];
        for (kind, low, high) in cases {
            let e = FieldEncoding::exact(kind);
            for value in [low, high, low + 3.0, high - 3.0] {
                let back = e.dequantize_int(e.quantize(value));
                assert_eq!(back as f32, value, "{kind:?} {value} did not survive");
            }
        }
        // 32-bit integers need the f64 path: an f32 quantizer would smear them past 2^24.
        let e = FieldEncoding::exact(WireKind::I32);
        for value in [-2_147_483_648i64, -1, 0, 12_345_678, 2_147_483_647] {
            assert_eq!(
                e.dequantize_int(e.quantize(value as f32)),
                value,
                "I32 {value} did not survive"
            );
        }
    }

    #[test]
    fn a_float_round_trip_stays_inside_half_a_step() {
        let e = FieldEncoding::range(20, -512.0, 512.0);
        assert!(
            e.error() < 0.001,
            "20 bits over a kilometre is sub-millimetre"
        );
        for value in [-511.5f32, -1.0, 0.0, 0.25, 512.0, 1_000.0, -1_000.0] {
            let back = e.dequantize(e.quantize(value));
            let clamped = value.clamp(e.min(), e.max());
            assert!(
                (back - clamped).abs() <= e.error() + 1e-4,
                "{value} came back as {back}"
            );
        }
    }

    #[test]
    fn fewer_bits_means_a_coarser_step_and_a_smaller_frame() {
        let coarse = FieldEncoding::range(8, 0.0, 360.0);
        let fine = FieldEncoding::range(16, 0.0, 360.0);
        assert!((coarse.step() - 360.0 / 255.0).abs() < 1e-3);
        assert!((fine.step() - 360.0 / 65_535.0).abs() < 1e-4);
        assert!(fine.step() < coarse.step());
    }

    #[test]
    fn unpacked_is_the_default_and_never_rounds_anything() {
        assert_eq!(default_encoding(WireKind::F32), FieldEncoding::NONE);
        assert_eq!(default_encoding(WireKind::I64), FieldEncoding::NONE);
        assert_eq!(default_encoding(WireKind::String), FieldEncoding::NONE);
        assert!(!FieldEncoding::NONE.is_packed());
        assert_eq!(FieldEncoding::NONE.quantize(3.5), 0);
        assert_eq!(FieldEncoding::NONE.dequantize(7), 0.0);
        assert!(FieldEncoding::NONE.error().is_infinite());
    }

    #[test]
    fn a_degenerate_range_falls_back_to_unpacked_instead_of_dividing_by_zero() {
        assert_eq!(FieldEncoding::range(16, 5.0, 5.0), FieldEncoding::NONE);
        assert_eq!(FieldEncoding::range(16, 10.0, -10.0), FieldEncoding::NONE);
        assert_eq!(FieldEncoding::range(0, 0.0, 1.0), FieldEncoding::NONE);
        assert_eq!(FieldEncoding::range(40, 0.0, 1.0), FieldEncoding::NONE);
    }

    #[test]
    fn an_out_of_range_value_clamps_to_a_real_endpoint() {
        let e = FieldEncoding::range(8, 0.0, 100.0);
        assert_eq!(e.dequantize(e.quantize(1_000.0)), 100.0);
        assert_eq!(e.dequantize(e.quantize(-5.0)), 0.0);
    }

    #[test]
    fn the_defaults_match_the_kind_widths() {
        assert_eq!(default_encoding(WireKind::Bool).bits, 1);
        assert_eq!(default_encoding(WireKind::U8).bits, 8);
        assert_eq!(default_encoding(WireKind::I16).bits, 16);
        assert_eq!(default_encoding(WireKind::U32).bits, 32);
        for kind in [
            WireKind::Bool,
            WireKind::U8,
            WireKind::I8,
            WireKind::U16,
            WireKind::I16,
            WireKind::U32,
            WireKind::I32,
        ] {
            assert_eq!(
                default_encoding(kind),
                FieldEncoding::exact(kind),
                "{kind:?} should default to exact"
            );
        }
    }
}
