//! Watch command (M3).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::SmeltError;
use crate::cli::{ConvertArgs, WatchArgs};
use crate::commands::convert;
use crate::ui::{ColorChoice, Theme};

pub fn run(args: &WatchArgs) -> Result<(), SmeltError> {
    let dir = &args.dir;
    if !dir.is_dir() {
        return Err(SmeltError::usage(format!(
            "watch: not a directory: {}",
            dir.display()
        )));
    }
    let to = args
        .to
        .as_deref()
        .ok_or_else(|| SmeltError::usage("watch requires -t/--to target format"))?;

    let mut seen: HashMap<std::path::PathBuf, SystemTime> = HashMap::new();
    let theme = Theme::new(ColorChoice::Auto);
    eprintln!("watching {} -> .{}", dir.display(), to);
    loop {
        scan_and_convert(dir, to, &mut seen, &theme)?;
        thread::sleep(Duration::from_millis(1500));
    }
}

fn scan_and_convert(
    dir: &Path,
    to: &str,
    seen: &mut HashMap<std::path::PathBuf, SystemTime>,
    theme: &Theme,
) -> Result<(), SmeltError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| SmeltError::new(1, format!("cannot read dir {}: {}", dir.display(), e)))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if let Ok(meta) = entry.metadata()
            && let Ok(modified) = meta.modified()
        {
            let prev = seen.get(&path).copied();
            if prev.map(|p| p < modified).unwrap_or(true) {
                let cargs = ConvertArgs {
                    input: Some(path.clone()),
                    to: Some(to.to_string()),
                    output: None,
                    quiet: true,
                    verbose: false,
                    recursive: false,
                };
                match convert::run(&cargs, theme, ColorChoice::Auto) {
                    Ok(()) => {
                        eprintln!("converted: {}", path.display());
                    }
                    Err(e) => {
                        eprintln!("watch: {}: {}", path.display(), e.message);
                    }
                }
                seen.insert(path, modified);
            }
        }
    }
    Ok(())
}
