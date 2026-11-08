use engine_net::transport::TransportEvent;

use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn dispatch_events(&mut self) {
        let batch = self.scratch.events.len().min(MAX_FRAMES_PER_LOOP);
        let drained: Vec<TransportEvent> = self.scratch.events.drain(..batch).collect();
        for event in drained {
            match event {
                TransportEvent::Connected { client_id, .. } => {
                    let _ = self.run_connected(&client_id);
                }
                TransportEvent::Disconnected { client_id } => {
                    self.run_disconnected(&client_id);
                }
                TransportEvent::Packet {
                    client_id, payload, ..
                } => {
                    self.run_packet(&client_id, payload);
                }
            }
        }
    }
}
