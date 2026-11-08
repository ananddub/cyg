use engine_net::framing::OpCode;

use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_schema_udp(
        &mut self,
        ctx: &mut TickCtx,
        name: &str,
        force_full: bool,
        server_tick_u32: u32,
    ) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let delta = &self.delta_buf[..];
        let Some(schema) = netcode::split_schema(delta) else {
            return;
        };
        if let Some(ref udp_sock) = ctx.udp.udp_sock {
            for (peer_addr, (peer_id, peer_session, _)) in &ctx.udp.udp_peers {
                let is_full = force_full || !room.schema_sent.contains(peer_session.as_str());
                let body = if is_full { schema.full } else { schema.patch };
                netcode::wrap_delta_into(
                    &mut self.delta_body,
                    *peer_id as i64,
                    body,
                    0,
                    server_tick_u32,
                );
                let _ = udp_sock.send_to(&self.delta_body, *peer_addr);
                if is_full {
                    room.schema_sent
                        .insert(std::rc::Rc::from(peer_session.as_str()));
                }
            }
        }
    }

    pub(crate) fn tick_schema_spectators(
        &mut self,
        ctx: &mut TickCtx,
        name: &str,
        force_full: bool,
        server_tick_u32: u32,
        delta_seq: u16,
    ) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let delta = &self.delta_buf[..];
        let Some(schema) = netcode::split_schema(delta) else {
            return;
        };
        let spectators: Vec<std::rc::Rc<str>> = room.spectators.iter().cloned().collect();
        for spec in spectators {
            let is_full = force_full || !room.schema_sent.contains(spec.as_ref());
            let body = if is_full { schema.full } else { schema.patch };
            netcode::wrap_delta_into(&mut self.delta_body, 0, body, 0, server_tick_u32);
            let framed = ctx.transport.send_delta_framed(
                &spec,
                delta_seq,
                server_tick_u32,
                &self.delta_body,
            );
            if framed == 0 {
                if is_full {
                    let _ = ctx.transport.send_state(&spec, &self.delta_body);
                } else {
                    let _ = ctx
                        .transport
                        .send_to(&spec, OpCode::RoomDataBytes, &self.delta_body);
                }
            }
        }
    }
}
