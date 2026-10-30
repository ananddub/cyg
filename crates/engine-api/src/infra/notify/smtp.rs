//! One SMTP client, spoken directly rather than through a library: the whole
//! protocol is a line-oriented conversation, and the alternative is a
//! dependency with a template engine, a TLS backend and a start-time
//! configuration type attached to it.
//!
//! Three transports are worth having. `none` is for a relay on this machine's
//! loopback, `starttls` upgrades an ordinary connection before any credential
//! is typed into it, and `implicit` is TLS from the first byte. Credentials are
//! optional: a relay that offers `AUTH LOGIN` is given them and one that does
//! not is used as an open local relay, which is what a test runs.

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

use super::settings::{Smtp, Transport};
use super::{CHANNEL_EMAIL, Message, PROVIDER_SMTP, Receipt, Reply, Vendor};
use crate::infra::liveops;

pub struct Sender {
    settings: Smtp,
}

impl Sender {
    pub fn new(settings: Smtp) -> Sender {
        Sender { settings }
    }
}

impl Vendor for Sender {
    fn provider(&self) -> &'static str {
        PROVIDER_SMTP
    }

    fn channel(&self) -> &'static str {
        CHANNEL_EMAIL
    }

    /// The relay owns how long this takes, so it gets a deadline rather than a
    /// vote: a relay that stops answering is owed another attempt later, not a
    /// worker parked forever.
    fn send(&self, message: Message) -> Reply<'_> {
        Box::pin(async move {
            let budget = Duration::from_millis(liveops::config().notify_timeout_ms.max(1_000));
            tokio::time::timeout(budget, self.converse(&message))
                .await
                .map_err(|_| {
                    format!(
                        "the relay did not answer within {} seconds",
                        budget.as_secs()
                    )
                })?
        })
    }
}

impl Sender {
    async fn converse(&self, message: &Message) -> Result<Receipt, String> {
        let host = self.settings.host.clone();
        let port = self.settings.port;
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|err| format!("the relay could not be reached: {err}"))?;
        match self.settings.transport {
            Transport::Plain => self.ordered(BufShip::new(tcp), message, false).await,
            Transport::Implicit => {
                let secure = connector()
                    .connect(name(&host)?, tcp)
                    .await
                    .map_err(|err| format!("the relay's TLS was refused: {err}"))?;
                self.ordered(BufShip::new(secure), message, true).await
            }
            Transport::StartTls => {
                let mut plain = BufShip::new(tcp);
                let offered = greet(&mut plain, &self.settings.ehlo).await?;
                if !offers(&offered, "STARTTLS") {
                    return Ok(Receipt::Refused(
                        "the relay will not upgrade the connection, and OX_EMAIL_TLS is not none"
                            .to_owned(),
                    ));
                }
                let (code, reply) = ask(&mut plain, "STARTTLS").await?;
                if code != 220 {
                    return Ok(Receipt::Later(format!(
                        "the relay refused to upgrade: {code} {}",
                        reply.join(" ")
                    )));
                }
                let secure = connector()
                    .connect(name(&host)?, plain.into_inner())
                    .await
                    .map_err(|err| format!("the relay's TLS was refused: {err}"))?;
                self.ordered(BufShip::new(secure), message, true).await
            }
        }
    }

    /// Everything after the connection is the right kind of connection.
    async fn ordered<S>(
        &self,
        mut stream: BufShip<S>,
        message: &Message,
        secure: bool,
    ) -> Result<Receipt, String>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let offered = greet(&mut stream, &self.settings.ehlo).await?;
        if !secure
            && self.settings.transport == Transport::StartTls
            && !self.settings.username.is_empty()
        {
            return Ok(Receipt::Refused(
                "a password is configured but the connection is not encrypted".to_owned(),
            ));
        }
        if !self.settings.username.is_empty() && offers(&offered, "AUTH") {
            login(&mut stream, &self.settings).await?;
        }
        deliver(&mut stream, &self.settings, message).await
    }
}

type BufShip<S> = tokio::io::BufStream<S>;

/// The banner the relay sends unasked, then the capability list.
async fn greet<S>(stream: &mut S, domain: &str) -> Result<Vec<String>, String>
where
    S: AsyncBufRead + AsyncWrite + Unpin,
{
    let (code, lines) = read_reply(stream).await?;
    if code != 220 {
        return Err(format!("the relay opened with {code}: {}", lines.join(" ")));
    }
    let (code, caps) = ask(stream, &format!("EHLO {domain}")).await?;
    if code != 250 {
        return Err(format!("the relay did not recognise us: {code}"));
    }
    Ok(caps)
}

/// `AUTH LOGIN`, which is a base64 of the credential and nothing more: the
/// point of the round trips is only that the password is not plain text on a
/// connection that might not be encrypted.
async fn login<S>(stream: &mut S, settings: &Smtp) -> Result<(), String>
where
    S: AsyncBufRead + AsyncWrite + Unpin,
{
    let (code, _) = ask(stream, "AUTH LOGIN").await?;
    if code != 334 {
        return Ok(());
    }
    let (code, _) = ask(stream, &b64(settings.username.as_bytes())).await?;
    if code != 334 {
        return Err(format!("the relay would not take the address: {code}"));
    }
    let (code, reply) = ask(stream, &b64(settings.password.as_bytes())).await?;
    if code == 235 {
        return Ok(());
    }
    Err(format!(
        "the relay refused these credentials: {code} {}",
        reply.join(" ")
    ))
}

