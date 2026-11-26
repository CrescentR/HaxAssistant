//! Live process-based discovery for the running League of Legends client (LCU).
//!
//! Instead of blindly scanning fixed disk paths (which misses custom install
//! directories and gets fooled by stale lockfiles left after crashes), we first
//! check whether `LeagueClientUx` is actually running right now:
//!
//! 1. If an explicit override env var (`HAXASSISTANT_LCU_LOCKFILE` /
//!    `HEXBOX_LCU_LOCKFILE`) is set, read that lockfile directly (dev/testing).
//! 2. Check the live OS process table for `LeagueClientUx`:
//!    - **Not running** → return `None` immediately (zero disk scanning).
//!    - **Running** → parse `--app-port=` and `--remoting-auth-token=` straight
//!      from the process command line (works on any drive or install directory).
//!    - If `--install-directory=` is present without a token, read `<dir>/lockfile`.
//!    - On Windows UAC elevation where `LeagueClientUx.exe` is confirmed running
//!      as Administrator and hides its command line from a non-elevated process,
//!      fall back to reading its `lockfile` only while the process is alive.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::LockfileCredentials;

const CACHE_TTL: Duration = Duration::from_millis(1200);

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
struct CachedDiscovery {
    checked_at: Instant,
    #[allow(dead_code)]
    ux_pid: Option<u32>,
    creds: Option<LockfileCredentials>,
}

fn cache_slot() -> &'static Mutex<Option<CachedDiscovery>> {
    static CACHE: OnceLock<Mutex<Option<CachedDiscovery>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Clear cached credentials (e.g. after an LCU request fails or WebSocket closes).
pub fn invalidate_discovery_cache() {
    if let Ok(mut guard) = cache_slot().lock() {
        *guard = None;
    }
}

/// Legacy/fallback candidate lockfile paths. Never scanned unless explicitly
/// requested or `LeagueClientUx` is already confirmed running and its command
/// line could not be inspected (e.g. elevated process under Windows UAC).
pub fn candidate_lockfile_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for env_key in ["HAXASSISTANT_LCU_LOCKFILE", "HEXBOX_LCU_LOCKFILE"] {
        if let Ok(env_path) = std::env::var(env_key) {
            let trimmed = env_path.trim();
            if !trimmed.is_empty() {
                paths.push(PathBuf::from(trimmed));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/Applications/League of Legends.app/Contents/LoL/lockfile",
        ));
        if let Ok(home) = std::env::var("HOME") {
            paths.push(
                PathBuf::from(home)
                    .join("Applications/League of Legends.app/Contents/LoL/lockfile"),
            );
        }
    }

    #[cfg(target_os = "windows")]
    {
        for drive in ["C", "D", "E", "F", "G", "H"] {
            paths.push(PathBuf::from(format!(
                "{drive}:\\Riot Games\\League of Legends\\lockfile"
            )));
            paths.push(PathBuf::from(format!(
                "{drive}:\\WeGameApps\\英雄联盟\\LeagueClient\\lockfile"
            )));
            paths.push(PathBuf::from(format!(
                "{drive}:\\Program Files\\腾讯游戏\\英雄联盟\\LeagueClient\\lockfile"
            )));
            paths.push(PathBuf::from(format!(
                "{drive}:\\腾讯游戏\\英雄联盟\\LeagueClient\\lockfile"
            )));
        }
    }

    paths
}

fn read_explicit_env_lockfile() -> Option<LockfileCredentials> {
    for env_key in ["HAXASSISTANT_LCU_LOCKFILE", "HEXBOX_LCU_LOCKFILE"] {
        if let Ok(env_path) = std::env::var(env_key) {
            let trimmed = env_path.trim();
            if trimmed.is_empty() {
                continue;
            }
            let path = PathBuf::from(trimmed);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(creds) = LockfileCredentials::parse(&content, path) {
                    return Some(creds);
                }
            }
        }
    }
    None
}

/// Discover LCU credentials only when the League of Legends client is actually running.
pub fn discover_lockfile() -> Option<LockfileCredentials> {
    if let Some(creds) = read_explicit_env_lockfile() {
        return Some(creds);
    }

    let now = Instant::now();
    if let Ok(guard) = cache_slot().lock() {
        if let Some(cached) = guard.as_ref() {
            if now.duration_since(cached.checked_at) < CACHE_TTL {
                return cached.creds.clone();
            }
        }
    }

    let prev_cached = cache_slot()
        .lock()
        .ok()
        .and_then(|g| g.clone());

    let (ux_pid, creds) = discover_from_running_process(prev_cached.as_ref());

    if let Ok(mut guard) = cache_slot().lock() {
        *guard = Some(CachedDiscovery {
            checked_at: Instant::now(),
            ux_pid,
            creds: creds.clone(),
        });
    }

    creds
}

