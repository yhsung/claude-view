//! Install-source detection + install-beacon ping.
//!
//! Extracted from `main.rs` in CQRS Phase 7.c. The beacon is
//! fire-and-forget (3 s timeout, never blocks startup); the detection
//! is a best-effort classification (one `stat`, once at startup) of how
//! the binary was launched.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Filename of the install-source marker written by `install.sh` /
/// `install.ps1` next to the binary tree (see `install.sh`:
/// `${INSTALL_DIR}/install-source`). The file *content* names the
/// installer (`install_sh` or `install_ps1`); presence alone means a
/// script install as opposed to npx/plugin.
const INSTALL_SOURCE_MARKER: &str = "install-source";

/// Detect how the server was installed/launched.
///
/// Returns one of: "plugin", "install_sh", "install_ps1", "npx".
///
/// Precedence is by *current launch*, then *install origin*:
/// 1. `plugin`      — Claude Code sets `CLAUDE_PLUGIN_ROOT` when it spawns us.
/// 2. `install_sh`  — `install.sh` left its marker beside the binary.
/// 3. `install_ps1` — `install.ps1` left its marker beside the binary.
/// 4. `npx`         — neither signal: ran from npm's transient cache.
pub fn detect_install_source() -> &'static str {
    if std::env::var("CLAUDE_PLUGIN_ROOT").is_ok() {
        return "plugin";
    }
    match read_install_marker() {
        Some(content) if content.trim() == "install_ps1" => "install_ps1",
        Some(_) => "install_sh",
        None => "npx",
    }
}

/// Read the install-source marker beside the running binary, if present.
///
/// The binary lives at `${INSTALL_DIR}/bin/claude-view`; the marker at
/// `${INSTALL_DIR}/install-source`. Resolving the marker *relative to the
/// executable* makes the install.sh↔server contract explicit and
/// drift-proof — it holds regardless of `CLAUDE_VIEW_INSTALL_DIR`, unlike
/// the previous brittle substring match on the install path.
fn read_install_marker() -> Option<String> {
    let marker = std::env::current_exe()
        .ok()
        .and_then(|exe| marker_path_for_exe(&exe))?;
    std::fs::read_to_string(marker).ok()
}

/// Pure path arithmetic: `${INSTALL_DIR}/bin/claude-view` →
/// `${INSTALL_DIR}/install-source`. Split out (no I/O, exe injected) so
/// the location contract is unit-testable without a real install — the
/// exact logic the `~/.cache → ~/.claude-view` migration broke unguarded.
fn marker_path_for_exe(exe: &Path) -> Option<PathBuf> {
    exe.parent() // …/bin
        .and_then(Path::parent) // …/ (INSTALL_DIR)
        .map(|root| root.join(INSTALL_SOURCE_MARKER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_resolves_beside_install_dir_not_cache() {
        // install.sh: BIN_DIR=${INSTALL_DIR}/bin, binary at bin/claude-view.
        let exe = Path::new("/home/u/.claude-view/bin/claude-view");
        assert_eq!(
            marker_path_for_exe(exe),
            Some(PathBuf::from("/home/u/.claude-view/install-source")),
            "marker must resolve to the INSTALL_DIR root, beside `version`"
        );
    }

    #[test]
    fn marker_path_honors_custom_install_dir() {
        // CLAUDE_VIEW_INSTALL_DIR override: contract still holds because
        // the marker is resolved relative to the binary, not a fixed path.
        let exe = Path::new("/opt/cv/bin/claude-view");
        assert_eq!(
            marker_path_for_exe(exe),
            Some(PathBuf::from("/opt/cv/install-source"))
        );
    }

    #[test]
    fn detect_reads_real_marker_via_injected_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        let exe = bin.join("claude-view");

        // No marker yet → not install_sh.
        assert!(!marker_path_for_exe(&exe).unwrap().is_file());

        // install.sh writes the marker → detected.
        std::fs::write(dir.path().join(INSTALL_SOURCE_MARKER), "install_sh").expect("write marker");
        assert!(marker_path_for_exe(&exe).unwrap().is_file());
    }
}

/// Fire-and-forget ping to CF Worker for unified install tracking.
pub fn ping_install_beacon(source: &str) {
    let url = format!(
        "https://get.claudeview.ai/ping?source={}&v={}",
        source,
        env!("CARGO_PKG_VERSION"),
    );
    tokio::spawn(async move {
        let _ = reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await;
    });
}
