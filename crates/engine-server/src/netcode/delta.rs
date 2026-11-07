use engine_net::transport::WebRtcServer;

use crate::rooms::SessionSeqs;

use super::DeltaEntity;

pub fn parse_delta_sections(
    delta: &[u8],
) -> Option<(u32, Vec<DeltaEntity>, u32, usize, usize)> {
    let (ver, entities, bullets, (bc_off, bullet_end)) = engine_spatial::aoi::parse_delta(delta)?;
    Some((ver, entities, bullets, bc_off, bullet_end))
}

pub fn filter_delta_for_viewer(
    entities: &[DeltaEntity],
    bullet_count: u32,
    bullets_range: (usize, usize),
    delta: &[u8],
    viewer_id: i64,
    viewer_pos: Option<[f64; 3]>,
    tick: u64,
    aoi: &engine_config::AoiConfig,
    out: &mut Vec<u8>,
) {
    engine_spatial::aoi::filter_for_viewer(
        entities,
        bullet_count,
        bullets_range,
        delta,
        viewer_id,
        viewer_pos,
        tick,
        aoi,
        out,
    );
}

pub fn send_delta_framed(
    webrtc_server: &mut WebRtcServer,
    session: &str,
    seq: u16,
    tick: u32,
    body: &[u8],
) -> usize {
    match webrtc_server.send_delta(session, seq, tick, body) {
        Ok(report) => {
            if report.delivered() {
                report.fragments
            } else {
                0
            }
        }
        Err(e) => {
            tracing::error!("[server] ⚠️ delta refused for {session}: {e} — dropped");
            0
        }
    }
}

pub fn record_channel_ack(
    room: &mut crate::rooms::GameRoom,
    client_id: &str,
    ack: engine_webrtc_protocol::Ack,
) {
    // Keyed by player id (session-slab step 2): an ack from a session the
    // room no longer seats has no player to acknowledge — drop it.
    let Some(&id) = room.sessions.get(client_id) else {
        return;
    };
    let entry = room
        .seqs
        .entry(id)
        .or_insert(SessionSeqs { input: 0, acked_event: 0 });
    if engine_webrtc_protocol::is_newer(entry.acked_event, ack.seq) {
        entry.acked_event = ack.seq;
    }
}
