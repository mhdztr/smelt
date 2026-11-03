//! LibreOffice/soffice pool and precondition checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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

struct Worker {
    child: Option<Child>,
    profile_dir: PathBuf,
    last_used: Instant,
    failures: u32,
}

struct PoolState {
    workers: Vec<Worker>,
    shutdown: bool,
    idle_timeout: Duration,
}

pub struct SofficePool {
    state: Arc<Mutex<PoolState>>,
    cv: Arc<Condvar>,
    shutdown_flag: Arc<AtomicBool>,
    soffice_path: Option<PathBuf>,
}

impl Default for SofficePool {
    fn default() -> Self {
        Self::new()
    }
}

impl SofficePool {
    pub fn new() -> Self {
        let pre = check_soffice_preconditions();
        Self {
            state: Arc::new(Mutex::new(PoolState {
                workers: Vec::new(),
                shutdown: false,
                idle_timeout: Duration::from_secs(30),
            })),
            cv: Arc::new(Condvar::new()),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            soffice_path: pre.path,
        }
    }

    pub fn start(&self) -> PoolResult<()> {
        let pre = check_soffice_preconditions();
        if !pre.ok {
            return Err(PoolError::Message(format!("soffice pool: {}", pre.message)));
        }
        // For now, ensure at least one worker slot concept is "started"
        // Real workers will be spawned on demand
        Ok(())
    }

    pub fn stop(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        let mut state = self.state.lock().unwrap();
        state.shutdown = true;
        for w in state.workers.iter_mut() {
            if let Some(mut child) = w.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = fs::remove_dir_all(&w.profile_dir);
        }
        state.workers.clear();
        self.cv.notify_all();
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
        let state = self.state.lock().unwrap();
        let workers = state.workers.iter().filter(|w| w.child.is_some()).count();
        if workers == 0 {
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
