//! Regression test: the README quickstart must be executable verbatim on a
//! fresh clone. It used to convert `notes.md`, a file that is not in the
//! repository, so the very first command failed with exit 4 while the README
//! claimed every command works.
//!
//! The test copies the README and the built binary into a temp directory and
//! runs each quickstart command with bash. Nothing runs in the real checkout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "smelt-readme-{}-{}-{tag}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("temp dir created");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The shell lines inside the README's `## Quickstart` fenced block.
fn quickstart_commands(readme: &str) -> Vec<&str> {
    let section = readme
        .split_once("## Quickstart")
        .expect("README has a Quickstart section")
        .1;
    let block = section
        .split_once("```sh")
        .expect("Quickstart has a sh block")
        .1
        .split_once("```")
        .expect("Quickstart block is closed")
        .0;

    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[cfg(unix)]
#[test]
fn readme_quickstart_runs_verbatim_in_a_fresh_directory() {
    use std::os::unix::fs::symlink;

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let readme = fs::read_to_string(repo.join("README.md")).expect("README readable");
    let commands = quickstart_commands(&readme);
    assert!(
        commands.len() >= 5,
        "quickstart parser found too few commands: {commands:?}"
    );

    let dir = TempDir::new("quickstart");
    fs::copy(repo.join("README.md"), dir.path().join("README.md")).expect("README copied");
    let bin_dir = dir.path().join("target/release");
    fs::create_dir_all(&bin_dir).expect("binary dir created");
    symlink(env!("CARGO_BIN_EXE_smelt"), bin_dir.join("smelt"))
        .expect("built binary linked into the quickstart tree");

    for line in commands {
        let out = Command::new("bash")
            .arg("-e")
            .arg("-c")
            .arg(line)
            .current_dir(dir.path())
            .output()
            .expect("bash runs");
        assert!(
            out.status.success(),
            "quickstart command failed: {line}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    assert!(
        dir.path().join("README.html").is_file(),
        "first quickstart conversion wrote README.html"
    );
    assert!(
        dir.path().join("docs-out/one.html").is_file(),
        "batch quickstart wrote docs-out/one.html"
    );
}
