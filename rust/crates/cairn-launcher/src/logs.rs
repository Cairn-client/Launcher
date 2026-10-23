//! The logs of this PC in one zip on the Desktop (to send with a bug report), and the hand-over to the uninstaller.

use cairn_setup::redact;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The files that make up a log bundle: the launcher's logs (`%APPDATA%\Cairn\app*.log`) and the logs the packs' programs wrote.
fn log_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in [crate::app_dirs::app_folder(), crate::app_dirs::packs_root()] {
        collect(&dir, 4, &mut files);
    }
    files
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            collect(&p, depth - 1, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("log")) && fs::metadata(&p).is_ok_and(|m| m.len() < 20 * 1024 * 1024)
        {
            out.push(p);
        }
    }
}

/// Writes `Cairn-logs-<time>.zip` on the Desktop (or the user's folder): each log with the player's names, Steam ids and folders taken out.
pub fn save_bundle() -> Result<PathBuf, String> {
    let desktop = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .map(|h| h.join("Desktop"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let name = format!(
        "Cairn-logs-{}.zip",
        cairn_setup::time_util::format_utc(cairn_setup::time_util::now_utc()).replace([':', 'T', 'Z'], "-")
    );
    let path = desktop.join(name);
    let file = fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    let files = log_files();
    for (i, f) in files.iter().enumerate() {
        let text = String::from_utf8_lossy(&fs::read(f).map_err(|e| e.to_string())?).into_owned();
        let entry = format!(
            "{i:02}-{}",
            f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        );
        zip.start_file(entry, opts).map_err(|e| e.to_string())?;
        zip.write_all(redact::text(&text).as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.start_file("about.txt", opts).map_err(|e| e.to_string())?;
    zip.write_all(format!("Cairn {}\nlogs: {}\n", crate::app::VERSION, files.len()).as_bytes())
        .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// Starts the uninstaller that Setup put beside the launcher.
pub fn start_uninstaller() -> Result<(), String> {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
        .ok_or("the launcher's folder is not known")?;
    let un = ["unins000.exe", "unins001.exe"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
        .ok_or("this launcher was not installed with Setup, so there is nothing to uninstall")?;
    std::process::Command::new(un).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_are_found_below_a_folder_and_other_files_are_not() {
        let d = std::env::temp_dir().join(format!("cairn-logs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("a").join("b")).unwrap();
        fs::write(d.join("app.log"), "x").unwrap();
        fs::write(d.join("a").join("b").join("agent.LOG"), "x").unwrap();
        fs::write(d.join("a").join("notes.txt"), "x").unwrap();
        let mut found = Vec::new();
        collect(&d, 4, &mut found);
        assert_eq!(found.len(), 2);
        let mut shallow = Vec::new();
        collect(&d, 0, &mut shallow);
        assert_eq!(shallow.len(), 1);
        let _ = fs::remove_dir_all(&d);
    }
}
