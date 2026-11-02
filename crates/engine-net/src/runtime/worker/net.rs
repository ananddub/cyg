use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use mio::event::Event;
use mio::net::TcpListener;
use mio::{Interest, Poll, Token};

use super::ClientPacket;
use crate::runtime::connection::ClientConn;

pub(super) fn accept_pending(
    poll: &mut Poll,
    listener: &mut TcpListener,
    clients: &mut HashMap<Token, ClientConn>,
    next_token: &mut usize,
) {
    loop {
        match listener.accept() {
            Ok((mut stream, peer_addr)) => {
                let token = Token(*next_token);
                *next_token += 1;

                if let Err(e) = stream.set_nodelay(true) {
                    tracing::error!("Failed to set nodelay: {}", e);
                }

                if let Err(e) = poll.registry().register(
                    &mut stream,
                    token,
                    Interest::READABLE | Interest::WRITABLE,
                ) {
                    tracing::error!("Failed to register client: {}", e);
                    continue;
                }

                let session_id: Arc<str> = format!("sess_{}_{}", token.0, peer_addr.port()).into();
                clients.insert(token, ClientConn::new(stream, session_id));
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => {
                tracing::error!("Accept error: {}", e);
                break;
            }
        }
    }
}

pub(super) fn service_client(
    clients: &mut HashMap<Token, ClientConn>,
    client_token: Token,
    event: &Event,
    packets: &mut Vec<ClientPacket>,
) -> bool {
    let Some(conn) = clients.get_mut(&client_token) else {
        return false;
    };
    let mut conn_disconnected = false;

    if event.is_readable() {
        match conn.do_read() {
            Ok(0) => conn_disconnected = true,
            Ok(_) => {
                while let Some((opcode, payload)) = conn.next_frame() {
                    packets.push(ClientPacket {
                        session_id: conn.session_id.clone(),
                        token: client_token,
                        opcode,
                        payload,
                    });
                }
            }
            Err(e) => {
                tracing::error!("Socket read error: {}", e);
                conn_disconnected = true;
            }
        }
    }

    if event.is_writable() && conn.has_pending_writes() {
        if let Err(e) = conn.do_write() {
            tracing::error!("Socket write error: {}", e);
            conn_disconnected = true;
        }
    }
    conn_disconnected
}
