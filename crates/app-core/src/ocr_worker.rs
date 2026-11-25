use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use shared_types::OcrCacheCleanupReport;

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

    /// Single-shot CLI fallback when the persistent JSON-RPC pipe fails.
    pub fn run_oneshot_fallback(
        &self,
        preset_key: &str,
        screen_image_path: Option<&str>,
        performance_mode: &str,
        lang: &str,
        allow_non_lol_foreground: bool,
    ) -> Result<Value> {
        self.launcher.check()?;
        let mut cmd = self.launcher.command();
        cmd.arg("--oneshot")
            .arg("--preset-key")
            .arg(preset_key)
            .arg("--performance-mode")
            .arg(performance_mode)
            .arg("--lang")
            .arg(lang)
            .env("ARAM_ASSISTANT_DATA_DIR", &self.data_dir)
            .env("ARAM_ASSISTANT_OCR_MODEL_DIR", &self.model_dir)
            .env("PYTHONIOENCODING", "utf-8");
        if let Some(img) = screen_image_path {
            cmd.arg("--screen-image").arg(img);
        }
        if allow_non_lol_foreground {
            cmd.arg("--allow-non-lol-foreground");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let output = cmd.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let last_line = stdout
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .ok_or_else(|| anyhow!("oneshot OCR returned empty output"))?;
        let mut parsed: Value = serde_json::from_str(last_line.trim())?;
        if let Some(obj) = parsed.as_object_mut() {
            obj.insert("fallback_mode".to_string(), Value::String("oneshot-cli".to_string()));
        }
        Ok(parsed)
    }

    pub fn ping(&mut self) -> Result<Value> {
        self.call("ping", json!({}))
    }

    pub fn run_pipeline(
        &mut self,
        preset_key: &str,
        screen_image_path: Option<&str>,
        performance_mode: &str,
        lang: &str,
        allow_non_lol_foreground: bool,
    ) -> Result<Value> {
        let mut params = json!({
            "preset_key": preset_key,
            "performance_mode": performance_mode,
            "lang": lang,
            "allow_non_lol_foreground": allow_non_lol_foreground,
        });
        if let Some(img) = screen_image_path {
            params["screen_image_path"] = Value::String(img.to_string());
        }
        match self.call("run_pipeline", params) {
            Ok(val) => Ok(val),
            Err(primary_err) => self
                .run_oneshot_fallback(
                    preset_key,
                    screen_image_path,
                    performance_mode,
                    lang,
                    allow_non_lol_foreground,
                )
                .map_err(|fallback_err| {
                    anyhow!("常驻 OCR 失败 ({primary_err})，单次 CLI 回退亦失败 ({fallback_err})")
                }),
        }
    }

    pub fn watch_tick(
        &mut self,
        preset_key: &str,
        screen_image_path: Option<&str>,
        performance_mode: &str,
        lang: &str,
        allow_non_lol_foreground: bool,
    ) -> Result<Value> {
        let mut params = json!({
            "preset_key": preset_key,
            "performance_mode": performance_mode,
            "lang": lang,
            "allow_non_lol_foreground": allow_non_lol_foreground,
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

/// Remove temporary OCR screenshot captures in `<data_dir>/ocr_cache` and OS temp dir.
pub fn cleanup_ocr_cache(data_dir: &Path) -> OcrCacheCleanupReport {
    let mut removed_files = 0usize;
    let mut freed_bytes = 0u64;
    let mut scanned_dirs = Vec::new();

    let cache_dir = data_dir.join("ocr_cache");
    if cache_dir.is_dir() {
        scanned_dirs.push(cache_dir.display().to_string());
        if let Ok(entries) = fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    if fs::remove_file(&path).is_ok() {
                        removed_files += 1;
                        freed_bytes += size;
                    }
                }
            }
        }
    }

    let tmp_dir = std::env::temp_dir();
    scanned_dirs.push(tmp_dir.display().to_string());
    if let Ok(entries) = fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_hexbox_capture = (name.starts_with("hexbox_ocr_") || name.starts_with("hexbox-ocr-capture-"))
                && (name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".tmp"));
            if path.is_file() && is_hexbox_capture {
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if fs::remove_file(&path).is_ok() {
                    removed_files += 1;
                    freed_bytes += size;
                }
            }
        }
    }

    OcrCacheCleanupReport {
        removed_files,
        freed_bytes,
        scanned_dirs,
    }
}
