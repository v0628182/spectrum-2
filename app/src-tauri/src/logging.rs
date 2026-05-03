use crate::models::{LogFileDto, LogSnapshotDto};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_subscriber::EnvFilter;

static LOG_ROOT: OnceLock<PathBuf> = OnceLock::new();
static FRONTEND_LOG: OnceLock<PathBuf> = OnceLock::new();
static APP_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();
static LOG_INIT: OnceLock<()> = OnceLock::new();
static SESSION_MARKER: OnceLock<String> = OnceLock::new();

/// In-memory ring buffer for release builds — no disk footprint.
/// Capped at MAX_RING_LINES to prevent unbounded memory growth.
static MEMORY_RING: OnceLock<Mutex<RingBuffer>> = OnceLock::new();
const MAX_RING_LINES: usize = 512;

struct RingBuffer {
    lines: Vec<String>,
}

impl RingBuffer {
    fn new() -> Self {
        Self {
            lines: Vec::with_capacity(MAX_RING_LINES),
        }
    }

    fn push(&mut self, line: String) {
        if self.lines.len() >= MAX_RING_LINES {
            self.lines.remove(0);
        }
        self.lines.push(line);
    }

    fn snapshot(&self) -> Vec<String> {
        self.lines.clone()
    }
}

fn ring() -> &'static Mutex<RingBuffer> {
    MEMORY_RING.get_or_init(|| Mutex::new(RingBuffer::new()))
}

fn is_release_build() -> bool {
    !cfg!(debug_assertions)
}

const LOG_FILES: [(&str, &str); 7] = [
    ("app", "vanysound-app.log"),
    ("frontend", "vanysound-frontend.log"),
    ("control", "control.log"),
    ("master", "echosuite_master.log"),
    ("apo", "echosuite_apo.log"),
    ("hifi", "hificable_install.log"),
    ("loudness", "echosuite_loudness.log"),
];
const DEBUG_REPORT_FILE: &str = "vanysound-debug-report.txt";

pub fn init_logging() {
    if LOG_INIT.get().is_some() {
        return;
    }

    // Ensure ring buffer is initialized regardless of build mode
    let _ = ring();

    if is_release_build() {
        // ═══════════════════════════════════════════════════════════
        // RELEASE: No file logging. Tracing goes to a sink (dropped).
        // In-memory ring buffer captures what we need for on-demand
        // debug reports without leaving forensic artifacts on disk.
        // ═══════════════════════════════════════════════════════════
        let filter = EnvFilter::new("error");
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::sink)
            .with_ansi(false)
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);
        purge_stale_log_files();
    } else {
        // ═══════════════════════════════════════════════════════════
        // DEV: Full file logging — same behavior as before.
        // ═══════════════════════════════════════════════════════════
        let root = log_root();
        let _ = fs::create_dir_all(&root);
        write_session_markers();

        let appender = tracing_appender::rolling::never(&root, "vanysound-app.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(appender);
        let _ = APP_GUARD.set(guard);

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,tauri=warn,wry=warn"));
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(non_blocking)
            .with_ansi(false)
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);

        tracing::info!(
            session_marker = %session_marker(),
            log_root = %root.display(),
            "VanySound backend logging initialized"
        );
    }

    std::panic::set_hook(Box::new(|panic_info| {
        append_frontend_log("panic", &panic_info.to_string());
    }));
    let _ = LOG_INIT.set(());
}

/// Deletes leftover log files from previous dev/release runs.
/// Called once at startup in release mode to clean the slate.
fn purge_stale_log_files() {
    let root = log_root();
    if !root.exists() {
        return;
    }

    // Only delete our known log files — don't nuke the directory
    // (install_receipt.json lives in parent, not in logs/)
    let targets = [
        "vanysound-app.log",
        "vanysound-frontend.log",
        "control.log",
        "vanysound-debug-report.txt",
        "overlay.log",
    ];

    for name in targets {
        let path = root.join(name);
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }
}

pub fn log_root() -> PathBuf {
    LOG_ROOT
        .get_or_init(|| {
            let root = std::env::var_os("ProgramData")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
                .join("VanySound")
                .join("logs");
            let _ = fs::create_dir_all(&root);
            root
        })
        .clone()
}

fn frontend_log_path() -> PathBuf {
    FRONTEND_LOG
        .get_or_init(|| log_root().join("vanysound-frontend.log"))
        .clone()
}

fn app_log_path() -> PathBuf {
    log_root().join("vanysound-app.log")
}

fn control_log_path() -> PathBuf {
    log_root().join("control.log")
}

pub fn debug_report_path() -> PathBuf {
    log_root().join(DEBUG_REPORT_FILE)
}

