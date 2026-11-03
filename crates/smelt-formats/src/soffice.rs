//! LibreOffice/soffice pool and precondition checks.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::error::{PoolError, PoolResult};
use crate::pool::{EnginePool, PoolHealth};

#[derive(Debug, Clone)]
pub struct SofficePreconditions {
    pub ok: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub message: String,
}

fn find_soffice() -> Option<PathBuf> {
    // CLI is responsible for env, but formats crate needs to find it too
    // Try common names
    if let Ok(path) = std::env::var("SMELT_SOFFICE") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    for name in &["soffice", "soffice.bin", "libreoffice"] {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
    }
    None
}

fn get_version(soffice: &PathBuf) -> Option<String> {
    let out = Command::new(soffice)
        .args(["--version"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        let s2 = String::from_utf8_lossy(&out.stderr);
        let line2 = s2.lines().next().unwrap_or("").trim();
        if line2.is_empty() {
            None
        } else {
            Some(line2.to_string())
        }
    } else {
        Some(line.to_string())
    }
}

pub fn check_soffice_preconditions() -> SofficePreconditions {
    match find_soffice() {
        Some(path) => match get_version(&path) {
            Some(version) => SofficePreconditions {
                ok: true,
                path: Some(path),
                version: Some(version),
                message: "ready".to_string(),
            },
            None => SofficePreconditions {
                ok: false,
                path: Some(path),
                version: None,
                message: "soffice found but --version failed; check headless mode support"
                    .to_string(),
            },
        },
        None => SofficePreconditions {
            ok: false,
            path: None,
            version: None,
            message:
                "soffice not found in PATH. Install LibreOffice for docx/odt/pptx/xlsx conversion."
                    .to_string(),
        },
    }
}

pub struct SofficePool {
    workers: Arc<Mutex<Vec<()>>>,
    shutdown: Arc<AtomicBool>,
    last_activity: Arc<Mutex<Instant>>,
    started: Arc<AtomicBool>,
}

impl Default for SofficePool {
    fn default() -> Self {
        Self::new()
    }
}

impl SofficePool {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(Mutex::new(Vec::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            started: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self) -> PoolResult<()> {
        let pre = check_soffice_preconditions();
        if !pre.ok {
            return Err(PoolError::Message(format!("soffice pool: {}", pre.message)));
        }
        if self.started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        // Placeholder: real worker pool will be implemented when wiring conversions
        let mut workers = self.workers.lock().unwrap();
        workers.push(());
        *self.last_activity.lock().unwrap() = Instant::now();
        Ok(())
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        let mut workers = self.workers.lock().unwrap();
        workers.clear();
    }
}

impl EnginePool for SofficePool {
    fn name(&self) -> &'static str {
        "soffice"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        let pre = check_soffice_preconditions();
        if !pre.ok {
            return Ok(PoolHealth {
                ok: false,
                workers: 0,
                message: Some(format!("soffice pool: no workers. {}", pre.message)),
            });
        }
        let workers = self.workers.lock().unwrap().len();
        if !self.started.load(Ordering::SeqCst) || workers == 0 {
            Ok(PoolHealth {
                ok: false,
                workers: 0,
                message: Some("soffice pool: no workers. run 'smelt doctor'".to_string()),
            })
        } else {
            Ok(PoolHealth {
                ok: true,
                workers,
                message: Some("ready".to_string()),
            })
        }
    }

    fn shutdown(&self) {
        self.stop();
    }
}
