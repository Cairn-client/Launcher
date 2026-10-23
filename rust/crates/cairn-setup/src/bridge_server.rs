// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The launcher's end of the bridge a running game's mod opens to it (docs/BRIDGE.md): a local socket (a named pipe on Windows) with a name
//! nobody can guess, given to the game in `CAIRN_BRIDGE_PIPE`. One bridge at a time; the first thing it says is `hello`, then the launcher
//! welcomes it. Every line from the bridge is checked by [`bridge_protocol::parse`] before the listener hears of it.

use crate::bridge_protocol::{self, BridgeIn};
use interprocess::local_socket::{tokio::prelude::*, GenericNamespaced, ListenerOptions};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

type OnReceived = Box<dyn Fn(BridgeIn) + Send + Sync>;
type OnConnection = Box<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
struct Shared {
    /// The game that said hello; empty while nobody is connected.
    game: String,
    /// Lines for the connected bridge.
    out: Option<mpsc::UnboundedSender<String>>,
}

pub struct BridgeServer {
    pipe_name: String,
    shared: Arc<Mutex<Shared>>,
    stop: watch::Sender<bool>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl BridgeServer {
    /// Starts listening. `on_received` hears every message of the connected bridge (after `hello`); `on_connection` hears the game's id when a
    /// bridge says hello, and "" when it goes away.
    pub fn start(
        on_received: impl Fn(BridgeIn) + Send + Sync + 'static,
        on_connection: impl Fn(&str) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let pipe_name = format!("cairn-bridge-{}", random_hex());
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (stop, stop_rx) = watch::channel(false);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::io::Result<()>>();
        let thread = {
            let (name, shared) = (pipe_name.clone(), shared.clone());
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                rt.block_on(run(name, shared, Box::new(on_received), Box::new(on_connection), stop_rx, ready_tx));
            })
        };
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                pipe_name,
                shared,
                stop,
                thread: Mutex::new(Some(thread)),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(std::io::Error::other("the bridge could not start")),
        }
    }

    /// The name to give the game in `CAIRN_BRIDGE_PIPE`.
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    /// The game of the connected bridge; empty when there is none.
    pub fn game(&self) -> String {
        self.shared.lock().unwrap().game.clone()
    }

    /// A bridge has said hello and is still there.
    pub fn connected(&self) -> bool {
        let s = self.shared.lock().unwrap();
        s.out.is_some() && !s.game.is_empty()
    }

    /// Sends one line to the connected bridge; `false` when there is none (or it just went).
    pub fn send(&self, line: &str) -> bool {
        let s = self.shared.lock().unwrap();
        if s.game.is_empty() {
            return false;
        }
        s.out.as_ref().is_some_and(|tx| tx.send(line.to_string()).is_ok())
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

fn random_hex() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn run(
    name: String,
    shared: Arc<Mutex<Shared>>,
    on_received: OnReceived,
    on_connection: OnConnection,
    mut stop: watch::Receiver<bool>,
    ready: std::sync::mpsc::Sender<std::io::Result<()>>,
) {
    let listener = match listener_for(&name) {
        Ok(l) => {
            let _ = ready.send(Ok(()));
            l
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    loop {
        let stream = tokio::select! {
            _ = stop.changed() => return,
            s = listener.accept() => match s {
                Ok(s) => s,
                Err(_) => { tokio::time::sleep(Duration::from_millis(100)).await; continue; } // a bridge that went away: wait for the next one
            },
        };
        let (tx, rx) = mpsc::unbounded_channel();
        {
            let mut s = shared.lock().unwrap();
            s.out = Some(tx);
            s.game.clear();
        }
        tokio::select! {
            _ = stop.changed() => { reset(&shared, &on_connection); return; }
            _ = serve(stream, rx, &shared, &on_received, &on_connection) => {}
        }
        reset(&shared, &on_connection);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn reset(shared: &Mutex<Shared>, on_connection: &OnConnection) {
    let had = {
        let mut s = shared.lock().unwrap();
        s.out = None;
        !std::mem::take(&mut s.game).is_empty()
    };
    if had {
        on_connection("");
    }
}

#[cfg(windows)]
fn listener_for(name: &str) -> std::io::Result<interprocess::local_socket::tokio::Listener> {
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    // only the user who runs the launcher (the owner of the pipe) and the system may open it
    let sddl = widestring::U16CString::from_str("D:P(A;;GA;;;OW)(A;;GA;;;SY)").map_err(std::io::Error::other)?;
    let sd = SecurityDescriptor::deserialize(&sddl)?;
    ListenerOptions::new()
        .name(name.to_ns_name::<GenericNamespaced>()?)
        .security_descriptor(sd)
        .create_tokio()
}

#[cfg(not(windows))]
fn listener_for(name: &str) -> std::io::Result<interprocess::local_socket::tokio::Listener> {
    ListenerOptions::new().name(name.to_ns_name::<GenericNamespaced>()?).create_tokio()
}

async fn serve(
    stream: interprocess::local_socket::tokio::Stream,
    mut out: mpsc::UnboundedReceiver<String>,
    shared: &Mutex<Shared>,
    on_received: &OnReceived,
    on_connection: &OnConnection,
) {
    let (mut rd, mut wr) = stream.split();
    let mut buf = [0u8; 4096];
    let mut line: Vec<u8> = Vec::with_capacity(256);
    let mut greeted = false;
    loop {
        let n = tokio::select! {
            n = rd.read(&mut buf) => match n { Ok(0) | Err(_) => return, Ok(n) => n },
            Some(l) = out.recv() => {
                if wr.write_all(format!("{l}\n").as_bytes()).await.is_err() || wr.flush().await.is_err() { return; }
                continue;
            }
        };
        for &b in &buf[..n] {
            if b != b'\n' {
                line.push(b);
                if line.len() > bridge_protocol::MAX_LINE {
                    return; // a line that never ends: the connection ends
                }
                continue;
            }
            let text = String::from_utf8_lossy(&line).trim_end_matches('\r').to_string();
            line.clear();
            if text.is_empty() {
                continue;
            }
            let msg = bridge_protocol::parse(&text);
            if !greeted {
                // the first thing a bridge says is hello
                let Some(BridgeIn::Hello { protocol, game }) = msg else { return };
                if protocol > bridge_protocol::VERSION {
                    let _ = wr
                        .write_all(format!("{}\n", bridge_protocol::bye("the mod needs a newer launcher")).as_bytes())
                        .await;
                    let _ = wr.flush().await;
                    return;
                }
                greeted = true;
                if wr.write_all(format!("{}\n", bridge_protocol::welcome()).as_bytes()).await.is_err() || wr.flush().await.is_err() {
                    return;
                }
                shared.lock().unwrap().game = game.clone();
                on_connection(&game);
                continue;
            }
            match msg {
                None | Some(BridgeIn::Hello { .. }) => {}
                Some(m) => on_received(m),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Instant;

    struct Fixture {
        server: BridgeServer,
        got: Receiver<BridgeIn>,
        changes: Receiver<String>,
    }

    fn fixture() -> Fixture {
        let (gtx, got) = channel();
        let (ctx, changes) = channel();
        let (gtx, ctx) = (Mutex::new(gtx), Mutex::new(ctx));
        let server = BridgeServer::start(
            move |m| drop(gtx.lock().unwrap().send(m)),
            move |g| drop(ctx.lock().unwrap().send(g.to_string())),
        )
        .unwrap();
        Fixture { server, got, changes }
    }

    /// A bridge, as the game's mod is one: it connects, says lines and reads lines. It lives on a small runtime of its own, as the server does.
    struct Client {
        rt: tokio::runtime::Runtime,
        stream: interprocess::local_socket::tokio::Stream,
        buf: Vec<u8>,
    }

    impl Client {
        fn connect(name: &str) -> Self {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let stream = rt.block_on(async {
                let start = Instant::now();
                loop {
                    match interprocess::local_socket::tokio::Stream::connect(name.to_ns_name::<GenericNamespaced>().unwrap()).await {
                        Ok(s) => return s,
                        Err(_) if start.elapsed() < Duration::from_secs(5) => tokio::time::sleep(Duration::from_millis(25)).await,
                        Err(e) => panic!("could not connect: {e}"),
                    }
                }
            });
            Self {
                rt,
                stream,
                buf: Vec::new(),
            }
        }

        fn write_all(&mut self, bytes: &[u8]) {
            let stream = &mut self.stream;
            let _ = self
                .rt
                .block_on(async { tokio::time::timeout(Duration::from_secs(5), stream.write_all(bytes)).await });
        }

        fn say(&mut self, line: &str) {
            self.write_all(format!("{line}\n").as_bytes());
        }

        /// The next line, or `None` when the server closed the connection.
        fn read(&mut self) -> Option<String> {
            let (stream, buf) = (&mut self.stream, &mut self.buf);
            self.rt.block_on(async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                            let line: Vec<u8> = buf.drain(..=pos).collect();
                            return Some(String::from_utf8_lossy(&line).trim_end().to_string());
                        }
                        let mut tmp = [0u8; 4096];
                        match stream.read(&mut tmp).await {
                            Ok(0) | Err(_) => return None,
                            Ok(n) => buf.extend_from_slice(&tmp[..n]),
                        }
                    }
                })
                .await
                .expect("a line in time")
            })
        }
    }

    fn until(mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn the_pipe_name_cannot_be_guessed() {
        let a = fixture();
        let b = fixture();
        assert_ne!(a.server.pipe_name(), b.server.pipe_name());
        let n = a.server.pipe_name();
        assert!(n.starts_with("cairn-bridge-") && n.len() == "cairn-bridge-".len() + 32 && n[13..].bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_bridge_says_hello_is_welcomed_and_its_messages_arrive_checked() {
        let f = fixture();
        let mut c = Client::connect(f.server.pipe_name());
        c.say(r#"{"t":"hello","protocol":1,"game":"noita"}"#);
        assert_eq!(c.read().unwrap(), bridge_protocol::welcome());
        until(|| f.server.connected());
        assert_eq!(f.server.game(), "noita");
        assert_eq!(f.changes.recv_timeout(Duration::from_secs(5)).unwrap(), "noita");

        c.say(r#"{"t":"notice","text":"Ann joined"}"#);
        c.say("garbage that is not json");
        c.say(r#"{"t":"somethingnew"}"#);
        c.say(r#"{"t":"notice_hide"}"#);
        assert_eq!(
            f.got.recv_timeout(Duration::from_secs(5)).unwrap(),
            BridgeIn::Notice {
                show: true,
                text: "Ann joined".into()
            }
        );
        assert_eq!(
            f.got.recv_timeout(Duration::from_secs(5)).unwrap(),
            BridgeIn::Notice {
                show: false,
                text: String::new()
            }
        ); // the damaged lines were skipped
    }

    #[test]
    fn the_launcher_can_answer_the_bridge() {
        let f = fixture();
        let mut c = Client::connect(f.server.pipe_name());
        c.say(r#"{"t":"hello","protocol":1,"game":"noita"}"#);
        c.read();
        until(|| f.server.connected());
        assert!(f.server.send(&bridge_protocol::attach()));
        assert_eq!(c.read().unwrap(), r#"{"t":"attach"}"#);
        assert!(f.server.send(&bridge_protocol::do_item("friendly_fire", "enter", 1).unwrap()));
        assert_eq!(c.read().unwrap(), r#"{"t":"do","id":"friendly_fire","verb":"enter"}"#);
    }

    #[test]
    fn with_no_bridge_nothing_is_sent() {
        let f = fixture();
        assert!(!f.server.send(&bridge_protocol::attach()));
        assert!(!f.server.connected());
    }

    #[test]
    fn a_bridge_that_needs_a_newer_launcher_is_told_so_and_disconnected() {
        let f = fixture();
        let mut c = Client::connect(f.server.pipe_name());
        c.say(&format!(
            r#"{{"t":"hello","protocol":{},"game":"noita"}}"#,
            bridge_protocol::VERSION + 1
        ));
        assert!(c.read().unwrap().contains("newer launcher"));
        assert_eq!(c.read(), None); // the connection ended
        assert!(!f.server.connected());
    }

    #[test]
    fn a_bridge_that_does_not_say_hello_first_is_dropped() {
        let f = fixture();
        let mut c = Client::connect(f.server.pipe_name());
        c.say(r#"{"t":"notice","text":"sneaking in"}"#);
        assert_eq!(c.read(), None);
        assert!(f.got.try_recv().is_err());
        assert!(!f.server.connected());
    }

    #[test]
    fn a_line_that_never_ends_drops_the_connection() {
        let f = fixture();
        let mut c = Client::connect(f.server.pipe_name());
        c.say(r#"{"t":"hello","protocol":1,"game":"noita"}"#);
        c.read();
        until(|| f.server.connected());
        c.write_all("x".repeat(bridge_protocol::MAX_LINE + 100).as_bytes());
        until(|| !f.server.connected());
    }

    #[test]
    fn a_restarted_bridge_takes_over() {
        let f = fixture();
        let mut first = Client::connect(f.server.pipe_name());
        first.say(r#"{"t":"hello","protocol":1,"game":"noita"}"#);
        first.read();
        until(|| f.server.connected());
        drop(first);
        until(|| !f.server.connected());
        assert_eq!(f.changes.recv_timeout(Duration::from_secs(5)).unwrap(), "noita");
        assert_eq!(f.changes.recv_timeout(Duration::from_secs(5)).unwrap(), ""); // it went away
        let mut second = Client::connect(f.server.pipe_name());
        second.say(r#"{"t":"hello","protocol":1,"game":"noita"}"#);
        assert_eq!(second.read().unwrap(), bridge_protocol::welcome());
        until(|| f.server.connected());
    }

    #[test]
    fn dropping_the_server_ends_it() {
        let f = fixture();
        let mut c = Client::connect(f.server.pipe_name());
        c.say(r#"{"t":"hello","protocol":1,"game":"noita"}"#);
        c.read();
        until(|| f.server.connected());
        drop(f.server);
        assert_eq!(c.read(), None);
    }
}
