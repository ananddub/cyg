use engine_webrtc_protocol::channel::MAX_DELTA_CHUNK;
use engine_webrtc_protocol::{EntitySnapshot, write_delta_frame};

pub fn snapshot(id: u16) -> EntitySnapshot {
    EntitySnapshot {
        id,
        x: 1.0,
        y: 2.0,
        z: 3.0,
        yaw: 0.0,
        pitch: 0.0,
        vx: 0.0,
        vy: 0.0,
        vz: 0.0,
        state: 0,
    }
}

pub fn first_fragment(seq: u16, tick: u32, total: u8) -> Vec<u8> {
    let chunk = [0xABu8; MAX_DELTA_CHUNK];
    let mut frame = Vec::new();
    write_delta_frame(&mut frame, seq, tick, 0, total, &chunk);
    frame
}
