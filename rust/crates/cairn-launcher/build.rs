// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Puts the Cairn icon and the version into the Windows executable (the tray icon and the taskbar read the icon from it).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "Cairn Client");
        res.set("FileDescription", "Cairn Client");
        if let Err(e) = res.compile() {
            println!("cargo:warning=the icon could not be embedded: {e}");
        }
    }
}
