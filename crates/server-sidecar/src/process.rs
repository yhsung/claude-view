// crates/server/src/sidecar/process.rs
//! Process utilities: port cleanup and sidecar directory discovery.

use std::path::PathBuf;
#[cfg(not(windows))]
use std::process::{Command, Stdio};

use super::error::SidecarError;

/// Kill stale node/sidecar processes holding a TCP port.
///
/// Only kills processes whose command name contains "node" (sidecar runs
/// via `node dist/index.js`). Leaves other apps alone.
pub(crate) fn kill_port_holder(port: u16) {
    #[cfg(windows)]
    {
        // Windows: no lsof. Kill stale node.exe processes that look like
        // the sidecar; the port bind retry confirms success.
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, false);
        let my_pid = std::process::id();
        for (pid, proc_) in sys.processes() {
            if pid.as_u32() == my_pid {
                continue;
            }
            let name = proc_.name().to_string_lossy().to_lowercase();
            if !name.contains("node") {
                continue;
            }
            let cmd = proc_
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            if cmd.contains("sidecar") || cmd.contains(&port.to_string()) {
                tracing::info!(pid = pid.as_u32(), port, "Killing stale node sidecar (Windows)");
                let _ = claude_view_core::process::terminate_pid(pid.as_u32(), true);
            }
        }
        return;
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("lsof")
        .args(["-ti", &format!(":{port}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let pids = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return,
    };

    let my_pid = std::process::id().to_string();
    for pid in pids.split_whitespace() {
        if pid == my_pid {
            continue;
        }
        // Only kill node processes (sidecar runs as `node dist/index.js`)
        let is_node = Command::new("ps")
            .args(["-p", pid, "-o", "comm="])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_lowercase()
                    .contains("node")
            })
            .unwrap_or(false);
        if is_node {
            tracing::info!(pid, port, "Killing stale node process on sidecar port");
            let _ = Command::new("kill").args(["-9", pid]).status();
        } else {
            tracing::warn!(pid, port, "Non-node process on sidecar port, skipping");
        }
    }
    }
}

/// Locate the sidecar directory.
///
/// Priority:
/// 1. `SIDECAR_DIR` env var (set by npx-cli)
/// 2. `./sidecar/` relative to the binary (npx distribution)
/// 3. `./sidecar/` relative to CWD (dev mode: `cargo run` from repo root)
pub(crate) fn find_sidecar_dir() -> Result<PathBuf, SidecarError> {
    // 1. Explicit env var
    if let Ok(dir) = std::env::var("SIDECAR_DIR") {
        let p = PathBuf::from(&dir);
        if p.exists() {
            return Ok(p);
        }
        tracing::warn!(sidecar_dir = %dir, "SIDECAR_DIR set but directory does not exist");
    }

    // 2. Binary-relative (npx distribution)
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(canonical) = exe.canonicalize() {
            // On Windows, canonicalize() returns UNC `\\?\D:\...` paths.
            // Rust fs handles them, but Node.js mangles a UNC main-entry
            // argument (resolves to `D:` → EISDIR crash loop), so strip it.
            let canonical = strip_unc_prefix(canonical);
            if let Some(exe_dir) = canonical.parent() {
                let bin_sidecar = exe_dir.join("sidecar");
                if bin_sidecar.join("dist/index.js").exists() {
                    return Ok(bin_sidecar);
                }
            }
        }
    }

    // 3. CWD-relative (dev mode)
    let cwd_sidecar = PathBuf::from("sidecar");
    if cwd_sidecar.join("dist/index.js").exists() {
        return Ok(cwd_sidecar);
    }

    Err(SidecarError::SidecarDirNotFound)
}

/// Strip the Windows UNC `\\?\` prefix from a canonicalized path.
///
/// Rust's `canonicalize()` returns e.g. `\\?\D:\app\claude-view.exe`.
/// Rust fs APIs accept that, but child processes (Node.js entry-point
/// resolution) mangle it — observed: `lstat 'D:'` EISDIR crash loop.
/// No-op on non-Windows and on paths without the prefix.
fn strip_unc_prefix(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        const UNC: &str = r"\\?\";
        const UNC_UNC: &str = r"\\?\UNC\";
        if let Some(s) = p.to_str() {
            if let Some(rest) = s.strip_prefix(UNC_UNC) {
                return PathBuf::from(format!(r"\\{rest}"));
            }
            if let Some(rest) = s.strip_prefix(UNC) {
                return PathBuf::from(rest);
            }
        }
        p
    }
    #[cfg(not(windows))]
    {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_unc_prefix_leaves_plain_paths_alone() {
        let p = PathBuf::from("/usr/local/bin/claude-view");
        assert_eq!(strip_unc_prefix(p.clone()), p);
        #[cfg(windows)]
        {
            assert_eq!(
                strip_unc_prefix(PathBuf::from(r"\\?\D:\app\claude-view.exe")),
                PathBuf::from(r"D:\app\claude-view.exe")
            );
            assert_eq!(
                strip_unc_prefix(PathBuf::from(r"\\?\UNC\host\share\x.exe")),
                PathBuf::from(r"\\host\share\x.exe")
            );
        }
    }
}