/// Extract a `--key=value` flag from a process command line, handling both
/// double-quoted arguments (Windows: `"--install-directory=D:\My Games\LoL"`)
/// and unquoted arguments (macOS: `--install-directory=/Applications/League of Legends.app/Contents/LoL`).
///
/// Ensures `--key=` starts at a token boundary (beginning of line, whitespace,
/// or quote), so `--app-port=` never accidentally matches `--riotclient-app-port=`.
pub fn extract_cmdline_flag(cmdline: &str, flag_name: &str) -> Option<String> {
    let needle = format!("--{flag_name}=");
    let bytes = cmdline.as_bytes();
    let mut search_from = 0usize;

    while let Some(rel_idx) = cmdline[search_from..].find(&needle) {
        let start = search_from + rel_idx;
        let preceded_by_quote = start > 0 && bytes[start - 1] == b'"';
        let valid_boundary =
            start == 0 || preceded_by_quote || bytes[start - 1].is_ascii_whitespace();
        if !valid_boundary {
            search_from = start + needle.len();
            continue;
        }

        let val_start = start + needle.len();
        let rest = &cmdline[val_start..];
        let raw_val = if preceded_by_quote {
            // Quoted whole arg: "--key=value with spaces"
            match rest.find('"') {
                Some(end) => &rest[..end],
                None => rest,
            }
        } else if let Some(after_q) = rest.strip_prefix('"') {
            // Flag with quoted value: --key="value with spaces"
            match after_q.find('"') {
                Some(end) => &after_q[..end],
                None => after_q,
            }
        } else if flag_name == "install-directory" {
            // Unquoted macOS/Linux path may contain spaces; stops before next " --" or end.
            match rest.find(" --") {
                Some(end) => rest[..end].trim(),
                None => rest.trim(),
            }
        } else {
            // Simple token without spaces (port, auth token, pid)
            let end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '"')
                .unwrap_or(rest.len());
            &rest[..end]
        };

        let val = raw_val.trim();
        if !val.is_empty() {
            return Some(val.to_string());
        }
        search_from = val_start;
    }
    None
}

/// Parse LCU credentials from a running `LeagueClientUx` command line.
pub fn parse_ux_cmdline(cmdline: &str, fallback_pid: u32) -> Option<LockfileCredentials> {
    // Reject helper/renderer child processes that do not host the UX flags
    let port_str = extract_cmdline_flag(cmdline, "app-port")?;
    let port: u16 = port_str.parse().ok()?;
    let password = extract_cmdline_flag(cmdline, "remoting-auth-token")?;
    let pid = extract_cmdline_flag(cmdline, "app-pid")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(fallback_pid);

    let path = extract_cmdline_flag(cmdline, "install-directory")
        .map(|dir| PathBuf::from(dir).join("lockfile"))
        .unwrap_or_else(|| PathBuf::from(format!("process://LeagueClientUx/{pid}")));

    Some(LockfileCredentials {
        process_name: "LeagueClientUx".to_string(),
        pid,
        port,
        password,
        protocol: "https".to_string(),
        path,
    })
}

fn read_lockfile_from_install_dir(cmdline: &str) -> Option<LockfileCredentials> {
    let dir = extract_cmdline_flag(cmdline, "install-directory")?;
    let path = PathBuf::from(dir).join("lockfile");
    let content = fs::read_to_string(&path).ok()?;
    LockfileCredentials::parse(&content, path).ok()
}

fn read_fallback_lockfile_while_running() -> Option<LockfileCredentials> {
    for path in candidate_lockfile_paths() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(creds) = LockfileCredentials::parse(&content, path) {
                return Some(creds);
            }
        }
    }
    None
}

// ============================================================================
// Unix (macOS / Linux) live process scanning
// ============================================================================

