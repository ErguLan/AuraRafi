//! AuraRafi Editor native entry point.
//!
//! The executable owns only logging and the Winit event loop. RafUI owns
//! retained editor surfaces and ApiGraphicBasic owns scene/UI composition.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const LOG_LIMIT_BYTES: u64 = 16 * 1024 * 1024;

fn main() {
    let log_path = init_logging();
    install_panic_hook(log_path.clone());
    tracing::info!(log = %log_path.display(), "Proyecto Rafi Editor starting with native RafUI host");

    if std::env::args().nth(1).as_deref() == Some("--runtime") {
        let result = std::env::args()
            .nth(2)
            .ok_or_else(|| "runtime launch payload is missing".to_string())
            .and_then(|json| {
                if json.len() > 8192 {
                    return Err("runtime launch payload is too large".into());
                }
                serde_json::from_str::<raf_player::PlayerLaunch>(&json).map_err(|e| e.to_string())
            })
            .and_then(raf_player::run);
        if let Err(error) = result {
            tracing::error!(%error, "local runtime stopped with an error");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = raf_editor::native_application::run_native() {
        tracing::error!(%error, "native editor stopped with an error");
        std::process::exit(1);
    }
}

fn init_logging() -> PathBuf {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    rotate_log(&path);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()
        .map(|file| Arc::new(Mutex::new(file)));
    let writer = Tee { file };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_ansi(false)
        .with_writer(writer)
        .init();
    path
}

fn log_path() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local").join("state"))
        })
        .unwrap_or_else(std::env::temp_dir);
    root.join("AuraRafi").join("logs").join("editor.log")
}

/// Keeps the on-disk log bounded. The previous file is retained for diagnosis.
fn rotate_log(path: &Path) {
    let too_large = std::fs::metadata(path)
        .map(|meta| meta.len() > LOG_LIMIT_BYTES)
        .unwrap_or(false);
    if !too_large {
        return;
    }
    let previous = path.with_file_name("editor.previous.log");
    let _ = std::fs::remove_file(&previous);
    let _ = std::fs::rename(path, previous);
}

fn install_panic_hook(log_path: PathBuf) {
    let crash_path = log_path.with_file_name("crash.log");
    std::panic::set_hook(Box::new(move |info| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = format!("PANIC at {stamp} ({location}): {info}");
        eprintln!("{message}");
        for path in [&log_path, &crash_path] {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{message}");
                let _ = file.flush();
            }
        }
    }));
}

#[derive(Clone)]
struct Tee {
    file: Option<Arc<Mutex<File>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Tee {
    type Writer = TeeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TeeWriter {
            file: self.file.clone(),
        }
    }
}

struct TeeWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buffer);
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.write_all(buffer);
            }
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.flush();
            }
        }
        Ok(())
    }
}
