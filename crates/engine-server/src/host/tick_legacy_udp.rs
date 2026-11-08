use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_legacy_udp(&mut self, ctx: &mut TickCtx, _name: &str, server_tick_u32: u32) {
        let delta = &self.delta_buf[..];
        if let Some(ref udp_sock) = ctx.udp.udp_sock {
            let peers = &ctx.udp.udp_peers;
            // Reuse the batch's frame buffers across ticks instead of cloning a
            // fresh Vec per peer; only the embedded player id differs per peer.
            self.udp_batch.resize_with(peers.len(), || {
                (std::net::SocketAddr::from(([0, 0, 0, 0], 0)), Vec::new())
            });
            for ((&peer_addr, &(id, _, _)), slot) in peers.iter().zip(self.udp_batch.iter_mut()) {
                slot.0 = peer_addr;
                netcode::wrap_delta_into(&mut slot.1, id, delta, 0, server_tick_u32);
            }
            engine_net::transport::send_datagram_batch(udp_sock, &self.udp_batch);
        }
    }
}
