//! Configuration file support (M4).

use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub color: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let path = Self::default_path();
        if let Some(path) = path {
            if let Ok(content) = std::fs::read_to_string(&path) {
                // Minimal parser: look for color = "..."
                for line in content.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("color") {
                        let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
                        let val = rest.trim_matches('"').trim_matches('\'').to_string();
                        if !val.is_empty() {
                            return Self { color: Some(val) };
                        }
                    }
                }
            }
        }
        Self::default()
    }

    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("smelt").join("config.toml"))
    }
}
