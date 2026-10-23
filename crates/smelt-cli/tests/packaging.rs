//! Packaging regressions: `smelt-git`'s `pkgver()` must work in a repository
//! with no tags, and `scripts/aur-update.sh` must not truncate the tracked
//! `.SRCINFO` when `makepkg --printsrcinfo` fails.
//!
//! These tests drive the real shell artifacts with fake `makepkg`/git state;
//! they never touch the checkout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "smelt-packaging-{}-{}-{tag}",
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        // A maintainer/CI global `commit.gpgsign=true` must not make this
        // fixture commit (or the test) fail.
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Run the PKGBUILD's `pkgver()` against the git repo at `repo/smelt-git`.
#[cfg(unix)]
fn run_pkgver(pkgbuild: &Path, repo: &Path) -> String {
    let out = Command::new("bash")
        .arg("-c")
        .arg("set -e; source \"$1\"; pkgver")
        .arg("_")
        .arg(pkgbuild)
        .current_dir(repo)
        .output()
        .expect("bash runs");
    assert!(
        out.status.success(),
        "pkgver failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[cfg(unix)]
#[test]
fn smelt_git_pkgver_handles_no_tags_and_strips_the_v() {
    let repo = repo_root();
    let dir = TempDir::new("pkgver");
    let work = dir.path().join("smelt-git");
    fs::create_dir_all(&work).expect("work tree created");
    git(&work, &["init", "-q"]);
    git(&work, &["config", "user.email", "test@example.invalid"]);
    git(&work, &["config", "user.name", "smelt test"]);
    fs::write(work.join("README.md"), "fixture\n").expect("fixture written");
    git(&work, &["add", "README.md"]);
    git(&work, &["commit", "-q", "-m", "init"]);

    let pkgbuild = dir.path().join("PKGBUILD");
    fs::copy(repo.join("packaging/aur/smelt-git/PKGBUILD"), &pkgbuild).expect("PKGBUILD copied");

    let commit = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--short=8", "HEAD"])
            .current_dir(&work)
            .output()
            .expect("git rev-parse runs")
            .stdout,
    )
    .trim()
    .to_owned();

    // No tags: `git describe` fails, the fallback must still produce a valid
    // pkgver instead of aborting the build.
    let untagged = run_pkgver(&pkgbuild, dir.path());
    assert_eq!(untagged, format!("0.1.0.r0.g{commit}"));

    // A tag named with the release `v` prefix is stripped from pkgver.
    git(&work, &["tag", "v0.2.0"]);
    let tagged = run_pkgver(&pkgbuild, dir.path());
    assert_eq!(tagged, format!("0.2.0.r0.g{commit}"));
    assert!(!tagged.contains('v'), "leading v leaked into pkgver");
}

#[cfg(unix)]
#[test]
fn pkgbuilds_declare_git_and_use_cargos_default_target_dir() {
    let repo = repo_root();

    // `smelt`'s `check()` runs this test suite, which shells out to `git`;
    // git is not part of Arch `base-devel`, so it must be a makedepend or a
    // clean-chroot `makepkg` fails in check().
    let smelt =
        fs::read_to_string(repo.join("packaging/aur/smelt/PKGBUILD")).expect("smelt PKGBUILD");
    assert!(
        smelt.contains("makedepends=('cargo' 'git')"),
        "smelt must declare git in makedepends"
    );
    // package() installs from cargo's default target directory inside the
    // manifest directory; a CARGO_TARGET_DIR override in build() pointed
    // somewhere else and package() could not find the binary.
    assert!(
        !smelt.contains("CARGO_TARGET_DIR"),
        "smelt must not override CARGO_TARGET_DIR"
    );
    assert!(
        smelt.contains("\"$pkgname-$pkgver/target/release/smelt\""),
        "smelt package() must read the default target dir"
    );

    let smelt_git = fs::read_to_string(repo.join("packaging/aur/smelt-git/PKGBUILD"))
        .expect("smelt-git PKGBUILD");
    assert!(
        smelt_git.contains("makedepends=('git' 'cargo')"),
        "smelt-git keeps git in makedepends"
    );
    assert!(
        !smelt_git.contains("CARGO_TARGET_DIR"),
        "smelt-git must not override CARGO_TARGET_DIR"
    );
    assert!(
        smelt_git.contains("\"$pkgname/target/release/smelt\""),
        "smelt-git package() must read the default target dir"
    );
}

#[cfg(unix)]
#[test]
fn aur_update_keeps_srcinfo_when_makepkg_fails() {
    use std::os::unix::fs::PermissionsExt;

    let repo = repo_root();
    let dir = TempDir::new("srcinfo");
    let package = dir.path().join("packaging/aur/smelt-git");
    fs::create_dir_all(&package).expect("package dir created");
    fs::create_dir_all(dir.path().join("scripts")).expect("scripts dir created");
    fs::copy(
        repo.join("scripts/aur-update.sh"),
        dir.path().join("scripts/aur-update.sh"),
    )
    .expect("script copied");
    for name in ["PKGBUILD", ".SRCINFO"] {
        fs::copy(
            repo.join("packaging/aur/smelt-git").join(name),
            package.join(name),
        )
        .expect("package file copied");
    }
    let srcinfo = package.join(".SRCINFO");
    let before = fs::read(&srcinfo).expect(".SRCINFO readable");
    assert!(!before.is_empty(), "fixture .SRCINFO is not empty");

    let bin = dir.path().join("fakebin");
    fs::create_dir_all(&bin).expect("fake bin created");
    let makepkg = bin.join("makepkg");
    fs::write(&makepkg, "#!/bin/sh\nexit 1\n").expect("fake makepkg written");
    let mut perms = fs::metadata(&makepkg).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&makepkg, perms).expect("fake makepkg executable");

    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let failed = Command::new("bash")
        .arg(dir.path().join("scripts/aur-update.sh"))
        .arg("smelt-git")
        .env("PATH", &path_env)
        .current_dir(dir.path())
        .output()
        .expect("script runs");
    assert!(
        !failed.status.success(),
        "script must fail when makepkg fails"
    );
    assert_eq!(
        fs::read(&srcinfo).expect(".SRCINFO still readable"),
        before,
        ".SRCINFO must be byte-identical after a failed makepkg"
    );
    let leftovers: Vec<_> = fs::read_dir(&package)
        .expect("package dir readable")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".SRCINFO."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );

    // Success path still replaces the file, through the same temp+mv.
    fs::write(&makepkg, "#!/bin/sh\nprintf 'pkgbase = fake\\n'\n").expect("fake makepkg rewritten");
    let ok = Command::new("bash")
        .arg(dir.path().join("scripts/aur-update.sh"))
        .arg("smelt-git")
        .env("PATH", &path_env)
        .current_dir(dir.path())
        .output()
        .expect("script runs");
    assert!(
        ok.status.success(),
        "script failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(
        fs::read_to_string(&srcinfo).expect(".SRCINFO readable"),
        "pkgbase = fake\n"
    );
}

