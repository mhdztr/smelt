//! Integration tests for the `smelt` binary: documented exit codes, stdin
//! streaming, and output rules. Spec of record: `docs/cli.md`.

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique-per-test temp directory, removed on drop (M0 adds no fixture
/// crates; this stays dependency-free).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "smelt-test-{}-{}-{tag}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("temp dir created");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Write a fixture file into the directory and return its path.
    fn file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("fixture written");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_smelt"))
        .args(args)
        .output()
        .expect("smelt runs")
}

fn run_stdin<I, S>(args: I, input: &str) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(env!("CARGO_BIN_EXE_smelt"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("smelt spawns");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("stdin written");
    child.wait_with_output().expect("smelt finishes")
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("smelt exits with a code")
}

#[test]
fn single_file_converts_to_html() {
    let dir = TempDir::new("file-html");
    let input = dir.file("note.md", "# Hi\n\n**bold**\n");
    let out = run(["convert", input.to_str().unwrap(), "-t", "html"]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let html = fs::read_to_string(dir.path().join("note.html")).expect("output file written");
    assert!(html.contains("<h1>Hi</h1>"), "{html}");
    assert!(html.contains("<strong>bold</strong>"), "{html}");
    assert!(stdout_of(&out).is_empty(), "file conversion uses no stdout");

    let err = stderr_of(&out);
    assert!(err.contains("note.md -> note.html"), "success line: {err}");
    assert!(
        !err.contains('\u{1b}'),
        "piped output stays uncolored: {err:?}"
    );
}

#[test]
fn output_extension_infers_the_target_format() {
    // docs/cli.md resolution table: with no `-t`, a known extension on `-o`
    // infers the format. Regression test: the inference path was missing and
    // the README quickstart's first example failed with exit 2.
    let dir = TempDir::new("infer-from-o");
    let input = dir.file("note.md", "# Hi\n\n**bold**\n");
    let out_path = dir.path().join("note.html");
    let out = run([
        "convert",
        input.to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
    ]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let html = fs::read_to_string(&out_path).expect("output file written");
    assert!(html.contains("<h1>Hi</h1>"), "{html}");
    assert!(html.contains("<strong>bold</strong>"), "{html}");
    assert!(
        stderr_of(&out).contains("note.md -> note.html"),
        "success line"
    );

    // An alias extension infers too (`htm` -> Html).
    let alias_path = dir.path().join("alias.htm");
    let alias = run([
        "convert",
        input.to_str().unwrap(),
        "-o",
        alias_path.to_str().unwrap(),
    ]);
    assert_eq!(code(&alias), 0, "stderr: {}", stderr_of(&alias));
    assert!(alias_path.is_file(), "alias output written");

    // `-t` still wins over an inferable `-o` extension.
    let txt_path = dir.path().join("forced.txt");
    let forced = run([
        "convert",
        input.to_str().unwrap(),
        "-t",
        "txt",
        "-o",
        txt_path.to_str().unwrap(),
    ]);
    assert_eq!(code(&forced), 0, "stderr: {}", stderr_of(&forced));
    let txt = fs::read_to_string(&txt_path).expect("output file written");
    assert!(
        !txt.contains("<h1>"),
        "-t wins over the -o extension: {txt}"
    );
}

#[test]
fn unknown_output_extension_is_a_usage_error() {
    // No `-t` and an extension nobody can map: exit 2, and the message names
    // the path so the user can see what was rejected.
    let dir = TempDir::new("infer-unknown");
    let input = dir.file("note.md", "# Hi\n");
    let out_path = dir.path().join("note.zzz");
    let out = run([
        "convert",
        input.to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
    ]);

    assert_eq!(code(&out), 2, "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(
        err.contains("pass -t/--to or -o/--output"),
        "actionable message: {err}"
    );
    assert!(err.contains("note.zzz"), "message names the path: {err}");
    assert!(!out_path.exists(), "nothing written on a usage error");
}

#[test]
fn stdin_streams_html_to_stdout() {
    let out = run_stdin(["convert", "-", "-t", "html"], "# Hi\n\n**bold**\n");

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(text.contains("<h1>Hi</h1>"), "{text}");
    assert!(text.contains("<strong>bold</strong>"), "{text}");
    assert!(stderr_of(&out).contains("- -> -"), "success line");
}

#[test]
fn stdin_text_is_routed_to_the_markdown_reader() {
    // docs/cli.md rule: stdin detection landing on plain `Text` uses the
    // Markdown reader (markdown is a superset of plain text), so markdown
    // syntax from a pipe converts instead of coming out verbatim.
    let bold = run_stdin(["convert", "-", "-t", "html"], "**bold**\n");
    assert_eq!(code(&bold), 0, "stderr: {}", stderr_of(&bold));
    let html = stdout_of(&bold);
    assert!(html.contains("<strong>bold</strong>"), "{html}");
    assert!(!html.contains("**bold**"), "not verbatim text: {html}");

    let heading = run_stdin(["convert", "-", "-t", "md"], "<h1>Hi</h1>\n");
    assert_eq!(code(&heading), 0, "stderr: {}", stderr_of(&heading));
    assert_eq!(stdout_of(&heading).trim(), "# Hi");
}

#[test]
fn unsupported_format_exits_3() {
    let dir = TempDir::new("unsupported");
    let input = dir.file("doc.pdf", "%PDF-1.4\n");
    let out = run(["convert", input.to_str().unwrap(), "-t", "html"]);

    assert_eq!(code(&out), 3, "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("no native reader for pdf"), "stderr: {err}");
}

#[test]
fn missing_input_exits_4() {
    let dir = TempDir::new("missing");
    let missing = dir.path().join("nope.md");
    let out = run(["convert", missing.to_str().unwrap(), "-t", "html"]);

    assert_eq!(code(&out), 4, "stderr: {}", stderr_of(&out));
    assert!(stderr_of(&out).contains("no such file:"));
}

#[test]
fn missing_target_choice_exits_2() {
    let dir = TempDir::new("usage");
    let input = dir.file("a.md", "# A\n");

    let no_target = run(["convert", input.to_str().unwrap()]);
    assert_eq!(code(&no_target), 2, "stderr: {}", stderr_of(&no_target));
    assert!(stderr_of(&no_target).contains("pass -t/--to or -o/--output"));

    let stdin_no_target = run_stdin(["convert", "-"], "# A\n");
    assert_eq!(
        code(&stdin_no_target),
        2,
        "stderr: {}",
        stderr_of(&stdin_no_target)
    );
    assert!(stderr_of(&stdin_no_target).contains("stdin requires -t"));
}

#[test]
fn batch_with_unsupported_file_exits_1() {
    let dir = TempDir::new("batch");
    dir.file("one.md", "# One\n");
    dir.file("two.pdf", "%PDF-1.4\n");
    let out_dir = dir.path().join("out");
    let out = run([
        "convert",
        dir.path().to_str().unwrap(),
        "-t",
        "html",
        "-o",
        out_dir.to_str().unwrap(),
    ]);

    assert_eq!(code(&out), 1, "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("converting one.md"), "per-file line: {err}");
    assert!(err.contains("converted"), "summary box: {err}");
    assert!(
        err.contains("1 of 2 conversions failed"),
        "error line: {err}"
    );
    assert!(
        out_dir.join("one.html").is_file(),
        "successful file written"
    );
    assert!(
        !out_dir.join("two.html").exists(),
        "failed file not written"
    );
}

#[test]
fn no_color_env_beats_color_always() {
    // DESIGN.md precedence: NO_COLOR (any value, including empty) >
    // --color=always.
    let dir = TempDir::new("color");
    let input = dir.file("a.md", "# A\n");
    let args = [
        "convert",
        input.to_str().unwrap(),
        "-t",
        "html",
        "--color=always",
    ];

    let colored = run(args);
    assert_eq!(code(&colored), 0, "stderr: {}", stderr_of(&colored));
    assert!(
        stderr_of(&colored).contains('\u{1b}'),
        "--color=always styles output: {:?}",
        stderr_of(&colored)
    );

    let neutral = Command::new(env!("CARGO_BIN_EXE_smelt"))
        .args(args)
        .env("NO_COLOR", "")
        .output()
        .expect("smelt runs");
    assert_eq!(code(&neutral), 0, "stderr: {}", stderr_of(&neutral));
    assert!(!stdout_of(&neutral).contains('\u{1b}'));
    assert!(
        !stderr_of(&neutral).contains('\u{1b}'),
        "NO_COLOR wins: {:?}",
        stderr_of(&neutral)
    );
}

#[test]
fn doctor_reports_engines_and_exits_0() {
    let out = run(["doctor"]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let text = stdout_of(&out);
    for engine in ["soffice", "tesseract", "pandoc"] {
        assert!(text.contains(engine), "row for {engine}: {text}");
    }
    assert!(text.contains("smelt M0 uses no external engines"), "{text}");
}

#[test]
fn completions_bash_prints_a_script() {
    let out = run(["completions", "bash"]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let script = stdout_of(&out);
    assert!(script.contains("_smelt"), "script head: {script}");
}

#[test]
fn convert_alias_matches_the_subcommand() {
    let dir = TempDir::new("alias");
    let input = dir.file("b.md", "**x**\n");
    let out = run(["c", input.to_str().unwrap(), "-t", "html"]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let html = fs::read_to_string(dir.path().join("b.html")).expect("output file written");
    assert!(html.contains("<strong>x</strong>"), "{html}");
}
