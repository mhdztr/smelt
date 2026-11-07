//! Configuration file support (M4).

use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub color: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let path = Self::default_path();
        if let Some(path) = path
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            return Self::parse_str(&content);
        }
        Self::default()
    }

    pub fn parse_str(content: &str) -> Self {
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
        Self::default()
    }

    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("smelt").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_color() {
        let cfg = Config::parse_str(r#"color = "always""#);
        assert_eq!(cfg.color.as_deref(), Some("always"));

        let cfg2 = Config::parse_str("color = 'never'");
        assert_eq!(cfg2.color.as_deref(), Some("never"));

        let empty = Config::parse_str("# comment only");
        assert_eq!(empty.color, None);
    }
}
