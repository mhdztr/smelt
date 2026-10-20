//! Optional external engines: PATH scan plus a bounded `--version` probe.
//! `smelt doctor` is the only caller (PERFORMANCE.md guardrail 1: never on
//! the convert path).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Engines reported by `smelt doctor` in M0 (docs/cli.md).
pub const ENGINES: &[&str] = &["soffice", "tesseract", "pandoc"];

/// `--version` probe timeout (docs/cli.md: 5 s); a slow or hung engine
/// degrades to path-only.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// One doctor row: what was looked for, where it was found, what it said.
#[derive(Debug)]
pub struct Engine {
    pub name: &'static str,
    /// `None` when the binary is not on PATH.
    pub path: Option<PathBuf>,
    /// `None` when the probe failed or timed out (row shows path only).
    pub version: Option<String>,
}

/// Look up `name` on PATH and probe it. A missing binary or failed probe is
/// a normal outcome here, not an error.
pub fn probe(name: &'static str) -> Engine {
    let path = which(name);
    let version = path.as_deref().and_then(version_of);
    Engine {
        name,
        path,
        version,
    }
}

/// First executable named `name` on PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

/// Run `path --version`, wait at most [`PROBE_TIMEOUT`], and return the
/// first non-empty output line (stdout first, stderr as fallback).
pub fn version_of(path: &Path) -> Option<String> {
    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }

    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        use std::io::Read;
        let _ = pipe.read_to_end(&mut stdout);
    }
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        use std::io::Read;
        let _ = pipe.read_to_end(&mut stderr);
    }

    first_line(&stdout).or_else(|| first_line(&stderr))
}

/// First non-empty trimmed line of `bytes` as lossy UTF-8.
fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_misses_on_absent_binary() {
        assert!(which("smelt-definitely-not-on-path").is_none());
    }

    #[test]
    fn version_of_fails_cleanly_on_absent_path() {
        assert!(version_of(Path::new("/nonexistent/smelt-probe")).is_none());
    }

    #[test]
    fn probe_of_absent_engine_is_pathless() {
        let engine = probe("smelt-definitely-not-on-path");
        assert!(engine.path.is_none());
        assert!(engine.version.is_none());
    }

    #[test]
    fn first_line_skips_blanks_and_trims() {
        assert_eq!(
            first_line(b"\n\n  tesseract 5.5.0  \nleptonica\n").as_deref(),
            Some("tesseract 5.5.0")
        );
        assert_eq!(first_line(b"\n \n").as_deref(), None);
        assert_eq!(first_line(b"").as_deref(), None);
    }
}
