use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use mio::{Events, Interest, Poll, Token};
use mio::net::UdpSocket;
use quiche::Type::Handshake;

const SOCKET: Token = Token(0);


fn main() -> io::Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    let mut socket = UdpSocket::bind("0.0.0.0:8000".parse().unwrap())?;
    poll.registry().register(&mut socket, SOCKET, Interest::READABLE)?;


    let mut recv = [0u8; 65545];
    let mut sent = [0u8; 65545];
    let mut client: HashSet<SocketAddr> = HashSet::new();
    let mut connections:HashMap<SocketAddr,RefCell<quiche::Connection>> = HashMap::new();

    loop {
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            if event.token() == SOCKET || !event.is_readable() {
                continue;
            }
            loop {
                let (len, peer) = match socket.recv_from(&mut recv){
                    Ok(result) => result,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        break;
                    },
                    Err(e) => return Err(e),
                };

                if !client.contains(&peer){
                   client.insert(peer);
                }

                let local = socket.local_addr();
                let info = quiche::RecvInfo{
                    from: peer,
                    to: local?
                };

                // match  peer.


            }
        }
    }

}