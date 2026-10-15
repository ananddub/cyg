mod events;
mod inputs;
mod states;

use engine_webrtc_protocol::EntitySnapshot;

pub(crate) fn snapshot(id: u16) -> EntitySnapshot {
    EntitySnapshot {
        id,
        x: 12.34,
        y: -0.5,
        z: 499.9375,
        yaw: -2.5,
        pitch: 0.75,
        vx: 160.0,
        vy: -3.25,
        vz: 0.0,
        state: 0b101,
    }
}