async fn deliver<S>(stream: &mut S, settings: &Smtp, message: &Message) -> Result<Receipt, String>
where
    S: AsyncBufRead + AsyncWrite + Unpin,
{
    let (code, reply) = ask(stream, &format!("MAIL FROM:<{}>", settings.from)).await?;
    if code != 250 {
        return Ok(settle(code, &reply, "the sender"));
    }
    let (code, reply) = ask(stream, &format!("RCPT TO:<{}>", message.recipient)).await?;
    if code != 250 {
        return Ok(settle(code, &reply, "the recipient"));
    }
    let (code, reply) = ask(stream, "DATA").await?;
    if code != 354 {
        return Ok(settle(code, &reply, "the message"));
    }
    stream
        .write_all(letter(settings, message).as_bytes())
        .await
        .map_err(|err| err.to_string())?;
    stream.flush().await.map_err(|err| err.to_string())?;
    let (code, reply) = read_reply(stream).await?;
    let outcome = settle(code, &reply, "the relay");
    let _ = stream.write_all(b"QUIT\r\n").await;
    let _ = stream.flush().await;
    Ok(outcome)
}

/// One command and the reply it earns. Multi-line replies keep coming until a
/// line drops the dash.
async fn ask<S>(stream: &mut S, line: &str) -> Result<(u16, Vec<String>), String>
where
    S: AsyncBufRead + AsyncWrite + Unpin,
{
    stream
        .write_all(line.as_bytes())
        .await
        .map_err(|err| err.to_string())?;
    stream
        .write_all(b"\r\n")
        .await
        .map_err(|err| err.to_string())?;
    stream.flush().await.map_err(|err| err.to_string())?;
    read_reply(stream).await
}

async fn read_reply<S>(stream: &mut S) -> Result<(u16, Vec<String>), String>
where
    S: AsyncBufRead + Unpin,
{
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        if stream
            .read_line(&mut line)
            .await
            .map_err(|err| err.to_string())?
            == 0
        {
            return Err("the relay closed the connection mid-reply".to_owned());
        }
        let text = line.trim_end();
        let digits = text.get(..3).ok_or(format!("the relay said {text:?}"))?;
        let code: u16 = digits
            .parse()
            .map_err(|_| format!("the relay said {text:?}"))?;
        lines.push(text.get(4..).unwrap_or_default().to_owned());
        if text.get(3..4) != Some("-") {
            return Ok((code, lines));
        }
    }
}

/// A 4xx is the relay having a bad minute and a 5xx is its answer about this
/// particular message. Only the first is owed another try.
fn settle(code: u16, reply: &[String], what: &str) -> Receipt {
    let said = reply.join(" ");
    if (200..400).contains(&code) {
        Receipt::Accepted
    } else if code >= 500 {
        Receipt::Refused(format!("the relay refused {what}: {code} {said}"))
    } else {
        Receipt::Later(format!("the relay refused {what}: {code} {said}"))
    }
}

fn offers(caps: &[String], want: &str) -> bool {
    caps.iter()
        .any(|cap| cap.to_ascii_uppercase().starts_with(want))
}

/// The whole mail as one write, terminating dot included. The footer the
/// recipient reads is already in the body: this only wraps it in the headers a
/// mail client needs.
fn letter(settings: &Smtp, message: &Message) -> String {
    format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nDate: {}\r\n\
         Message-ID: <{}@{}>\r\nMIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\r\n{}\r\n.\r\n",
        settings.from,
        message.recipient,
        phrase(&message.title),
        chrono::Utc::now().to_rfc2822(),
        Uuid::new_v4(),
        settings.ehlo,
        stuff(&message.text),
    )
}

/// A line of body text that begins with a dot is the end of the mail as far as
/// the relay is concerned, so it has to arrive doubled.
fn stuff(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.starts_with('.') {
                format!(".{line}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// A subject that is not plain ASCII has to be encoded before it can be put on
/// the wire at all (RFC 2047).
fn phrase(text: &str) -> String {
    let flat = text.replace(['\r', '\n'], " ");
    if flat.is_ascii() {
        flat
    } else {
        format!("=?UTF-8?B?{}?=", b64(flat.as_bytes()))
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn name(host: &str) -> Result<ServerName<'static>, String> {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ServerName::try_from(ip)
            .map_err(|_| format!("{host} cannot be checked against a certificate"));
    }
    ServerName::try_from(host.to_owned())
        .map_err(|_| format!("{host} cannot be checked against a certificate"))
}

/// The roots are the ones Mozilla ships, compiled in: a container that has no
/// certificate store still reaches its relay.
fn connector() -> tokio_rustls::TlsConnector {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    let config = CONFIG.get_or_init(|| {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        Arc::new(
            rustls::ClientConfig::builder_with_provider(
                rustls::crypto::ring::default_provider().into(),
            )
            .with_safe_default_protocol_versions()
            .expect("rustls accepts its own default protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
        )
    });
    tokio_rustls::TlsConnector::from(config.clone())
}