#[cfg(unix)]
#[test]
fn aur_update_exits_non_zero_when_the_download_fails() {
    use std::os::unix::fs::PermissionsExt;

    let repo = repo_root();
    let dir = TempDir::new("curl-fail");
    let package = dir.path().join("packaging/aur/smelt-bin");
    fs::create_dir_all(&package).expect("package dir created");
    fs::create_dir_all(dir.path().join("scripts")).expect("scripts dir created");
    fs::copy(
        repo.join("scripts/aur-update.sh"),
        dir.path().join("scripts/aur-update.sh"),
    )
    .expect("script copied");
    for name in ["PKGBUILD", ".SRCINFO"] {
        fs::copy(
            repo.join("packaging/aur/smelt-bin").join(name),
            package.join(name),
        )
        .expect("package file copied");
    }
    let srcinfo = package.join(".SRCINFO");
    let srcinfo_before = fs::read(&srcinfo).expect(".SRCINFO readable");
    let pkgbuild = package.join("PKGBUILD");
    let pkgbuild_before = fs::read_to_string(&pkgbuild).expect("PKGBUILD readable");
    assert!(
        pkgbuild_before.contains("sha256sums=('0000"),
        "fixture keeps the placeholder sums"
    );

    let bin = dir.path().join("fakebin");
    fs::create_dir_all(&bin).expect("fake bin created");
    // A failing download must be fatal even though makepkg would succeed:
    // continuing would keep the old checksum while bumping pkgver.
    let curl = bin.join("curl");
    fs::write(&curl, "#!/bin/sh\nexit 7\n").expect("fake curl written");
    let makepkg = bin.join("makepkg");
    fs::write(&makepkg, "#!/bin/sh\nprintf 'pkgbase = fake\\n'\n").expect("fake makepkg written");
    for tool in [&curl, &makepkg] {
        let mut perms = fs::metadata(tool).expect("stat").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(tool, perms).expect("fake tool executable");
    }

    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .arg(dir.path().join("scripts/aur-update.sh"))
        .arg("smelt-bin")
        .arg("9.9.9")
        .env("PATH", &path_env)
        .current_dir(dir.path())
        .output()
        .expect("script runs");

    assert!(
        !out.status.success(),
        "script must exit non-zero after a failed download"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("could not download"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read(&srcinfo).expect(".SRCINFO still readable"),
        srcinfo_before,
        ".SRCINFO must not be regenerated after a failed download"
    );
    let pkgbuild_after = fs::read_to_string(&pkgbuild).expect("PKGBUILD readable");
    assert!(
        pkgbuild_after.contains(
            "sha256sums=('0000000000000000000000000000000000000000000000000000000000000000')"
        ),
        "stale placeholder checksum must be left untouched"
    );
}
