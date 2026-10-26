//! One bounded, cancellable transport for the process-owned Edge instance.
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};
use tungstenite::{handshake::HandshakeError, protocol::WebSocketConfig, Message};

const REQUEST_LIMIT: usize = 1024 * 1024;
#[cfg(test)]
#[path = "kickthemap_cdp_tests.rs"]
mod tests;
#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub duration: Duration,
    pub response_bytes: usize,
}
struct Stream<'a> {
    socket: TcpStream,
    cancel: &'a AtomicBool,
    deadline: Instant,
}
impl Stream<'_> {
    fn guard(&self) -> io::Result<()> {
        if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::other("Edge-opdracht geannuleerd"));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Edge-antwoord duurde te lang",
            ));
        }
        Ok(())
    }
}
impl Read for Stream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.guard()?;
        self.socket.read(buffer)
    }
}
impl Write for Stream<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.guard()?;
        self.socket.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.guard()?;
        self.socket.flush()
    }
}
fn check(cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
    crate::input::check(cancel)?;
    if Instant::now() >= deadline {
        return Err("Edge-antwoord duurde te lang".into());
    }
    Ok(())
}
fn pending(error: &tungstenite::Error) -> bool {
    matches!(error,tungstenite::Error::Io(e) if e.kind()==io::ErrorKind::WouldBlock)
}
fn pause(cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
    check(cancel, deadline)?;
    thread::sleep(Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())));
    Ok(())
}
pub(super) fn command(
    port: u16,
    target: &str,
    method: &str,
    params: Value,
    cancel: &AtomicBool,
    limits: Limits,
) -> Result<Value, String> {
    let deadline = Instant::now()
        .checked_add(limits.duration)
        .ok_or("Ongeldig Edge-tijdbudget")?;
    check(cancel, deadline)?;
    if target.len() > 4096
        || limits.response_bytes == 0
        || limits.response_bytes > super::MAX_CDP_RESPONSE
    {
        return Err("Ongeldig Edge-antwoordbudget of doel".into());
    }
    super::validated_ws_url(target, port)?;
    let request = json!({"id":1,"method":method,"params":params}).to_string();
    if request.len() > REQUEST_LIMIT {
        return Err("Edge-opdracht is te groot".into());
    }
    let url = reqwest::Url::parse(target).map_err(|_| "Ongeldig Edge-doel")?;
    let ip = if matches!(url.host_str(), Some("::1" | "[::1]")) {
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    };
    check(cancel, deadline)?;
    let socket = TcpStream::connect_timeout(
        &SocketAddr::new(ip, port),
        Duration::from_millis(200).min(deadline.saturating_duration_since(Instant::now())),
    )
    .map_err(|e| format!("Edge-CDP-verbinding: {e}"))?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    let stream = Stream {
        socket,
        cancel,
        deadline,
    };
    let config = WebSocketConfig::default()
        .read_buffer_size(16 * 1024)
        .write_buffer_size(16 * 1024)
        .max_write_buffer_size(REQUEST_LIMIT + 64 * 1024)
        .max_message_size(Some(limits.response_bytes))
        .max_frame_size(Some(limits.response_bytes));
    let mut handshake = tungstenite::client::client_with_config(target, stream, Some(config));
    let (mut socket, _) = loop {
        check(cancel, deadline)?;
        match handshake {
            Ok(result) => break result,
            Err(HandshakeError::Interrupted(state)) => {
                pause(cancel, deadline)?;
                handshake = state.handshake();
            }
            Err(HandshakeError::Failure(error)) => {
                return Err(format!("Edge-CDP-handshake: {error}"))
            }
        }
    };
    check(cancel, deadline)?;
    // On WouldBlock tungstenite retains the queued frame. Flush it; resending
    // the message could execute a browser operation twice.
    if let Err(error) = socket.send(Message::Text(request.into())) {
        if !pending(&error) {
            return Err(format!("Edge-CDP-opdracht: {error}"));
        }
        loop {
            pause(cancel, deadline)?;
            match socket.flush() {
                Ok(()) => break,
                Err(e) if pending(&e) => {}
                Err(e) => return Err(format!("Edge-CDP-schrijven: {e}")),
            }
        }
    }
    loop {
        check(cancel, deadline)?;
        let message = match socket.read() {
            Ok(message) => message,
            Err(e) if pending(&e) => {
                pause(cancel, deadline)?;
                continue;
            }
            Err(e) => return Err(format!("Edge-CDP-antwoord: {e}")),
        };
        check(cancel, deadline)?;
        if let Message::Text(body) = message {
            let reply: Value =
                serde_json::from_str(&body).map_err(|_| "Edge gaf ongeldige CDP-gegevens terug")?;
            check(cancel, deadline)?;
            if reply["id"] == 1 {
                if reply.get("error").is_some() {
                    return Err(format!("Edge ondersteunt CDP {method} niet"));
                }
                return reply
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "Geen CDP-resultaat".into());
            }
        }
    }
}
