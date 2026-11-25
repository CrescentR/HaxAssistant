use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// How the OCR worker process is started.
#[derive(Debug, Clone, PartialEq)]
pub enum OcrLauncher {
    /// Standalone compiled runner (`ocr/hexbox-ocr[.exe]`); needs no Python.
    Compiled(PathBuf),
    /// `python -u worker.py` from the `ocr-sidecar` sources.
    Python { python: PathBuf, script: PathBuf },
}

impl OcrLauncher {
    fn check(&self) -> Result<()> {
        match self {
            OcrLauncher::Compiled(exe) if !exe.exists() => {
                Err(anyhow!("ocr runner not found: {}", exe.display()))
            }
            OcrLauncher::Python { script, .. } if !script.exists() => Err(anyhow!(
                "ocr worker script not found: {} (把仓库的 ocr-sidecar 文件夹放到程序目录，或设置 HEXBOX_OCR_SIDECAR_DIR)",
                script.display()
            )),
            _ => Ok(()),
        }
    }

    fn command(&self) -> Command {
        match self {
            OcrLauncher::Compiled(exe) => Command::new(exe),
            OcrLauncher::Python { python, script } => {
                let mut cmd = Command::new(python);
                cmd.arg("-u").arg(script);
                cmd
            }
        }
    }

    fn describe(&self) -> String {
        match self {
            OcrLauncher::Compiled(exe) => exe.display().to_string(),
            OcrLauncher::Python { python, .. } => format!(
                "{} (请安装 Python 3.10+ 并 pip install -r ocr-sidecar/requirements.txt，或设置 ARAM_ASSISTANT_OCR_PYTHON)",
                python.display()
            ),
        }
    }
}

struct RunningWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl RunningWorker {
    fn spawn(launcher: &OcrLauncher, data_dir: &Path, model_dir: &Path) -> Result<Self> {
        let mut cmd = launcher.command();
        cmd.env("ARAM_ASSISTANT_DATA_DIR", data_dir)
            .env("ARAM_ASSISTANT_OCR_MODEL_DIR", model_dir)
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: keep a console window from flashing over the game.
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd
            .spawn()
            .map_err(|err| anyhow!("无法启动 OCR 进程 {}: {err}", launcher.describe()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to open ocr worker stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to open ocr worker stdout"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        let payload = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&payload)?;
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;

        let mut resp_line = String::new();
        let read_bytes = self.stdout.read_line(&mut resp_line)?;
        if read_bytes == 0 {
            return Err(anyhow!("ocr worker closed stdout unexpectedly"));
        }
        let parsed: Value = serde_json::from_str(resp_line.trim())?;
        Ok(parsed)
    }

    fn shutdown(&mut self) {
        let _ = self.request(0, "shutdown", json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct PersistentOcrWorker {
    launcher: OcrLauncher,
    data_dir: PathBuf,
    model_dir: PathBuf,
    next_id: AtomicU64,
    process: Option<RunningWorker>,
}

impl PersistentOcrWorker {
    pub fn new(launcher: OcrLauncher, data_dir: PathBuf, model_dir: PathBuf) -> Self {
        Self {
            launcher,
            data_dir,
            model_dir,
            next_id: AtomicU64::new(1),
            process: None,
        }
    }

    fn ensure_running(&mut self) -> Result<&mut RunningWorker> {
        let needs_spawn = match self.process.as_mut() {
            Some(proc_ref) => !proc_ref.is_alive(),
            None => true,
        };
        if needs_spawn {
            self.launcher.check()?;
            let worker = RunningWorker::spawn(&self.launcher, &self.data_dir, &self.model_dir)?;
            self.process = Some(worker);
        }
        self.process
            .as_mut()
            .ok_or_else(|| anyhow!("ocr worker failed to start"))
    }

    pub fn launcher(&self) -> &OcrLauncher {
        &self.launcher
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let first_try = {
            let worker = self.ensure_running()?;
            worker.request(id, method, params.clone())
        };
        match first_try {
            Ok(v) => Ok(v),
            Err(_) => {
                // One automatic restart attempt if the sidecar died mid-flight
                if let Some(mut old) = self.process.take() {
                    old.shutdown();
                }
                let worker = self.ensure_running()?;
                worker.request(id, method, params)
            }
        }
    }

    pub fn ping(&mut self) -> Result<Value> {
        self.call("ping", json!({}))
    }

    pub fn run_pipeline(
        &mut self,
        preset_key: &str,
        screen_image_path: Option<&str>,
        performance_mode: &str,
    ) -> Result<Value> {
        let mut params = json!({
            "preset_key": preset_key,
            "performance_mode": performance_mode,
        });
        if let Some(img) = screen_image_path {
            params["screen_image_path"] = Value::String(img.to_string());
        }
        self.call("run_pipeline", params)
    }

    pub fn watch_tick(
        &mut self,
        preset_key: &str,
        screen_image_path: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({
            "preset_key": preset_key,
        });
        if let Some(img) = screen_image_path {
            params["screen_image_path"] = Value::String(img.to_string());
        }
        self.call("watch_tick", params)
    }

    pub fn reset_watcher(&mut self) -> Result<Value> {
        self.call("reset_watcher", json!({}))
    }
}

impl Drop for PersistentOcrWorker {
    fn drop(&mut self) {
        if let Some(mut worker) = self.process.take() {
            worker.shutdown();
        }
    }
}
