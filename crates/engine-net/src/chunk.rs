//! Reliable large-block chunking for datagram transports.
//!
//! One block is split into fixed-size slices; the receiver acknowledges a
//! bitfield, and only the missing slices are retransmitted. This is the
//! Gaffer "large blocks of data" pattern: a single lost datagram must not lose
//! the whole join snapshot or descriptor.

pub const CHUNK_SLICE_OP: u8 = 0x27;
pub const CHUNK_ACK_OP: u8 = 0x28;
pub const DEFAULT_SLICE_SIZE: usize = 1024;
pub const MAX_SLICES: usize = 256;
pub const MAX_BLOCK: usize = DEFAULT_SLICE_SIZE * MAX_SLICES;
pub const SLICE_HEADER: usize = 9; // op + chunk + index + total + len
pub const RETRY_MS: u64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkError {
    Empty,
    TooLarge { bytes: usize },
    Truncated,
    BadOp(u8),
    BadIndex { index: u16, total: u16 },
    BadLength { got: usize, want: usize },
    ChunkMismatch { got: u16, want: u16 },
    TotalMismatch { got: u16, want: u16 },
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty chunk block"),
            Self::TooLarge { bytes } => write!(f, "chunk block of {bytes} B exceeds {MAX_BLOCK}"),
            Self::Truncated => write!(f, "truncated chunk frame"),
            Self::BadOp(op) => write!(f, "unknown chunk op 0x{op:02x}"),
            Self::BadIndex { index, total } => {
                write!(f, "chunk slice {index} outside 0..{total}")
            }
            Self::BadLength { got, want } => write!(f, "chunk slice length {got}, expected {want}"),
            Self::ChunkMismatch { got, want } => {
                write!(f, "chunk id {got} does not match in-flight {want}")
            }
            Self::TotalMismatch { got, want } => {
                write!(f, "chunk total {got} does not match in-flight {want}")
            }
        }
    }
}

impl std::error::Error for ChunkError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice<'a> {
    pub chunk_id: u16,
    pub index: u16,
    pub total: u16,
    pub data: &'a [u8],
}

pub fn write_slice(out: &mut Vec<u8>, chunk_id: u16, index: u16, total: u16, data: &[u8]) {
    out.clear();
    out.push(CHUNK_SLICE_OP);
    out.extend_from_slice(&chunk_id.to_le_bytes());
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
}

