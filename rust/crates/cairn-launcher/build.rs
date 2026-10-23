// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Reads the release number from the repository's `VERSION` file (the one source of truth) and puts the Cairn icon and the version into the
//! Windows executable (the tray icon and the taskbar read the icon from it).

fn main() {
    let version = std::fs::read_to_string("../../../VERSION")
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let strict = version.split('.').count() == 3
        && version
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p == "0" || !p.starts_with('0')));
    assert!(
        strict,
        "VERSION must be strict MAJOR.MINOR.PATCH (e.g. 0.1.4), got '{version}'. See docs/VERSIONING.md"
    );
    println!("cargo:rerun-if-changed=../../../VERSION");
    println!("cargo:rustc-env=CAIRN_VERSION={version}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "Cairn Client");
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);
        res.set("FileDescription", "Cairn Client");
        if let Err(e) = res.compile() {
            println!("cargo:warning=the icon could not be embedded: {e}");
        }
    }
}
