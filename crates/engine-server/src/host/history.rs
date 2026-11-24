use std::sync::atomic::{AtomicU64, Ordering};

use engine_bus::{Bus, SeatUpdate, SimEvent};

use crate::rooms::{OutboundMessage, StatIntent};

pub(crate) struct EventHistory<'a> {
    pub(crate) bus: &'a Bus,
    pub(crate) record: &'a engine_config::GameplayConfig,
    pub(crate) seq: AtomicU64,
    /// Identifies this host process. `seq` restarts at zero every boot, so the
    /// pair is what makes a stat write idempotent across restarts.
    boot_id: String,
    write_seq: AtomicU64,
}

impl<'a> EventHistory<'a> {
    pub(crate) fn new(bus: &'a Bus, record: &'a engine_config::GameplayConfig) -> Self {
        Self {
            bus,
            record,
            seq: AtomicU64::new(0),
            boot_id: uuid::Uuid::new_v4().to_string(),
            write_seq: AtomicU64::new(0),
        }
    }

    pub(crate) fn emit(
        &self,
        room: &str,
        tick: u64,
        target: i64,
        event: std::sync::Arc<str>,
        payload: bytes::Bytes,
    ) {
        self.bus.push_event(SimEvent {
            room: room.to_string(),
            tick: tick as i64,
            seq: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            target,
            event,
            payload,
        });
    }

    pub(crate) fn mirror(&self, room: &str, tick: u64, message: &OutboundMessage) {
        if !self.record.records(&message.event) {
            return;
        }
        self.emit(
            room,
            tick,
            message.target,
            message.event.clone(),
            message.payload.clone(),
        );
    }

    pub(crate) fn carries_result(&self, message: &OutboundMessage) -> bool {
        message.event.as_ref() == self.record.result_event.as_str()
    }

    pub(crate) fn record_desync(
        &self,
        room: &str,
        target: i64,
        desync: &engine_wasm::DesyncReport,
    ) {
        let mut payload = Vec::with_capacity(16);
        payload.extend_from_slice(&desync.expected_checksum.to_le_bytes());
        payload.extend_from_slice(&desync.client_checksum.to_le_bytes());
        self.emit(
            room,
            desync.tick,
            target,
            std::sync::Arc::from("desync"),
            bytes::Bytes::from(payload),
        );
    }

    /// Coalesces one guest call's stat intents per (seat, counter) and queues
    /// them for the control plane. A tick that raised `kills` ten times becomes
    /// one `+10` write, not ten.
    pub(crate) fn record_stats(&self, room: &str, tick: u64, intents: Vec<StatIntent>) {
        let mut merged: std::collections::HashMap<(i64, std::sync::Arc<str>), i64> =
            std::collections::HashMap::new();
        for intent in intents {
            *merged.entry((intent.seat, intent.stat)).or_default() += intent.delta;
        }
        for ((seat, stat), delta) in merged {
            if delta <= 0 {
                continue;
            }
            let seq = self.write_seq.fetch_add(1, Ordering::Relaxed) + 1;
            self.bus.push_stat(engine_bus::StatWrite {
                room: room.to_string(),
                write_id: format!("{}:{seq}", self.boot_id),
                tick: tick as i64,
                seat,
                stat: stat.to_string(),
                delta,
            });
        }
    }

    pub(crate) fn bridge_seat(&self, room: &str, seat: i64, user_id: Option<String>) {
        self.bus.push_seat(SeatUpdate {
            room: room.to_string(),
            seat,
            user_id,
        });
    }
}
