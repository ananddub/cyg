//! Bit-level packing for the wire.
//!
//! Fixed-width little-endian fields cost the same for a 3-bit live flag as for a 32-bit id. The
//! fix is the one Quake III shipped in 1999 (`MSG_WriteDeltaEntity`) and every modern netcode
//! reuses: write only the bits the value needs, back to back, into one stream.
//!
//! Bit order is **LSB-first inside each byte**, and bytes are appended in order. That is the usual
//! choice: two implementations (this crate and a JS SDK) can agree on it with a single golden byte
//! vector, and it does not depend on host endianness.
//!
//! Floats are quantized rather than bit-cast: a position that only needs centimetres does not need
//! 32 bits of mantissa, and the error is known up front instead of being whatever rounding left
//! behind. `bits = 0` writes nothing, which is how an absent optional field costs zero.

/// Writes bits into a byte buffer, least significant bit first.
#[derive(Debug, Default)]
pub struct BitWriter {
    out: Vec<u8>,
    /// Bits already written into the last byte (0..8).
    used: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            out: Vec::with_capacity(bytes),
            used: 0,
        }
    }

    /// Bits written so far.
    pub fn len_bits(&self) -> usize {
        self.out.len() * 8 - self.used as usize
    }

    /// Bytes the finished frame will occupy.
    pub fn len_bytes(&self) -> usize {
        self.out.len()
    }

    /// Writes the low `bits` bits of `value`. `bits` above 64 is clamped to 64; `0` writes nothing.
    pub fn write_bits(&mut self, value: u64, bits: u8) {
        let bits = bits.min(64) as usize;
        if bits == 0 {
            return;
        }
        // Mask off anything above the declared width so a caller cannot corrupt neighbours.
        let value = if bits == 64 {
            value
        } else {
            value & ((1u64 << bits) - 1)
        };
        let mut left = bits;
        let mut at = 0u8;
        while left > 0 {
            if self.used == 0 {
                self.out.push(0);
            }
            let space = 8 - self.used;
            let take = space.min(left as u8);
            let chunk = ((value >> at) & ((1u64 << take) - 1)) as u8;
            let last = self.out.last_mut().expect("a byte was just pushed");
            *last |= chunk << self.used;
            self.used += take;
            if self.used == 8 {
                self.used = 0;
            }
            at += take;
            left -= take as usize;
        }
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_bits(value as u64, 1);
    }

    /// Quantizes `value` into `bits` across `[min, max]`, clamped to that range.
    ///
    /// The error is at most half a step: `(max - min) / (2^bits - 1) / 2`.
    pub fn write_float(&mut self, value: f32, bits: u8, min: f32, max: f32) {
        if bits == 0 || !(max > min) {
            return;
        }
        let steps = ((1u64 << bits.min(32)) - 1) as f32;
        let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
        let q = (t * steps).round() as u64;
        self.write_bits(q, bits.min(32));
    }

    /// Pads to the next byte boundary, so a frame can end on a whole byte.
    pub fn finish(&mut self) -> &[u8] {
        self.used = 0;
        &self.out
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        self.used = 0;
        self.out
    }
}

