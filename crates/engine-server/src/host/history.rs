use std::sync::atomic::{AtomicU64, Ordering};

use engine_bus::{Bus, SeatUpdate, SimEvent};

use crate::rooms::OutboundMessage;

pub(crate) struct EventHistory<'a> {
    pub(crate) bus: &'a Bus,
    pub(crate) record: &'a engine_config::GameplayConfig,
    pub(crate) seq: AtomicU64,
}

impl<'a> EventHistory<'a> {
    pub(crate) fn new(bus: &'a Bus, record: &'a engine_config::GameplayConfig) -> Self {
        Self {
            bus,
            record,
            seq: AtomicU64::new(0),
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

    pub(crate) fn bridge_seat(&self, room: &str, seat: i64, user_id: Option<String>) {
        self.bus.push_seat(SeatUpdate {
            room: room.to_string(),
            seat,
            user_id,
        });
    }
}
