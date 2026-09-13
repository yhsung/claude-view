//! Platform gate — macOS, Linux and Windows (x64, Win10+) are supported,
//! with `CLAUDE_VIEW_SKIP_PLATFORM_CHECK` as an escape hatch for the rest.
//!
//! Extracted from `main.rs` in CQRS Phase 7.f. Windows support added in
//! v0.45.1 (hooks via curl.exe, sysinfo-based process management);
//! tmux-backed CLI sessions and the sh/jq statusline wrapper remain
//! Unix-only (use WSL2 for tmux on Windows).

/// Returns `true` if the given platform is supported or the check is bypassed.
pub fn is_platform_supported(os: &str, skip_check: bool) -> bool {
    os == "macos" || os == "linux" || os == "windows" || skip_check
}

/// Check the current platform. If unsupported and the escape hatch is not set,
/// print a helpful message and exit with status 1.
pub fn ensure_supported() {
    let skip = std::env::var("CLAUDE_VIEW_SKIP_PLATFORM_CHECK").as_deref() == Ok("1");
    if !is_platform_supported(std::env::consts::OS, skip) {
        eprintln!(
            "\n\u{26a0}\u{fe0f}  claude-view supports macOS, Linux and Windows. \
             Your platform ({}) is not officially supported.",
            std::env::consts::OS
        );
        eprintln!("   Set CLAUDE_VIEW_SKIP_PLATFORM_CHECK=1 to try anyway.");
        eprintln!("   Report issues: https://github.com/tombelieber/claude-view/issues\n");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_macos_allowed() {
        assert!(is_platform_supported("macos", false));
    }

    #[test]
    fn platform_linux_allowed() {
        assert!(is_platform_supported("linux", false));
    }

    #[test]
    fn platform_windows_allowed() {
        assert!(is_platform_supported("windows", false));
    }

    #[test]
    fn platform_unknown_blocked() {
        assert!(!is_platform_supported("freebsd", false));
    }

    #[test]
    fn platform_skip_bypass() {
        assert!(is_platform_supported("freebsd", true));
    }
}