#[cfg(unix)]
fn parse_unix_ps_output(output: &str) -> Option<(u32, LockfileCredentials)> {
    let mut running_ux_pid: Option<u32> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((pid_str, cmd)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid_str.trim().parse::<u32>() else {
            continue;
        };
        let cmd = cmd.trim();
        if !cmd.contains("LeagueClientUx") {
            continue;
        }
        // Skip Chromium helper/renderer child processes
        if cmd.contains("LeagueClientUx Helper") || cmd.contains("LeagueClientUxRender") {
            continue;
        }
        running_ux_pid = Some(pid);

        if let Some(creds) = parse_ux_cmdline(cmd, pid) {
            return Some((pid, creds));
        }
        if let Some(creds) = read_lockfile_from_install_dir(cmd) {
            return Some((pid, creds));
        }
    }

    // Confirmed LeagueClientUx IS running, but its command line hid args:
    // safely fall back to lockfile read only because the process is live.
    if let Some(pid) = running_ux_pid {
        if let Some(creds) = read_fallback_lockfile_while_running() {
            return Some((pid, creds));
        }
    }

    None
}

#[cfg(unix)]
fn discover_from_running_process(
    _prev: Option<&CachedDiscovery>,
) -> (Option<u32>, Option<LockfileCredentials>) {
    let Ok(out) = Command::new("ps").args(["-ax", "-o", "pid=,command="]).output() else {
        return (None, None);
    };
    if !out.status.success() {
        return (None, None);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    match parse_unix_ps_output(&stdout) {
        Some((pid, creds)) => (Some(pid), Some(creds)),
        None => (None, None),
    }
}

// ============================================================================
// Windows live process scanning (zero console window flash, fast tasklist gate)
// ============================================================================

#[cfg(target_os = "windows")]
fn win_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_windows_tasklist_pid(csv_output: &str) -> Option<u32> {
    for line in csv_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // CSV format: "LeagueClientUx.exe","12345","Console","1","123,456 K"
        let mut cols = trimmed.split(',');
        let image = cols.next()?.trim().trim_matches('"');
        if !image.eq_ignore_ascii_case("LeagueClientUx.exe") {
            continue;
        }
        let pid_str = cols.next()?.trim().trim_matches('"');
        if let Ok(pid) = pid_str.parse::<u32>() {
            return Some(pid);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn query_windows_ux_pid() -> Option<u32> {
    let out = win_command("tasklist")
        .args(["/FI", "IMAGENAME eq LeagueClientUx.exe", "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_windows_tasklist_pid(&stdout)
}

#[cfg(target_os = "windows")]
fn query_windows_ux_cmdline_and_exe() -> (Option<String>, Option<String>) {
    // 1. PowerShell CIM (works on Windows 10 & Windows 11, including 24H2 where wmic is removed)
    if let Ok(out) = win_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "$p = Get-CimInstance Win32_Process -Filter \"Name='LeagueClientUx.exe'\" | Select-Object -First 1; if ($p) { Write-Output ('CMD=' + $p.CommandLine); Write-Output ('EXE=' + $p.ExecutablePath) }",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut cmdline = None;
            let mut exe = None;
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("CMD=") {
                    let s = rest.trim();
                    if !s.is_empty() {
                        cmdline = Some(s.to_string());
                    }
                } else if let Some(rest) = line.strip_prefix("EXE=") {
                    let s = rest.trim();
                    if !s.is_empty() {
                        exe = Some(s.to_string());
                    }
                }
            }
            if cmdline.is_some() || exe.is_some() {
                return (cmdline, exe);
            }
        }
    }

    // 2. Fallback to wmic on older Windows installations
    if let Ok(out) = win_command("wmic")
        .args([
            "process",
            "where",
            "name='LeagueClientUx.exe'",
            "get",
            "CommandLine,ExecutablePath",
            "/value",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut cmdline = None;
            let mut exe = None;
            for line in text.lines() {
                if let Some(rest) = line.trim().strip_prefix("CommandLine=") {
                    let s = rest.trim();
                    if !s.is_empty() {
                        cmdline = Some(s.to_string());
                    }
                } else if let Some(rest) = line.trim().strip_prefix("ExecutablePath=") {
                    let s = rest.trim();
                    if !s.is_empty() {
                        exe = Some(s.to_string());
                    }
                }
            }
            return (cmdline, exe);
        }
    }

    (None, None)
}

#[cfg(target_os = "windows")]
fn discover_from_running_process(
    prev: Option<&CachedDiscovery>,
) -> (Option<u32>, Option<LockfileCredentials>) {
    // Fast native check (~15ms): if LeagueClientUx.exe is NOT running, stop immediately.
    let Some(ux_pid) = query_windows_ux_pid() else {
        return (None, None);
    };

    // Same live UX process PID as before → reuse cached credentials with zero PowerShell spawns!
    if let Some(cached) = prev {
        if cached.ux_pid == Some(ux_pid) && cached.creds.is_some() {
            return (Some(ux_pid), cached.creds.clone());
        }
    }

    let (cmdline, exe_path) = query_windows_ux_cmdline_and_exe();

    if let Some(ref cmd) = cmdline {
        if let Some(creds) = parse_ux_cmdline(cmd, ux_pid) {
            return (Some(ux_pid), Some(creds));
        }
        if let Some(creds) = read_lockfile_from_install_dir(cmd) {
            return (Some(ux_pid), Some(creds));
        }
    }

    // If CommandLine was hidden (e.g. UAC elevation), try lockfile next to ExecutablePath
    if let Some(exe) = exe_path {
        if let Some(parent) = PathBuf::from(exe).parent() {
            let lockfile = parent.join("lockfile");
            if let Ok(content) = fs::read_to_string(&lockfile) {
                if let Ok(creds) = LockfileCredentials::parse(&content, lockfile) {
                    return (Some(ux_pid), Some(creds));
                }
            }
        }
    }

    // Final safety net ONLY while LeagueClientUx.exe is confirmed alive (elevated WeGame UAC case)
    (Some(ux_pid), read_fallback_lockfile_while_running())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn discover_from_running_process(
    _prev: Option<&CachedDiscovery>,
) -> (Option<u32>, Option<LockfileCredentials>) {
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_windows_ux_cmdline_and_ignores_riotclient_port() {
        let cmdline = r#""G:\Custom Games\英雄联盟\LeagueClient\LeagueClientUx.exe" "--riotclient-auth-token=wrong_token" "--riotclient-app-port=11111" "--remoting-auth-token=RealToken_123-abc" "--app-port=54321" "--install-directory=G:\Custom Games\英雄联盟\LeagueClient" "--app-pid=8888""#;
        let creds = parse_ux_cmdline(cmdline, 9999).expect("parse windows cmdline");
        assert_eq!(creds.port, 54321);
        assert_eq!(creds.password, "RealToken_123-abc");
        assert_eq!(creds.pid, 8888);
        assert_eq!(
            creds.path,
            PathBuf::from(r"G:\Custom Games\英雄联盟\LeagueClient").join("lockfile")
        );
    }

    #[test]
    fn parses_unquoted_macos_ux_cmdline_with_spaces_in_install_dir() {
        let cmdline = "/Applications/League of Legends.app/Contents/LoL/LeagueClient.app/Contents/MacOS/LeagueClientUx --riotclient-app-port=11111 --remoting-auth-token=MacToken99 --app-port=61234 --install-directory=/Applications/League of Legends.app/Contents/LoL --locale=zh_CN";
        let creds = parse_ux_cmdline(cmdline, 4321).expect("parse macos cmdline");
        assert_eq!(creds.port, 61234);
        assert_eq!(creds.password, "MacToken99");
        assert_eq!(creds.pid, 4321);
        assert_eq!(
            creds.path,
            PathBuf::from("/Applications/League of Legends.app/Contents/LoL/lockfile")
        );
    }

    #[test]
    fn rejects_helper_or_unrelated_process_without_lcu_flags() {
        let helper = r#""D:\LOL\LeagueClientUxRender.exe" --type=renderer --riotclient-app-port=11111"#;
        assert!(parse_ux_cmdline(helper, 100).is_none());
    }

    #[test]
    fn parses_windows_tasklist_csv_output() {
        let running = "\"LeagueClientUx.exe\",\"14280\",\"Console\",\"1\",\"210,340 K\"\r\n";
        assert_eq!(parse_windows_tasklist_pid(running), Some(14280));

        let not_running = "INFO: No tasks are running which match the specified criteria.\r\n";
        assert_eq!(parse_windows_tasklist_pid(not_running), None);
    }

    #[cfg(unix)]
    #[test]
    fn parses_unix_ps_table_and_skips_helpers() {
        let ps_out = "\
  101 /sbin/launchd
 2020 /Applications/League of Legends.app/Contents/LoL/LeagueClientUx Helper --type=renderer
 2055 /Applications/League of Legends.app/Contents/LoL/LeagueClientUx --remoting-auth-token=Tok_A --app-port=55123 --app-pid=2001
";
        let (pid, creds) = parse_unix_ps_output(ps_out).expect("discover from ps");
        assert_eq!(pid, 2055);
        assert_eq!(creds.pid, 2001);
        assert_eq!(creds.port, 55123);
        assert_eq!(creds.password, "Tok_A");

        let empty_ps = "  101 /sbin/launchd\n  202 /usr/bin/zsh\n";
        assert!(parse_unix_ps_output(empty_ps).is_none());
    }
}