pub fn parse_slice(data: &[u8]) -> Result<Slice<'_>, ChunkError> {
    if data.len() < SLICE_HEADER {
        return Err(ChunkError::Truncated);
    }
    if data[0] != CHUNK_SLICE_OP {
        return Err(ChunkError::BadOp(data[0]));
    }
    let chunk_id = u16::from_le_bytes([data[1], data[2]]);
    let index = u16::from_le_bytes([data[3], data[4]]);
    let total = u16::from_le_bytes([data[5], data[6]]);
    let len = u16::from_le_bytes([data[7], data[8]]) as usize;
    if total == 0 || total as usize > MAX_SLICES || index >= total {
        return Err(ChunkError::BadIndex { index, total });
    }
    if data.len() - SLICE_HEADER != len {
        return Err(ChunkError::BadLength {
            got: data.len() - SLICE_HEADER,
            want: len,
        });
    }
    Ok(Slice {
        chunk_id,
        index,
        total,
        data: &data[SLICE_HEADER..],
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkAck {
    pub chunk_id: u16,
    pub total: u16,
    bits: Vec<u64>,
}

impl ChunkAck {
    pub fn new(chunk_id: u16, total: u16) -> Self {
        Self {
            chunk_id,
            total,
            bits: vec![0; words(total)],
        }
    }

    fn bit(&self, index: u16) -> bool {
        let word = index as usize / 64;
        let bit = index as usize % 64;
        self.bits.get(word).is_some_and(|w| w & (1u64 << bit) != 0)
    }

    fn set(&mut self, index: u16) {
        let word = index as usize / 64;
        let bit = index as usize % 64;
        if let Some(w) = self.bits.get_mut(word) {
            *w |= 1u64 << bit;
        }
    }

    pub fn received(&self, index: u16) -> bool {
        self.bit(index)
    }

    pub fn complete(&self, total: u16) -> bool {
        total > 0 && (0..total).all(|i| self.bit(i))
    }
}

fn words(total: u16) -> usize {
    (total as usize).div_ceil(64)
}

pub fn write_ack(out: &mut Vec<u8>, ack: &ChunkAck) {
    out.clear();
    out.push(CHUNK_ACK_OP);
    out.extend_from_slice(&ack.chunk_id.to_le_bytes());
    out.extend_from_slice(&ack.total.to_le_bytes());
    for word in &ack.bits {
        out.extend_from_slice(&word.to_le_bytes());
    }
}

pub fn parse_ack(data: &[u8]) -> Result<ChunkAck, ChunkError> {
    if data.len() < 5 {
        return Err(ChunkError::Truncated);
    }
    if data[0] != CHUNK_ACK_OP {
        return Err(ChunkError::BadOp(data[0]));
    }
    let chunk_id = u16::from_le_bytes([data[1], data[2]]);
    let total = u16::from_le_bytes([data[3], data[4]]);
    if total == 0 || total as usize > MAX_SLICES {
        return Err(ChunkError::BadIndex { index: 0, total });
    }
    let want = words(total) * 8;
    if data.len() - 5 < want {
        return Err(ChunkError::Truncated);
    }
    let mut bits = Vec::with_capacity(words(total));
    for chunk in data[5..5 + want].chunks_exact(8) {
        bits.push(u64::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(ChunkAck {
        chunk_id,
        total,
        bits,
    })
}

/// Sends one block. `next_wire` emits slices; `on_ack` consumes the receiver's
/// bitfield; only the missing slices are re-emitted after [`RETRY_MS`].
#[derive(Clone, Debug)]
pub struct ChunkSender {
    chunk_id: u16,
    data: Vec<u8>,
    slice_size: usize,
    total: u16,
    sent: Vec<u64>,
    acked: Vec<u64>,
    last_pass_ms: u64,
    attempts: u32,
}

impl ChunkSender {
    pub fn new(chunk_id: u16, data: Vec<u8>, slice_size: usize) -> Result<Self, ChunkError> {
        if data.is_empty() {
            return Err(ChunkError::Empty);
        }
        let slice_size = slice_size.clamp(1, DEFAULT_SLICE_SIZE);
        let total = data.len().div_ceil(slice_size);
        if total > MAX_SLICES {
            return Err(ChunkError::TooLarge { bytes: data.len() });
        }
        Ok(Self {
            chunk_id,
            data,
            slice_size,
            total: total as u16,
            sent: vec![0; words(total as u16)],
            acked: vec![0; words(total as u16)],
            last_pass_ms: 0,
            attempts: 0,
        })
    }

    pub fn chunk_id(&self) -> u16 {
        self.chunk_id
    }

    pub fn total(&self) -> u16 {
        self.total
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn is_complete(&self) -> bool {
        (0..self.total).all(|i| Self::bit(&self.acked, i))
    }

    pub fn missing(&self) -> usize {
        (0..self.total).filter(|i| !Self::bit(&self.acked, *i)).count()
    }

    fn bit(bits: &[u64], index: u16) -> bool {
        let word = index as usize / 64;
        let bit = index as usize % 64;
        bits.get(word).is_some_and(|w| w & (1u64 << bit) != 0)
    }

    fn set(bits: &mut [u64], index: u16) {
        let word = index as usize / 64;
        let bit = index as usize % 64;
        if let Some(w) = bits.get_mut(word) {
            *w |= 1u64 << bit;
        }
    }

    fn clear(bits: &mut [u64], index: u16) {
        let word = index as usize / 64;
        let bit = index as usize % 64;
        if let Some(w) = bits.get_mut(word) {
            *w &= !(1u64 << bit);
        }
    }

    /// Next slice to put on the wire, or `None` when the block is complete or
    /// every outstanding slice was sent less than [`RETRY_MS`] ago.
    pub fn next_wire(&mut self, now_ms: u64, out: &mut Vec<u8>) -> Option<u16> {
        if self.is_complete() {
            return None;
        }
        // First pass: everything not yet sent. A retry pass clears the `sent`
        // bits of the missing slices, so the same loop walks them one by one.
        let mut pick = None;
        for i in 0..self.total {
            if !Self::bit(&self.sent, i) && !Self::bit(&self.acked, i) {
                pick = Some(i);
                break;
            }
        }
        if pick.is_none() {
            if now_ms.saturating_sub(self.last_pass_ms) < RETRY_MS {
                return None;
            }
            self.last_pass_ms = now_ms;
            self.attempts = self.attempts.saturating_add(1);
            for i in 0..self.total {
                if !Self::bit(&self.acked, i) {
                    Self::clear(&mut self.sent, i);
                }
            }
            for i in 0..self.total {
                if !Self::bit(&self.sent, i) && !Self::bit(&self.acked, i) {
                    pick = Some(i);
                    break;
                }
            }
        }
        let index = pick?;
        let begin = index as usize * self.slice_size;
        let end = (begin + self.slice_size).min(self.data.len());
        write_slice(out, self.chunk_id, index, self.total, &self.data[begin..end]);
        Self::set(&mut self.sent, index);
        Some(index)
    }

    pub fn on_ack(&mut self, ack: &ChunkAck) -> Result<bool, ChunkError> {
        if ack.chunk_id != self.chunk_id {
            return Err(ChunkError::ChunkMismatch {
                got: ack.chunk_id,
                want: self.chunk_id,
            });
        }
        if ack.total != self.total {
            return Err(ChunkError::TotalMismatch {
                got: ack.total,
                want: self.total,
            });
        }
        self.acked = ack.bits.clone();
        Ok(self.is_complete())
    }
}

#[derive(Clone, Debug)]
pub struct ChunkReceiver {
    chunk_id: u16,
    total: u16,
    slices: Vec<Option<Vec<u8>>>,
    received: Vec<u64>,
    bytes: usize,
}

impl Default for ChunkReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkReceiver {
    pub fn new() -> Self {
        Self {
            chunk_id: 0,
            total: 0,
            slices: Vec::new(),
            received: Vec::new(),
            bytes: 0,
        }
    }

    pub fn chunk_id(&self) -> Option<u16> {
        (self.total > 0).then_some(self.chunk_id)
    }

    pub fn total(&self) -> u16 {
        self.total
    }

    pub fn received(&self) -> usize {
        (0..self.total).filter(|i| ChunkSender::bit(&self.received, *i)).count()
    }

    pub fn ack(&self) -> ChunkAck {
        ChunkAck {
            chunk_id: self.chunk_id,
            total: self.total,
            bits: self.received.clone(),
        }
    }

    /// Accept one slice. Returns `Some(block)` once every slice is present.
    /// Duplicates are accepted but only counted once.
    pub fn accept(&mut self, wire: &[u8]) -> Result<Option<Vec<u8>>, ChunkError> {
        let slice = parse_slice(wire)?;
        if self.total == 0 {
            self.begin(slice.chunk_id, slice.total);
        } else if slice.chunk_id != self.chunk_id {
            return Err(ChunkError::ChunkMismatch {
                got: slice.chunk_id,
                want: self.chunk_id,
            });
        } else if slice.total != self.total {
            return Err(ChunkError::TotalMismatch {
                got: slice.total,
                want: self.total,
            });
        }
        let index = slice.index as usize;
        if self.slices[index].is_none() {
            self.bytes += slice.data.len();
            self.slices[index] = Some(slice.data.to_vec());
            ChunkSender::set(&mut self.received, slice.index);
        }
        if self.received() as u16 != self.total {
            return Ok(None);
        }
        let mut block = Vec::with_capacity(self.bytes);
        for part in &self.slices {
            let Some(part) = part else { return Ok(None) };
            block.extend_from_slice(part);
        }
        Ok(Some(block))
    }

    fn begin(&mut self, chunk_id: u16, total: u16) {
        self.chunk_id = chunk_id;
        self.total = total;
        self.slices = vec![None; total as usize];
        self.received = vec![0; words(total)];
        self.bytes = 0;
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn a_large_block_survives_loss_and_only_missing_slices_resend() {
        let data = block(256 * 1024);
        let mut sender = ChunkSender::new(9, data.clone(), DEFAULT_SLICE_SIZE).unwrap();
        assert_eq!(sender.total(), 256);
        let mut receiver = ChunkReceiver::new();
        let mut wire = Vec::new();
        let mut dropped = 0;

        while let Some(index) = sender.next_wire(0, &mut wire) {
            // Deterministic 1-in-17 loss.
            if index % 17 == 5 {
                dropped += 1;
                continue;
            }
            receiver.accept(&wire).expect("slice");
            assert!(sender.on_ack(&receiver.ack()).is_ok());
        }
        assert_eq!(dropped, 15);
        assert!(!sender.is_complete());

        // Retry pass: only the gaps are emitted.
        let mut resent = 0;
        let mut retried = 0;
        while let Some(_) = sender.next_wire(100, &mut wire) {
            resent += 1;
            let _slice = parse_slice(&wire).unwrap();
            retried += 1;
            receiver.accept(&wire).expect("retry slice");
            assert!(sender.on_ack(&receiver.ack()).is_ok());
            if retried > 512 {
                panic!("retry did not converge");
            }
        }
        assert_eq!(resent, 15, "only the dropped slices resent");
        assert!(sender.is_complete());
        assert_eq!(receiver.ack().total, 256);
    }

    #[test]
    fn out_of_order_and_duplicate_slices_reassemble_once() {
        let data = block(5_000);
        let mut sender = ChunkSender::new(3, data.clone(), 1000).unwrap();
        let mut frames = Vec::new();
        let mut wire = Vec::new();
        while sender.next_wire(0, &mut wire).is_some() {
            frames.push(wire.clone());
        }
        let mut receiver = ChunkReceiver::new();
        for index in [4usize, 0, 2, 2, 1, 4, 3] {
            let got = receiver.accept(&frames[index]).unwrap();
            if index == 3 {
                assert_eq!(got, Some(data.clone()));
            } else {
                assert!(got.is_none());
            }
        }
        assert_eq!(receiver.received(), 5);
        assert_eq!(receiver.accept(&frames[2]).unwrap(), Some(data));
    }

    #[test]
    fn a_lost_ack_makes_the_sender_resend_until_complete() {
        let data = block(3_000);
        let mut sender = ChunkSender::new(1, data, 1000).unwrap();
        let mut wire = Vec::new();
        for _ in 0..3 {
            sender.next_wire(0, &mut wire).unwrap();
        }
        let ack = ChunkAck::new(1, 3);
        assert_eq!(sender.next_wire(10, &mut wire), None, "retry is paced");
        assert_eq!(sender.next_wire(RETRY_MS, &mut wire), Some(0));
        assert!(!sender.on_ack(&ack).unwrap());
        let mut done = ChunkAck::new(1, 3);
        done.set(0);
        done.set(1);
        done.set(2);
        assert!(sender.on_ack(&done).unwrap());
    }

    #[test]
    fn oversized_and_malformed_blocks_are_refused() {
        assert!(matches!(
            ChunkSender::new(1, Vec::new(), DEFAULT_SLICE_SIZE),
            Err(ChunkError::Empty)
        ));
        assert!(matches!(
            ChunkSender::new(1, vec![0; MAX_BLOCK + 1], DEFAULT_SLICE_SIZE),
            Err(ChunkError::TooLarge { .. })
        ));
        assert_eq!(parse_slice(&[CHUNK_SLICE_OP]).unwrap_err(), ChunkError::Truncated);
        let mut wire = Vec::new();
        write_slice(&mut wire, 1, 4, 4, &[1]);
        assert!(matches!(parse_slice(&wire), Err(ChunkError::BadIndex { .. })));
        write_slice(&mut wire, 1, 0, 4, &[1, 2]);
        wire.pop();
        assert!(matches!(parse_slice(&wire), Err(ChunkError::BadLength { .. })));
    }

    #[test]
    fn chunk_ids_wrap_and_stale_acks_are_refused() {
        let mut a = ChunkSender::new(u16::MAX, vec![1; 10], 4).unwrap();
        let b = ChunkSender::new(0, vec![2; 10], 4).unwrap();
        assert_eq!(a.chunk_id(), u16::MAX);
        assert_eq!(b.chunk_id(), 0);
        let mut ack = ChunkAck::new(1, a.total());
        assert!(matches!(
            a.on_ack(&ack),
            Err(ChunkError::ChunkMismatch { .. })
        ));
        ack.chunk_id = u16::MAX;
        assert!(!a.on_ack(&ack).unwrap());
    }
}
