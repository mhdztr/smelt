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
fn batch_output_collision_is_a_usage_error() {
    // `notes.md` and `notes.html` both map to `notes.txt`. Before this check
    // the parallel writes raced and one silently overwrote the other at
    // exit 0 (docs/cli.md: collision is a usage error, exit 2).
    let dir = TempDir::new("batch-collision");
    dir.file("notes.md", "# Notes\n");
    dir.file("notes.html", "<p>Notes</p>\n");
    let out_dir = dir.path().join("out");
    let out = run([
        "convert",
        dir.path().to_str().unwrap(),
        "-t",
        "txt",
        "-o",
        out_dir.to_str().unwrap(),
    ]);

    assert_eq!(code(&out), 2, "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(
        err.contains("batch output collision"),
        "message names the problem: {err}"
    );
    assert!(err.contains("notes.md"), "names the first input: {err}");
    assert!(err.contains("notes.html"), "names the second input: {err}");
    assert!(
        !out_dir.join("notes.txt").exists(),
        "nothing written on a collision"
    );
}

#[test]
fn batch_distinct_stems_still_convert() {
    // The collision check must not reject a legitimate batch.
    let dir = TempDir::new("batch-ok");
    dir.file("one.md", "# One\n");
    dir.file("two.md", "# Two\n");
    let out_dir = dir.path().join("out");
    let out = run([
        "convert",
        dir.path().to_str().unwrap(),
        "-t",
        "html",
        "-o",
        out_dir.to_str().unwrap(),
    ]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    assert!(out_dir.join("one.html").is_file());
    assert!(out_dir.join("two.html").is_file());
}

#[test]
fn stdin_and_file_interpret_plain_text_identically() {
    // Regression: piped text went through the Markdown reader while the same
    // bytes in a `.txt` file went through `TextReader`, so
    // `printf 'a **b** c' | smelt convert - -t txt` emitted `a b c`
    // (emphasis markers dropped by TextWriter) while the file conversion
    // preserved them. Both channels now resolve plain `Text` to the Markdown
    // reader, so identical bytes give identical output.
    //
    // The `-t txt` target still drops emphasis markers on BOTH channels --
    // that is the documented TextWriter lossy case, not a channel difference.
    let dir = TempDir::new("channel-parity");
    let input = dir.file("p.txt", "a **b** c\n");
    let out_path = dir.path().join("p.out.txt");

    let piped = run_stdin(["convert", "-", "-t", "txt"], "a **b** c\n");
    assert_eq!(code(&piped), 0, "stderr: {}", stderr_of(&piped));
    let from_stdin = stdout_of(&piped);

    let from_file = run([
        "convert",
        input.to_str().unwrap(),
        "-t",
        "txt",
        "-o",
        out_path.to_str().unwrap(),
    ]);
    assert_eq!(code(&from_file), 0, "stderr: {}", stderr_of(&from_file));
    let file_out = fs::read_to_string(&out_path).expect("output written");

    assert_eq!(
        from_stdin.trim(),
        file_out.trim(),
        "stdin and file outputs must match"
    );
    // Neither channel keeps the markers and neither drops them alone: the
    // behavior is symmetric (docs/cli.md stdin-source rule). Content is
    // preserved; only the markup is dropped, as for any other target.
    assert!(
        !from_stdin.contains("**") && !file_out.contains("**"),
        "emphasis markers must drop on both channels: stdin={from_stdin:?} file={file_out:?}"
    );
    assert!(
        from_stdin.contains('b') && file_out.contains('b'),
        "emphasis content must survive: stdin={from_stdin:?} file={file_out:?}"
    );

    // The Markdown-superset convenience is what makes the pipe useful: a
    // heading marker from a pipe is structure, not literal text.
    let heading = run_stdin(["convert", "-", "-t", "html"], "# Hi\n");
    assert_eq!(code(&heading), 0, "stderr: {}", stderr_of(&heading));
    assert!(stdout_of(&heading).contains("<h1>Hi</h1>"), "pipe heading");
}

#[test]
fn permission_denied_is_not_reported_as_a_missing_file() {
    // `metadata`/`detect_path` failures used to all map to exit 4 with
    // "no such file", which is actively wrong when the file exists but is
    // unreadable (docs/cli.md exit-code table).
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("noperm");
    let input = dir.file("secret.md", "# Secret\n");
    let mut perms = fs::metadata(&input).expect("stat").permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&input, perms).expect("chmod");

    let out = run(["convert", input.to_str().unwrap(), "-t", "html"]);

    // Restore before asserting so the temp-dir cleanup can remove it.
    let mut perms = fs::metadata(&input).expect("stat").permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&input, perms).expect("chmod back");

    // Root can read anything; skip the assertion if the chmod did not bite.
    if code(&out) == 0 {
        eprintln!("skipping: running as root, permission bit not enforced");
        return;
    }
    assert_eq!(code(&out), 1, "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("permission denied"), "reason given: {err}");
    assert!(
        !err.contains("no such file"),
        "must not claim the file is missing: {err}"
    );
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
fn completions_cover_every_documented_shell() {
    // docs/cli.md lists bash, elvish, fish, powershell, zsh.
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let out = run(["completions", shell]);
        assert_eq!(code(&out), 0, "{shell}: stderr {}", stderr_of(&out));
        let script = stdout_of(&out);
        assert!(
            !script.trim().is_empty(),
            "{shell} produced no completion script"
        );
        assert!(
            script.contains("smelt"),
            "{shell} script does not mention the binary: {script}"
        );
    }
}

