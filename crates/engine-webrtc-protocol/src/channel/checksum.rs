pub const OP_CHECKSUM: u8 = 0x44;

pub const CHECKSUM_WIRE_LEN: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumReport {
    pub tick: u64,
    pub checksum: u64,
}

pub fn write_checksum(out: &mut Vec<u8>, report: &ChecksumReport) {
    out.clear();
    out.reserve(CHECKSUM_WIRE_LEN);
    out.push(OP_CHECKSUM);
    out.extend_from_slice(&report.tick.to_le_bytes());
    out.extend_from_slice(&report.checksum.to_le_bytes());
}

pub fn parse_checksum(buf: &[u8]) -> Option<ChecksumReport> {
    if buf.len() != CHECKSUM_WIRE_LEN || buf[0] != OP_CHECKSUM {
        return None;
    }
    let le_u64 = |at: usize| {
        let mut a = [0u8; 8];
        a.copy_from_slice(&buf[at..at + 8]);
        u64::from_le_bytes(a)
    };
    Some(ChecksumReport {
        tick: le_u64(1),
        checksum: le_u64(9),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_survives_the_wire() {
        let report = ChecksumReport {
            tick: 4_294_967_296,
            checksum: 0x0123_4567_89ab_cdef,
        };
        let mut wire = Vec::new();
        write_checksum(&mut wire, &report);
        assert_eq!(wire.len(), CHECKSUM_WIRE_LEN);
        assert_eq!(parse_checksum(&wire), Some(report));
    }

    #[test]
    fn a_payload_of_the_wrong_length_is_not_a_report() {
        let mut wire = vec![OP_CHECKSUM];
        wire.extend_from_slice(&7u64.to_le_bytes());
        assert_eq!(parse_checksum(&wire), None, "9 bytes is a truncated report");
        wire.push(0);
        assert_eq!(parse_checksum(&wire), None, "18 bytes is not either");
    }
}
