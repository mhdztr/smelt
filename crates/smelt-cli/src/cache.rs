//! Content-hash conversion cache.

use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use smelt_core::Document;

#[derive(Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new() -> Option<Self> {
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                home.join(".cache")
            })
            .join("smelt");
        fs::create_dir_all(&dir).ok()?;
        Some(Self { dir })
    }

    fn key(&self, input: &[u8], direction: &str, engine_version: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input);
        hasher.update(b"\0");
        hasher.update(direction.as_bytes());
        hasher.update(b"\0");
        hasher.update(engine_version.as_bytes());
        let hash = hasher.finalize();
        hex::encode(hash)
    }

    pub fn get_document(
        &self,
        input: &[u8],
        direction: &str,
        engine_version: &str,
    ) -> Option<Document> {
        let key = self.key(input, direction, engine_version);
        let path = self.dir.join(format!("{key}.json"));
        let data = fs::read(&path).ok()?;
        serde_json::from_slice(&data).ok()
    }

    pub fn put_document(
        &self,
        input: &[u8],
        direction: &str,
        engine_version: &str,
        doc: &Document,
    ) -> Option<()> {
        let key = self.key(input, direction, engine_version);
        let path = self.dir.join(format!("{key}.json"));
        let data = serde_json::to_vec(doc).ok()?;
        fs::write(&path, data).ok()
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new().unwrap_or_else(|| Cache {
            dir: PathBuf::from("/tmp/smelt-cache"),
        })
    }
}
