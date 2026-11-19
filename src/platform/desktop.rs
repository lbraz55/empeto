//! Implementação de desenvolvimento (Linux/Windows/macOS) para testar a interface sem aparelho.
//! - Importar: usa o arquivo mais recente em `<dados>/entrada/` (ou WORKOUT_IMPORT_FILE).
//! - Exportar: grava em `<dados>/saida/`.

use super::{FileResult, Platform};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

pub struct DesktopPlatform {
    dir: PathBuf,
    pending: RefCell<Option<FileResult>>,
}

impl DesktopPlatform {
    pub fn new() -> DesktopPlatform {
        let dir = std::env::var_os("WORKOUT_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("workout-data"));
        let _ = std::fs::create_dir_all(dir.join("entrada"));
        let _ = std::fs::create_dir_all(dir.join("saida"));
        DesktopPlatform { dir, pending: RefCell::new(None) }
    }
}

impl Default for DesktopPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for DesktopPlatform {
    fn is_android(&self) -> bool {
        false
    }
    fn data_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn open_document(&self, mimes: &[&str]) {
        let wants_zip = mimes.iter().any(|m| m.contains("zip"));
        let chosen = std::env::var_os("WORKOUT_IMPORT_FILE").map(PathBuf::from).or_else(|| {
            let mut files: Vec<_> = std::fs::read_dir(self.dir.join("entrada"))
                .ok()?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    if wants_zip { ext == "zip" } else { matches!(ext.as_str(), "xlsx" | "xls" | "ods") }
                })
                .collect();
            files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
            files.pop()
        });
        let r = match chosen {
            Some(p) => {
                let tmp = self.tmp_dir();
                let _ = std::fs::create_dir_all(&tmp);
                let dst = tmp.join(format!("import-{}", crate::domain::now_ms()));
                match std::fs::copy(&p, &dst) {
                    Ok(_) => FileResult::Opened {
                        path: dst,
                        name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    },
                    Err(e) => FileResult::Error(e.to_string()),
                }
            }
            None => FileResult::Error(format!(
                "Desktop: coloque o arquivo em {}/entrada/",
                self.dir.display()
            )),
        };
        *self.pending.borrow_mut() = Some(r);
    }

    fn save_document(&self, suggested_name: &str, _mime: &str, src: &Path) {
        let dst = self.dir.join("saida").join(suggested_name);
        let r = match std::fs::copy(src, &dst) {
            Ok(_) => FileResult::Saved { name: dst.display().to_string() },
            Err(e) => FileResult::Error(e.to_string()),
        };
        let _ = std::fs::remove_file(src);
        *self.pending.borrow_mut() = Some(r);
    }

    fn poll_file_result(&self) -> Option<FileResult> {
        self.pending.borrow_mut().take()
    }

    fn notifications_allowed(&self) -> bool {
        true
    }
    fn request_notification_permission(&self) {}
    fn poll_permission_result(&self) -> Option<bool> {
        None
    }
    fn open_notification_settings(&self) {}

    fn schedule_alarm(&self, id: i32, at_boot_ms: i64, title: &str, _text: &str, _sound: bool, _vibrate: bool) {
        log::info!("[desktop] alarme {id} agendado para boot+{at_boot_ms} ms: {title}");
    }
    fn cancel_alarm(&self, id: i32) {
        log::info!("[desktop] alarme {id} cancelado");
    }
    fn show_countdown(&self, _end_wall_ms: i64, _title: &str, _text: &str) {}
    fn cancel_countdown(&self) {}

    fn play_tone(&self, tone: i32, _duration_ms: i32) {
        log::info!("[desktop] som {tone}");
        eprint!("\x07");
    }
    fn vibrate(&self, ms: i64) {
        log::info!("[desktop] vibrar {ms} ms");
    }
    fn keep_screen_on(&self, _on: bool) {}
    fn system_dark_mode(&self) -> bool {
        std::env::var("WORKOUT_SYSTEM_THEME").map(|v| v != "light").unwrap_or(true)
    }
    fn add_calendar_event(&self, title: &str, _d: &str, begin_ms: i64, _e: i64, _a: bool) -> bool {
        log::info!("[desktop] agenda: {title} em {begin_ms}");
        false
    }
    fn move_to_back(&self) {}
}
