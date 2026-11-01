use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_legacy_udp(&mut self, ctx: &mut TickCtx, _name: &str, server_tick_u32: u32) {
        let delta = &self.delta_buf[..];
        if let Some(ref udp_sock) = ctx.udp.udp_sock {
            self.udp_batch.clear();
            for (&peer_addr, &(id, _, _)) in &ctx.udp.udp_peers {
                netcode::wrap_delta_into(&mut self.frame_buf, id, delta, 0, server_tick_u32);
                self.udp_batch.push((peer_addr, self.frame_buf.clone()));
            }
            engine_net::transport::send_datagram_batch(udp_sock, &self.udp_batch);
        }
    }
}