/// Reads what [`BitWriter`] wrote. Every read is bounds-checked; a short frame yields `None`
/// rather than garbage.
#[derive(Debug)]
pub struct BitReader<'a> {
    buf: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, bit: 0 }
    }

    pub fn bits_read(&self) -> usize {
        self.bit
    }

    pub fn read_bits(&mut self, bits: u8) -> Option<u64> {
        let bits = bits.min(64) as usize;
        if bits == 0 {
            return Some(0);
        }
        if self.bit + bits > self.buf.len() * 8 {
            return None;
        }
        let mut value = 0u64;
        let mut written = 0usize;
        while written < bits {
            let byte = self.buf[self.bit / 8];
            let offset = (self.bit % 8) as u32;
            let space = 8 - offset as usize;
            let take = space.min(bits - written);
            let mask = if take == 8 { 0xFFu8 } else { (1u8 << take) - 1 };
            let chunk = ((byte >> offset) & mask) as u64;
            value |= chunk << written;
            self.bit += take;
            written += take;
        }
        Some(value)
    }

    pub fn read_bool(&mut self) -> Option<bool> {
        self.read_bits(1).map(|v| v == 1)
    }

    pub fn read_float(&mut self, bits: u8, min: f32, max: f32) -> Option<f32> {
        if bits == 0 || !(max > min) {
            return Some(min);
        }
        let steps = ((1u64 << bits.min(32)) - 1) as f32;
        let q = self.read_bits(bits.min(32))? as f32;
        Some(min + (q / steps) * (max - min))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_pack_lsb_first_and_read_back() {
        let mut w = BitWriter::new();
        w.write_bits(0b1, 1);
        w.write_bits(0b10, 2);
        w.write_bits(0x1FF, 9);
        let bytes = w.clone_finish();

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(1), Some(1));
        assert_eq!(r.read_bits(2), Some(0b10));
        assert_eq!(r.read_bits(9), Some(0x1FF));
        assert_eq!(r.bits_read(), 12);
    }

    #[test]
    fn booleans_take_exactly_one_bit() {
        let mut w = BitWriter::new();
        for value in [true, false, false, true, true, false, true, false, true] {
            w.write_bool(value);
        }
        let bytes = w.clone_finish();
        assert_eq!(bytes.len(), 2, "8 flags per byte, not one byte each");
        // LSB-first: bit0=1, bit3=1, bit4=1, bit6=1 → 0b0101_1001.
        assert_eq!(bytes[0], 0b_0101_1001);

        let mut r = BitReader::new(&bytes);
        let read: Vec<bool> = (0..9).filter_map(|_| r.read_bool()).collect();
        assert_eq!(
            read,
            vec![true, false, false, true, true, false, true, false, true]
        );
    }

    #[test]
    fn a_wide_value_crosses_byte_boundaries() {
        let mut w = BitWriter::new();
        w.write_bits(3, 3);
        w.write_bits(0xDEAD_BEEF, 32);
        let bytes = w.clone_finish();

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(3), Some(3));
        assert_eq!(r.read_bits(32), Some(0xDEAD_BEEF));
    }

    #[test]
    fn a_quantized_float_stays_inside_half_a_step() {
        // 20 bits over 1024 metres: one step is ~1 mm, so the error is under a millimetre.
        let (bits, min, max) = (20u8, -512.0f32, 512.0f32);
        let step = (max - min) / ((1u64 << bits) - 1) as f32;

        for value in [-511.9f32, -1.0, 0.0, 0.5, 123.456, 511.9] {
            let mut w = BitWriter::new();
            w.write_float(value, bits, min, max);
            let bytes = w.clone_finish();
            let mut r = BitReader::new(&bytes);
            let back = r.read_float(bits, min, max).expect("a float comes back");
            assert!(
                (back - value).abs() <= step / 2.0 + 1e-5,
                "{value} came back as {back}, step is {step}"
            );
        }
    }

    #[test]
    fn an_out_of_range_float_clamps_instead_of_wrapping() {
        let (bits, min, max) = (10u8, 0.0f32, 100.0f32);
        let mut w = BitWriter::new();
        w.write_float(1_000.0, bits, min, max);
        w.write_float(-50.0, bits, min, max);
        let bytes = w.clone_finish();

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_float(bits, min, max), Some(max));
        assert_eq!(r.read_float(bits, min, max), Some(min));
    }

    #[test]
    fn zero_bits_costs_zero_and_reads_as_the_minimum() {
        let mut w = BitWriter::new();
        w.write_float(5.0, 0, 0.0, 10.0);
        w.write_bits(7, 0);
        assert_eq!(w.len_bits(), 0, "an absent field costs nothing");

        let mut r = BitReader::new(&[]);
        assert_eq!(r.read_float(0, 0.0, 10.0), Some(0.0));
    }

    #[test]
    fn a_short_frame_reads_none_instead_of_garbage() {
        let bytes = [0xFFu8];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(8), Some(0xFF));
        assert_eq!(r.read_bits(1), None, "past the end is an error, not a zero");
        assert_eq!(r.read_bits(8), None);
    }

    #[test]
    fn writing_past_the_declared_width_does_not_touch_neighbours() {
        let mut w = BitWriter::new();
        w.write_bits(0xFFFF, 4); // only the low 4 bits are kept
        w.write_bits(1, 4);
        let bytes = w.clone_finish();
        assert_eq!(bytes, vec![0b0001_1111]);
    }

    #[test]
    fn an_entity_style_record_is_far_smaller_than_fixed_fields() {
        // A typical replicated row: id 12 bits, position 3x20, yaw 12, health 8, 3 flags, alive 1.
        let mut w = BitWriter::new();
        for _ in 0..32 {
            w.write_bits(1234, 12);
            w.write_float(10.0, 20, -512.0, 512.0);
            w.write_float(-3.5, 20, -512.0, 512.0);
            w.write_float(7.25, 20, -512.0, 512.0);
            w.write_bits(2048, 12);
            w.write_bits(87, 8);
            w.write_bits(0b101, 3);
            w.write_bool(true);
        }
        let packed = w.len_bytes();
        // The fixed little-endian layout the wire uses today: id u32 + 3x f32 + yaw f32 + health u8
        // + flags u8 = 27 bytes per row.
        let fixed = 27 * 32;
        assert!(
            packed * 2 < fixed,
            "{packed} bytes packed vs {fixed} fixed — expected better than half"
        );
    }
}

impl BitWriter {
    /// Test-only helper: a copy of the bytes without consuming the writer.
    #[cfg(test)]
    fn clone_finish(&mut self) -> Vec<u8> {
        self.used = 0;
        self.out.clone()
    }
}