#[test]
fn info_json_shape_is_stable() {
    // docs/cli.md `--json` shape: exact keys, and null rules for non-text
    // formats. Hand-built JSON with no shape assertion is how this drifted
    // silently before.
    let dir = TempDir::new("info-json");
    let input = dir.file("note.md", "# Hi\n\nsome words here\n");
    let expected = [
        "bytes",
        "confidence",
        "evidence",
        "format",
        "lines",
        "path",
        "size_bytes",
        "words",
    ];

    let out = run(["info", input.to_str().unwrap(), "--json"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let value: serde_json::Value =
        serde_json::from_str(&stdout_of(&out)).expect("valid JSON on stdout");

    let obj = value.as_object().expect("top-level object");
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, expected, "exact key set");
    assert_eq!(obj["format"], "Markdown");
    assert_eq!(obj["confidence"], "guessed");
    assert_eq!(obj["evidence"], "extension .md");
    assert!(obj["size_bytes"].is_u64(), "file size is a number");
    assert!(obj["lines"].is_u64(), "text stats are numbers");

    // A non-text format has null stats (docs/cli.md null rules).
    let pdf = dir.file("doc.pdf", "%PDF-1.4\n");
    let pdf_out = run(["info", pdf.to_str().unwrap(), "--json"]);
    assert_eq!(code(&pdf_out), 0, "stderr: {}", stderr_of(&pdf_out));
    let pdf_json: serde_json::Value =
        serde_json::from_str(&stdout_of(&pdf_out)).expect("valid JSON");
    assert_eq!(pdf_json["format"], "Pdf");
    for key in ["lines", "words", "bytes"] {
        assert!(pdf_json[key].is_null(), "{key} is null for a PDF");
    }
    assert!(pdf_json["size_bytes"].is_u64(), "size known for a file");
}

#[test]
fn info_json_from_stdin_has_a_null_size() {
    // `size_bytes` is null for stdin: unknown until read (docs/cli.md).
    let out = run_stdin(["info", "-", "--json"], "# Hi\n");
    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let value: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(value["path"], "-");
    assert!(
        value["size_bytes"].is_null(),
        "size_bytes is null for stdin: {value}"
    );
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

// ---------------------------------------------------------------------------
// PTY-based tests
//
// `run`/`run_stdin` always pipe stdin and stdout, so any code path gated on
// `isatty` is invisible to them. These tests allocate a real pseudoterminal
// (std-only: `openpty` from libc is called through a tiny inline FFI-free
// helper built on `/dev/ptmx`) so the terminal branches are actually
// exercised. Skipped when `/dev/ptmx` is unavailable.
// ---------------------------------------------------------------------------

/// Owns a PTY master/slave pair. Std-only: the slave is opened through the
/// Unix98 `/dev/ptmx` interface via `posix_openpt`-equivalent syscalls
/// issued through `std::fs`/`libc`-free helpers would be racy, so we shell
/// out to `script(1)` when present and skip otherwise.
struct Pty;

impl Pty {
    /// Run `smelt <args>` with a PTY on stdin/stdout and no input written.
    /// Returns `(exit_code, combined_output)`. `None` when no PTY helper is
    /// available on this machine.
    fn run_no_input<I, S>(args: I) -> Option<(i32, String)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        // `script -qec CMD /dev/null` attaches a PTY and echoes output.
        let mut cmdline = String::from(env!("CARGO_BIN_EXE_smelt"));
        for arg in args {
            cmdline.push(' ');
            cmdline.push_str(&shell_quote(arg.as_ref()));
        }
        let output = Command::new("script")
            .arg("-qec")
            .arg(&cmdline)
            .arg("/dev/null")
            .stdin(Stdio::null())
            .output()
            .ok()?;
        let code = output.status.code()?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Some((code, text))
    }
}

fn shell_quote(s: &OsStr) -> String {
    let s = s.to_string_lossy();
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[test]
fn html_without_doctype_is_parsed_not_passed_through() {
    // Regression: a `.html` file with no doctype and no `<html>`/`<head>`/
    // `<body>` used to be detected as Text, read by the plain-text reader,
    // and written out as literal markup with exit 0.
    let dir = TempDir::new("html-fragment");
    let input = dir.file("page.html", "<p>Hello <strong>world</strong></p>\n");
    let out = run(["convert", input.to_str().unwrap(), "-t", "md"]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let md = fs::read_to_string(dir.path().join("page.md")).expect("output written");
    assert!(md.contains("**world**"), "emphasis parsed: {md}");
    assert!(!md.contains("<strong>"), "no literal markup: {md}");
    assert!(!md.contains("<p>"), "no literal markup: {md}");
}

#[test]
fn html_extension_without_any_markup_still_converts_as_html() {
    // No recognizable tag in the prefix: the `.html` extension decides, so
    // the file is not silently treated as plain text.
    let dir = TempDir::new("html-ext");
    let input = dir.file("bare.html", "a bare fragment without tags\n");
    let out = run(["convert", input.to_str().unwrap(), "-t", "md"]);

    assert_eq!(code(&out), 0, "stderr: {}", stderr_of(&out));
    let md = fs::read_to_string(dir.path().join("bare.md")).expect("output written");
    assert!(md.contains("a bare fragment"), "{md}");
}

#[test]
fn opendocument_spreadsheet_reports_its_own_format() {
    // An ODS is not an ODT: the exit-3 message must name the right format.
    let dir = TempDir::new("ods");
    let path = dir.path().join("sheet.ods");
    fs::write(&path, minimal_ods_zip()).expect("fixture written");
    let out = run(["convert", path.to_str().unwrap(), "-t", "md"]);

    assert_eq!(code(&out), 3, "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("no native reader for ods"), "stderr: {err}");
}

/// A minimal stored-entry zip stream carrying an OpenDocument spreadsheet
/// mimetype plus `content.xml` (enough for the detection scan).
fn minimal_ods_zip() -> Vec<u8> {
    fn entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        out
    }
    let mut zip = entry(
        "mimetype",
        b"application/vnd.oasis.opendocument.spreadsheet",
    );
    zip.extend(entry("content.xml", b"<office/>"));
    zip
}

#[test]
fn info_with_no_argument_on_a_terminal_exits_2_without_panicking() {
    // Regression: this is the default invocation at a shell prompt. The
    // panicking branch was unreachable from the piped tests above because
    // `isatty(stdin)` was always false there.
    let Some((exit, text)) = Pty::run_no_input(["info"]) else {
        eprintln!("skipping: no `script` helper for PTY tests on this machine");
        return;
    };

    assert_eq!(exit, 2, "output: {text}");
    assert!(
        !text.contains("panicked"),
        "must not panic on user input: {text}"
    );
    assert!(
        text.contains("missing input"),
        "actionable usage message: {text}"
    );
}

#[test]
fn convert_with_no_argument_on_a_terminal_exits_2() {
    // Same shape for `convert`; it already errored correctly, this guards it.
    let Some((exit, text)) = Pty::run_no_input(["convert"]) else {
        eprintln!("skipping: no `script` helper for PTY tests on this machine");
        return;
    };

    assert_eq!(exit, 2, "output: {text}");
    assert!(!text.contains("panicked"), "must not panic: {text}");
    assert!(text.contains("missing input"), "message: {text}");
}
