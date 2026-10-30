use std::sync::Arc;
use std::sync::Mutex;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// One mail as the relay saw it: every line of the letter, headers first.
#[derive(Debug, Clone)]
pub struct Mail {
    pub lines: Vec<String>,
}

impl Mail {
    pub fn says(&self, needle: &str) -> bool {
        self.lines.iter().any(|line| line.contains(needle))
    }
}

/// A loopback relay, which is the only honest way to ask what the SMTP client
/// actually put on the wire.
///
/// An address whose local part is `flaky` is turned away `FLAKY_REFUSALS` times
/// before it is taken, which is a relay having a bad afternoon; `bad@` is a
/// mailbox that is not there, which is a relay being certain.
pub struct Relay {
    pub port: u16,
    state: Arc<State>,
}

#[derive(Default)]
struct State {
    mail: Mutex<Vec<Mail>>,
    /// How many times each recipient has been offered, which is what makes a
    /// relay that is merely busy eventually agree.
    tries: Mutex<Vec<String>>,
}

const FLAKY_REFUSALS: usize = 2;

impl Relay {
    pub async fn listening() -> Arc<Relay> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback relay port");
        let port = listener.local_addr().expect("relay address").port();
        let state = Arc::new(State::default());
        let taken = state.clone();
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                let state = taken.clone();
                tokio::spawn(async move {
                    let _ = one(socket, state).await;
                });
            }
        });
        Arc::new(Relay { port, state })
    }

    /// The one letter offered to this local part, or nothing when it never
    /// arrived.
    pub fn letter_to(&self, local: &str) -> Option<Mail> {
        let want = format!("To: {local}@");
        self.state
            .mail
            .lock()
            .expect("the relay's log")
            .iter()
            .find(|letter| letter.says(&want))
            .cloned()
    }
}

async fn one(socket: TcpStream, state: Arc<State>) -> std::io::Result<()> {
    let mut io = BufReader::new(socket);
    let mut line = String::new();
    let mut rcpt = String::new();
    let mut bounced = false;
    say(&mut io, "220 relay.notify-e2e.test ESMTP\r\n").await?;
    loop {
        line.clear();
        if io.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let said = line.trim_end().to_ascii_uppercase();
        if said == "QUIT" {
            return say(&mut io, "221 bye\r\n").await;
        }
        if said == "DATA" {
            if bounced {
                // A mailbox that is not there is not offered a message.
                say(&mut io, "554 transaction failed\r\n").await?;
                continue;
            }
            say(&mut io, "354 go ahead\r\n").await?;
            let mut letter = Vec::new();
            loop {
                let mut body = String::new();
                if io.read_line(&mut body).await? == 0 {
                    return Ok(());
                }
                let flat = body.trim_end().to_owned();
                if flat == "." {
                    break;
                }
                letter.push(flat);
            }
            state
                .mail
                .lock()
                .expect("the relay's log")
                .push(Mail { lines: letter });
            let answer = if accept(&rcpt, &state) {
                "250 queued\r\n"
            } else {
                "451 now try later\r\n"
            };
            say(&mut io, answer).await?;
            continue;
        }
        let reply = if said.starts_with("EHLO") || said.starts_with("HELO") {
            "250-relay.notify-e2e.test\r\n250 SIZE 1048576\r\n"
        } else if said.starts_with("MAIL FROM") {
            "250 sender accepted\r\n"
        } else if let Some(rest) = said.strip_prefix("RCPT TO:") {
            rcpt = rest.to_owned();
            bounced = rcpt.contains("BAD@");
            if bounced {
                "550 5.1.1 that mailbox is not here\r\n"
            } else {
                "250 recipient accepted\r\n"
            }
        } else if said == "RSET" || said == "NOOP" {
            "250 very well\r\n"
        } else {
            "500 that is not a command\r\n"
        };
        say(&mut io, reply).await?;
    }
}

/// `flaky@` is turned away twice and then taken; `bad@` is a refusal, not a
/// delay, and a refusal is the vendor being certain rather than busy.
fn accept(rcpt: &str, state: &State) -> bool {
    let lower = rcpt.to_ascii_lowercase();
    if lower.contains("bad@") {
        return false;
    }
    if !lower.contains("flaky@") {
        return true;
    }
    let mut tries = state.tries.lock().expect("the relay's tally");
    tries.push(lower);
    tries.len() > FLAKY_REFUSALS
}

async fn say<R>(io: &mut BufReader<R>, text: &str) -> std::io::Result<()>
where
    R: AsyncRead + AsyncWrite + Unpin,
{
    let socket = io.get_mut();
    socket.write_all(text.as_bytes()).await?;
    socket.flush().await
}
