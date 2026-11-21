//! Regenerates the captured client fixtures into the packed wire.
//!
//! The capture in `bench/capture_cyg_deltas.mjs` stores the frames a server
//! sent. Those frames used to be fixed-width; clients only speak the packed
//! wire now, so the fixture is transcoded once with the same `PackedSet` the
//! host uses at runtime. `expected.json` is unchanged: the state is identical.
//!
//!   cargo run -p engine-server --example repack_fixtures
//!
//! Then rebuild the Unity/Godot native core before rerunning their parity
//! tests (they load `libcyg_core_rs.so`).

use std::path::Path;

use engine_server::netcode::PackedSet;

const PREFIX: &[u8] = b"\xA5delta";

fn main() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cyg-core-rs/tests/fixtures");
    let descriptor = std::fs::read(fixtures.join("descriptor.bin")).expect("descriptor.bin");
    let payloads = std::fs::read(fixtures.join("payloads.bin")).expect("payloads.bin");
    let mut set = PackedSet::from_descriptor(&descriptor).expect("descriptor has schemas");

    let frames = read_frames(&payloads);
    let mut out = Vec::with_capacity(payloads.len());
    let (mut repacked, mut skipped) = (0usize, 0usize);

    for frame in frames {
        let Some(prefix_len) = delta_prefix_len(frame) else {
            write_frame(&mut out, frame);
            skipped += 1;
            continue;
        };
        let body_at = prefix_len + 8; // your_id i64
        if body_at + 8 > frame.len() {
            write_frame(&mut out, frame);
            skipped += 1;
            continue;
        }
        // A raw capture carries a 6-byte envelope tail (acked + tick) and one
        // guest section. This tool upgrades it once; packed input is rejected
        // on purpose so a second run cannot silently double-append the stamp.
        let Some(full_kind) = frame.get(body_at + 4).copied() else {
            write_frame(&mut out, frame);
            skipped += 1;
            continue;
        };
        let Some(tail_at) = frame.len().checked_sub(6) else {
            write_frame(&mut out, frame);
            skipped += 1;
            continue;
        };
        let body = &frame[body_at..tail_at];
        let new_body = match full_kind {
            1 | 3 => set.full(body),
            2 | 4 => set.patch(body),
            _ => Err(format!("kind {full_kind} is already packed; use the raw capture")),
        };
        let Ok(new_body) = new_body else {
            write_frame(&mut out, frame);
            skipped += 1;
            continue;
        };
        let mut new_frame = Vec::with_capacity(prefix_len + 8 + new_body.len() + 8);
        new_frame.extend_from_slice(&frame[..prefix_len]);
        new_frame.extend_from_slice(&frame[prefix_len..body_at]);
        new_frame.extend_from_slice(&new_body);
        new_frame.extend_from_slice(&frame[tail_at..]);
        new_frame.extend_from_slice(&[0, 0]); // client stamp
        write_frame(&mut out, &new_frame);
        repacked += 1;
    }

    std::fs::write(fixtures.join("payloads.bin"), &out).expect("write payloads.bin");
    println!(
        "repacked {repacked} delta frames, left {skipped} non-delta frames alone ({} bytes)",
        out.len()
    );
}

/// `[0x11][0xA5 "delta"]` (7) or `[0xA5 "delta"]` (6); `None` for other frames.
fn delta_prefix_len(frame: &[u8]) -> Option<usize> {
    if frame.len() > 7 && frame[0] == 0x11 && frame[1..7] == *PREFIX {
        Some(7)
    } else if frame.len() > 6 && frame[0..6] == *PREFIX {
        Some(6)
    } else {
        None
    }
}

fn read_frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut at = 0;
    while at + 4 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        if at + len > bytes.len() {
            break;
        }
        frames.push(&bytes[at..at + len]);
        at += len;
    }
    frames
}

fn write_frame(out: &mut Vec<u8>, frame: &[u8]) {
    out.extend_from_slice(&(frame.len() as u32).to_le_bytes());
    out.extend_from_slice(frame);
}
