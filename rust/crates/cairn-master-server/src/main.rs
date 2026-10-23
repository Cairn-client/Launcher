// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
use cairn_master_server::config::{bind_address, Config};
use cairn_master_server::{log, server};
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let dir: PathBuf = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
        .unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = Config::load(&dir, std::env::var("ASPNETCORE_URLS").ok(), &args);
    let state = server::State::new(config.clone());
    let mut tasks = Vec::new();
    for url in &config.urls {
        let Some(addr) = bind_address(url) else {
            log::warn(&format!("Ignoring the address \"{url}\": it is not like http://0.0.0.0:5100."));
            continue;
        };
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                log::info(&format!("Listening on http://{addr}"));
                tasks.push(tokio::spawn(server::serve(listener, state.clone())));
            }
            Err(e) => log::warn(&format!("Could not listen on {addr}: {e}")),
        }
    }
    if tasks.is_empty() {
        eprintln!("Nothing to listen on.");
        std::process::exit(1);
    }
    for t in tasks {
        let _ = t.await;
    }
}