fn session_marker() -> &'static str {
    SESSION_MARKER
        .get_or_init(|| {
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or(0);
            format!("VANYSOUND_SESSION_START_{millis}")
        })
        .as_str()
}

fn write_session_markers() {
    if is_release_build() {
        return;
    }
    let marker = session_marker().to_string();
    append_line(&app_log_path(), "session", &marker);
    append_line(&frontend_log_path(), "session", &marker);
    append_line(&control_log_path(), "session", &marker);
}

pub fn append_frontend_log(level: &str, message: &str) {
    if is_release_build() {
        // Memory only — no disk trace
        if let Ok(mut buf) = ring().lock() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            buf.push(format!(
                "[{now}][{}] {}",
                level.to_ascii_uppercase(),
                message
            ));
        }
        return;
    }
    append_line(&frontend_log_path(), level, message);
}

pub fn append_line(path: &Path, level: &str, message: &str) {
    if is_release_build() {
        // Memory only — no disk writes in release
        if let Ok(mut buf) = ring().lock() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            buf.push(format!(
                "[{now}][{}] {}",
                level.to_ascii_uppercase(),
                message
            ));
        }
        return;
    }

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{now}][{}] {}", level.to_ascii_uppercase(), message);
    }
}

pub fn log_snapshot() -> LogSnapshotDto {
    let root = log_root();

    if is_release_build() {
        // Return in-memory ring buffer snapshot
        let ring_lines = ring().lock().map(|buf| buf.snapshot()).unwrap_or_default();

        let combined = ring_lines
            .iter()
            .rev()
            .take(80)
            .rev()
            .map(|line| format!("[mem] {line}"))
            .collect();

        return LogSnapshotDto {
            combined_tail: combined,
            files: Vec::new(),
            log_dir: root.display().to_string(),
        };
    }

    let mut combined = Vec::new();
    let mut files = Vec::new();

    for (key, name) in LOG_FILES {
        let path = if matches!(key, "master" | "apo" | "hifi" | "loudness") {
            std::env::temp_dir().join(name)
        } else {
            root.join(name)
        };

        let tail = tail_lines(&path, 32);
        if !tail.is_empty() {
            for line in tail.iter().rev().take(12).rev() {
                combined.push(format!("[{key}] {line}"));
            }
        }

        files.push(LogFileDto {
            key: key.to_string(),
            path: path.display().to_string(),
            tail,
        });
    }

    if combined.len() > 80 {
        combined = combined.split_off(combined.len() - 80);
    }

    LogSnapshotDto {
        combined_tail: combined,
        files,
        log_dir: root.display().to_string(),
    }
}

pub fn debug_report() -> String {
    if is_release_build() {
        // On-demand report from memory — never persisted to disk
        let ring_lines = ring().lock().map(|buf| buf.snapshot()).unwrap_or_default();

        let generated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut report = String::new();
        report.push_str("=== VanySound Debug Report ===\n");
        report.push_str(&format!("generated_at_unix={generated_at}\n"));
        report.push_str("mode=release\n");
        report.push_str(&format!("ring_lines={}\n\n", ring_lines.len()));
        for line in &ring_lines {
            report.push_str(line);
            report.push('\n');
        }
        // NOT written to disk — returned in-memory only
        return report;
    }

    let root = log_root();
    let marker = session_marker().to_string();
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    let mut report = String::new();
    report.push_str("=== VanySound Debug Report ===\n");
    report.push_str(&format!("generated_at_unix={generated_at}\n"));
    report.push_str(&format!("session_marker={marker}\n"));
    report.push_str(&format!("log_dir={}\n", root.display()));

    for (key, name) in LOG_FILES {
        let path = if matches!(key, "master" | "apo" | "hifi" | "loudness") {
            std::env::temp_dir().join(name)
        } else {
            root.join(name)
        };

        report.push_str("\n\n");
        report.push_str(&format!("=== {key} ({}) ===\n", path.display()));
        report.push_str(&read_log_since_session(&path, &marker));
    }

    let report_path = debug_report_path();
    let _ = fs::write(&report_path, &report);
    report
}

fn tail_lines(path: &Path, max_lines: usize) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut lines: Vec<String> = content
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();
    lines.retain(|line| !line.is_empty());
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    lines
}

fn read_log_since_session(path: &Path, marker: &str) -> String {
    match fs::read_to_string(path) {
        Ok(content) if !content.trim().is_empty() => {
            if let Some(index) = content.rfind(marker) {
                content[index..].trim().to_string()
            } else {
                content.trim().to_string()
            }
        }
        Ok(_) => "<archivo vacio>".to_string(),
        Err(error) => format!("No se pudo leer {}: {}", path.display(), error),
    }
}
