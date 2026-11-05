//! LibreOffice/soffice pool and preconditions.

#![allow(dead_code, unused_imports)]

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::pool::{EnginePool, PoolHealth};
use crate::{PoolError, PoolResult};

#[derive(Debug)]
struct Worker {
    child: Option<Child>,
    last_used: Instant,
    failures: u32,
}

#[derive(Debug)]
struct PoolState {
    workers: Vec<Worker>,
    max_workers: usize,
    idle_timeout: Duration,
    user: Option<String>,
    queue: VecDeque<()>,
}

#[derive(Debug)]
pub struct SofficePool {
    state: Arc<Mutex<PoolState>>,
    #[allow(dead_code)]
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
                max_workers: 4,
                idle_timeout: Duration::from_secs(300),
                user: pre.user,
                queue: VecDeque::new(),
            })),
            soffice_path: pre.path.clone(),
        }
    }

    pub fn start(&self, workers: usize) -> PoolResult<()> {
        let mut state = self.state.lock().unwrap();
        let to_start = workers.min(state.max_workers).max(1);
        for _ in 0..to_start {
            state.workers.push(Worker {
                child: None,
                last_used: Instant::now(),
                failures: 0,
            });
        }
        Ok(())
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        for w in state.workers.iter_mut() {
            if let Some(ref mut child) = w.child {
                let _ = child.kill();
                let _ = child.wait();
            }
            w.child = None;
        }
        state.workers.clear();
    }

    pub fn health_check(&self) -> PoolResult<PoolHealth> {
        self.health()
    }

    pub fn shutdown_idle(&self) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let timeout = state.idle_timeout;
        state.workers.retain(|w| {
            if w.child.is_none() {
                return now.duration_since(w.last_used) < timeout;
            }
            true
        });
        if state.workers.is_empty() {
            state.queue.clear();
        }
    }

    pub fn recycle_unhealthy(&self) {
        let mut state = self.state.lock().unwrap();
        for w in state.workers.iter_mut() {
            if w.failures >= 3 {
                if let Some(ref mut child) = w.child {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                w.child = None;
                w.failures = 0;
            }
        }
        state
            .workers
            .retain(|w| w.child.is_some() || w.failures < 3);
    }

    pub fn enqueue_request(&self) {
        let mut state = self.state.lock().unwrap();
        state.queue.push_back(());
    }

    pub fn queue_depth(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.queue.len()
    }
}

#[derive(Debug)]
pub struct Preconditions {
    pub ok: bool,
    pub message: String,
    pub path: Option<PathBuf>,
    pub user: Option<String>,
}

pub fn check_soffice_preconditions() -> Preconditions {
    if let Ok(path) = which::which("soffice") {
        return Preconditions {
            ok: true,
            message: "soffice found".to_string(),
            path: Some(path),
            user: std::env::var("USER").ok(),
        };
    }
    for candidate in ["/usr/bin/soffice", "/usr/lib/libreoffice/program/soffice"] {
        if std::path::Path::new(candidate).exists() {
            return Preconditions {
                ok: true,
                message: "soffice found".to_string(),
                path: Some(PathBuf::from(candidate)),
                user: std::env::var("USER").ok(),
            };
        }
    }
    Preconditions {
        ok: false,
        message: "soffice not found in PATH".to_string(),
        path: None,
        user: std::env::var("USER").ok(),
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
                ok: true,
                workers: state.workers.len().max(1),
                message: Some("ready".to_string()),
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

#[derive(Debug)]
pub struct ConversionResult {
    pub data: Vec<u8>,
}

impl SofficePool {
    pub fn convert(&self, input: &[u8], input_ext: &str, output_ext: &str) -> PoolResult<Vec<u8>> {
        let pre = check_soffice_preconditions();
        if !pre.ok {
            return Err(PoolError::Message(format!("soffice pool: {}", pre.message)));
        }
        let soffice = pre.path.as_ref().unwrap();
        let tmpdir = tempfile::tempdir().map_err(PoolError::Io)?;
        let in_path = tmpdir.path().join(format!("in.{input_ext}"));
        let mut file = fs::File::create(&in_path).map_err(PoolError::Io)?;
        file.write_all(input).map_err(PoolError::Io)?;

        let outdir = tmpdir.path().join("out");
        fs::create_dir_all(&outdir).map_err(PoolError::Io)?;

        let mut cmd = Command::new(soffice);
        cmd.args([
            "--headless",
            "--invisible",
            "--nologo",
            "--nofirststartwizard",
            "--convert-to",
            output_ext,
            "--outdir",
            outdir.to_str().unwrap(),
            in_path.to_str().unwrap(),
        ]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let output = cmd.output().map_err(PoolError::Io)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PoolError::Message(format!(
                "soffice conversion failed: {}",
                stderr.trim()
            )));
        }
        let out_path = outdir.join(format!("in.{output_ext}"));
        if !out_path.exists() {
            for entry in fs::read_dir(&outdir).map_err(PoolError::Io)? {
                let entry = entry.map_err(PoolError::Io)?;
                if entry.file_type().map_err(PoolError::Io)?.is_file() {
                    let data = fs::read(entry.path()).map_err(PoolError::Io)?;
                    return Ok(data);
                }
            }
            return Err(PoolError::Message("soffice produced no output".to_string()));
        }
        let data = fs::read(&out_path).map_err(PoolError::Io)?;
        Ok(data)
    }
}
