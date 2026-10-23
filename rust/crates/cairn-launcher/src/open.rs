//! Opens an address or a folder with the system's own program (the browser, Steam, the file explorer).

use std::process::{Command, Stdio};

/// Opens a web address or a `steam://` link.
pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("steam://")) {
        return;
    }
    spawn(url);
}

/// Shows a folder in the file explorer.
pub fn open_folder(path: &std::path::Path) {
    if path.is_dir() {
        spawn(&path.to_string_lossy());
    }
}

#[cfg(windows)]
fn spawn(target: &str) {
    use std::os::windows::process::CommandExt;
    // `start` takes the first quoted argument as a window title
    let _ = Command::new("cmd")
        .args(["/C", "start", "", target])
        .creation_flags(0x0800_0000)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(not(windows))]
fn spawn(target: &str) {
    let _ = Command::new("xdg-open")
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_web_and_steam_addresses_are_opened() {
        // nothing is started for anything else (a path, a program, a script)
        super::open_url("calc.exe");
        super::open_url("file:///etc/passwd");
    }
}
