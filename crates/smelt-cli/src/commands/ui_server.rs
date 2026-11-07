//! Localhost GUI server for `smelt ui`.
//! Runs purely locally on 127.0.0.1 with zero external network access.
//! Zero extra runtime dependencies (uses std::net).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

use smelt_core::{Format, read_document, writer_for};
use smelt_detect::detect;

use crate::SmeltError;
use crate::cli::UiArgs;

const HTML_CONTENT: &str = include_str!("ui_assets.html");

pub fn run(args: &UiArgs) -> Result<(), SmeltError> {
    let host = "127.0.0.1";
    let listener = TcpListener::bind((host, args.port)).map_err(|e| {
        SmeltError::new(
            1,
            format!(
                "failed to bind local GUI server to {host}:{}: {e}",
                args.port
            ),
        )
    })?;

    let url = format!("http://{host}:{}", args.port);
    println!("smelt ui: serving local GUI on {url} (Ctrl+C to quit)");

    if !args.no_open {
        open_browser(&url);
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // shortcut: sequential handling on localhost, upgrade to threadpool if multi-client load needed
                if let Err(e) = handle_client(stream) {
                    eprintln!("smelt ui: connection error: {e}");
                }
            }
            Err(e) => {
                eprintln!("smelt ui: accept error: {e}");
            }
        }
    }

    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
}

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0];
    let full_path = parts[1];

    let mut content_length = 0usize;
    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(val) = lower.strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
        line.clear();
    }

    let (path, query) = match full_path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (full_path, ""),
    };

    match (method, path) {
        ("GET", "/" | "/index.html") => {
            let body = HTML_CONTENT.as_bytes();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes())?;
            stream.write_all(body)?;
        }
        ("GET", "/api/formats") => {
            let formats_json = "[\"md\",\"html\",\"txt\",\"csv\",\"tsv\",\"pdf\",\"epub\"]";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                formats_json.len(),
                formats_json
            );
            stream.write_all(response.as_bytes())?;
        }
        ("POST", "/api/convert") => {
            let mut body_bytes = vec![0u8; content_length];
            reader.read_exact(&mut body_bytes)?;

            let target_name = parse_query_param(query, "to").unwrap_or_else(|| "html".to_string());
            let source_name = parse_query_param(query, "from");

            match perform_conversion(&body_bytes, source_name.as_deref(), &target_name) {
                Ok((out_bytes, mime)) => {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        mime,
                        out_bytes.len()
                    );
                    stream.write_all(response.as_bytes())?;
                    stream.write_all(&out_bytes)?;
                }
                Err(err_msg) => {
                    let err_bytes = err_msg.as_bytes();
                    let response = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        err_bytes.len()
                    );
                    stream.write_all(response.as_bytes())?;
                    stream.write_all(err_bytes)?;
                }
            }
        }
        _ => {
            let not_found = "404 Not Found";
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                not_found.len(),
                not_found
            );
            stream.write_all(response.as_bytes())?;
        }
    }

    stream.flush()?;
    Ok(())
}

fn parse_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == key
        {
            return Some(v.to_string());
        }
    }
    None
}

fn perform_conversion(
    input_bytes: &[u8],
    from_name: Option<&str>,
    to_name: &str,
) -> Result<(Vec<u8>, &'static str), String> {
    let target = Format::from_name(to_name)
        .ok_or_else(|| format!("unsupported target format: {to_name}"))?;

    let source = match from_name {
        Some(name) if !name.is_empty() && name != "auto" => {
            Format::from_name(name).ok_or_else(|| format!("unsupported source format: {name}"))?
        }
        _ => {
            let d = detect(input_bytes, None);
            if d.format == Format::Text {
                Format::Markdown
            } else {
                d.format
            }
        }
    };

    let doc = read_document(source, input_bytes)
        .map_err(|e| format!("cannot parse input as {}: {e}", source.name()))?;

    let writer = writer_for(target)
        .ok_or_else(|| format!("no writer available for target {}", target.name()))?;

    let out = writer
        .write(&doc, target)
        .map_err(|e| format!("failed to write {}: {e}", target.name()))?;

    let mime = match target {
        Format::Html => "text/html; charset=utf-8",
        Format::Markdown => "text/markdown; charset=utf-8",
        Format::Text => "text/plain; charset=utf-8",
        Format::Csv => "text/csv; charset=utf-8",
        Format::Tsv => "text/tab-separated-values; charset=utf-8",
        Format::Pdf => "application/pdf",
        Format::Epub => "application/epub+zip",
        _ => "application/octet-stream",
    };

    Ok((out, mime))
}
