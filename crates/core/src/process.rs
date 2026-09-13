//! Cross-platform process helpers.
//!
//! Single place for PID liveness + termination so Windows support does not
//! require `libc::kill` (which has no Windows equivalent).
//!
//! - Unix: `kill(pid, 0)` liveness check, `SIGTERM` / `SIGKILL` terminate.
//! - Windows: `sysinfo` liveness check, `taskkill` / `TerminateProcess`-equivalent
//!   via `sysinfo` kill (no POSIX signals on Windows).

/// Check if a process with the given PID is still alive.
///
/// Returns `false` for PIDs <= 1 to guard against kernel/init reparenting.
pub fn is_pid_alive(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    #[cfg(unix)]
    {
        // SAFETY: kill with signal 0 does not deliver a signal, only
        // checks existence. Returns 0 if the process exists.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), false);
        sys.process(sysinfo::Pid::from_u32(pid)).is_some()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// Terminate a process.
///
/// - Unix: `SIGTERM` by default, `SIGKILL` when `force` is true.
/// - Windows: graceful and force both terminate the process (Windows has no
///   POSIX signals; `taskkill` without `/F` is attempted first for graceful).
pub fn terminate_pid(pid: u32, force: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
        // SAFETY: pid validated by callers (never our own PID).
        let r = unsafe { libc::kill(pid as i32, sig) };
        if r == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(windows)]
    {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), true);
        if let Some(p) = sys.process(sysinfo::Pid::from_u32(pid)) {
            if p.kill() {
                return Ok(());
            }
        }
        // Fallback to taskkill for processes sysinfo cannot signal.
        let mut cmd = std::process::Command::new("taskkill");
        cmd.arg("/PID").arg(pid.to_string());
        if force {
            cmd.arg("/F");
        }
        let out = cmd.output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, force);
        Err(std::io::Error::other("unsupported platform"))
    }
}

/// Best-effort graceful shutdown of a `std::process::Child`.
///
/// - Unix: `SIGTERM` first (lets Node.js `process.on('SIGTERM')` run), then
///   poll up to 3 s, then `Child::kill()` (SIGKILL).
/// - Windows: `Child::kill()` == `TerminateProcess`; there is no SIGTERM
///   equivalent for a generic child, so terminate directly.
pub fn shutdown_child_graceful(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        // SAFETY: pid comes from a Child we own.
        let term_result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if term_result != 0 {
            tracing::warn!(pid, errno = term_result, "SIGTERM send failed");
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::info!(pid, ?status, "Child exited gracefully");
                    return;
                }
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => {
                    tracing::warn!(pid, error = %e, "try_wait failed, force-killing");
                    break;
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Resolve the user's home directory on any platform.
///
/// Prefers `dirs::home_dir()`, falls back to `USERPROFILE` (Windows) /
/// `HOME` (Unix), then to the temp dir rather than panicking (Windows
/// services often run without a profile).
pub fn home_dir() -> Option<std::path::PathBuf> {
    if let Some(h) = dirs::home_dir() {
        return Some(h);
    }
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                return Some(std::path::PathBuf::from(v));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_alive() {
        assert!(is_pid_alive(std::process::id()));
    }

    #[test]
    fn rejects_zero_and_one() {
        assert!(!is_pid_alive(0));
        assert!(!is_pid_alive(1));
    }

    #[test]
    fn nonexistent_pid_is_dead() {
        assert!(!is_pid_alive(4_000_000));
    }
}
