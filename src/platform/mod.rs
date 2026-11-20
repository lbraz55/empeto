//! AndroidAdapter (seção 3.2): notificações, vibração, alarmes, seletor de arquivos e ciclo de vida.
//! Em desktop (desenvolvimento) há uma implementação simples para testar a interface.

#[cfg(target_os = "android")]
pub mod android;
pub mod crumbs;
#[cfg(not(target_os = "android"))]
pub mod desktop;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum FileResult {
    /// Arquivo escolhido pelo usuário, copiado para um temporário privado.
    Opened { path: PathBuf, name: String },
    /// Arquivo salvo no destino escolhido.
    Saved { name: String },
    Cancelled,
    Error(String),
}

/// Eventos de feedback tátil (3.5). A camada Android traduz para intensidade/duração.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapticEvent {
    SetRegistered,
    RestFinished,
    Undo,
}

impl HapticEvent {
    pub fn code(self) -> i32 {
        match self {
            HapticEvent::SetRegistered => 1,
            HapticEvent::RestFinished => 2,
            HapticEvent::Undo => 3,
        }
    }
}

/// IDs de alarmes do cronômetro.
pub const ALARM_END: i32 = 2001;
pub const ALARM_PRE: i32 = 2002;

pub trait Platform {
    fn is_android(&self) -> bool;
    /// Diretório privado do app (banco SQLite e temporários).
    fn data_dir(&self) -> PathBuf;
    fn tmp_dir(&self) -> PathBuf {
        self.data_dir().join("tmp")
    }
    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Abre o seletor de documentos (ACTION_OPEN_DOCUMENT). Resultado via `poll_file_result`.
    fn open_document(&self, mimes: &[&str]);
    /// Pede o destino (ACTION_CREATE_DOCUMENT) e copia `src` para lá. Resultado via `poll_file_result`.
    fn save_document(&self, suggested_name: &str, mime: &str, src: &Path);
    fn poll_file_result(&self) -> Option<FileResult>;

    fn notifications_allowed(&self) -> bool;
    fn request_notification_permission(&self);
    /// Some(true/false) quando o usuário responde ao diálogo de permissão.
    fn poll_permission_result(&self) -> Option<bool>;
    fn open_notification_settings(&self);

    /// Agenda alerta no relógio monotônico com suspensão (CLOCK_BOOTTIME = elapsedRealtime).
    fn schedule_alarm(&self, id: i32, at_boot_ms: i64, title: &str, text: &str, sound: bool, vibrate: bool);
    fn cancel_alarm(&self, id: i32);
    fn show_countdown(&self, end_wall_ms: i64, title: &str, text: &str);
    fn cancel_countdown(&self);

    fn play_tone(&self, tone: i32, duration_ms: i32);
    fn vibrate(&self, ms: i64);
    /// Feedback tátil curto. Sem vibrador, não faz nada (nunca impede a operação).
    fn haptic(&self, event: HapticEvent) {
        let _ = event;
    }
    fn keep_screen_on(&self, on: bool);
    /// O sistema está em modo escuro? (para o tema "seguir sistema").
    fn system_dark_mode(&self) -> bool {
        true
    }
    /// O sistema permite animações? ("remover animações" da acessibilidade do Android).
    fn system_animations_enabled(&self) -> bool {
        true
    }
    /// Idioma do sistema (ex.: "pt-BR", "en-US").
    fn system_locale(&self) -> String {
        std::env::var("WORKOUT_LANG").or_else(|_| std::env::var("LANG")).unwrap_or_default()
    }
    /// Escala de fonte do Android (Configurações › Tela › Tamanho da fonte). 1.0 no desktop.
    fn font_scale(&self) -> f32 {
        1.0
    }
    /// Abre a agenda do sistema com um evento preenchido (Intent ACTION_INSERT; sem permissão).
    /// Devolve false se não houver app de agenda.
    fn add_calendar_event(&self, title: &str, description: &str, begin_ms: i64, end_ms: i64, all_day: bool) -> bool;
    fn move_to_back(&self);
    /// Abre um link externo (vídeo) no app/navegador do aparelho (Intent ACTION_VIEW, sem permissão).
    /// Devolve false se nenhum app puder abrir o link.
    fn open_url(&self, url: &str) -> bool {
        log::info!("abrir link: {url}");
        true
    }
    /// Compartilha um texto pelo seletor do sistema (ACTION_SEND, sem permissão). Opcional para o usuário.
    fn share_text(&self, subject: &str, text: &str) -> bool {
        log::info!("compartilhar: {subject}\n{text}");
        true
    }
    // ---- Atualizações (evolução 7.1/7.2): só no Android ----
    fn supports_updates(&self) -> bool {
        false
    }
    fn device_abis(&self) -> Vec<String> {
        vec![]
    }
    fn version_code(&self) -> i64 {
        0
    }
    /// Inicia o download dos metadados; resultado em `poll_update_check`.
    fn update_check(&self, _url: &str) {}
    /// `Some(Ok(json))`, `Some(Err(motivo))` ou `None` (ainda em andamento).
    fn poll_update_check(&self) -> Option<Result<String, String>> {
        None
    }
    fn update_download(&self, _url: &str, _sha256: &str, _size: u64) {}
    /// "progress:N", "ok:<caminho>", "error:<código>" ou None.
    fn poll_update_download(&self) -> Option<String> {
        None
    }
    fn poll_install_status(&self) -> Option<String> {
        None
    }
    fn can_install_packages(&self) -> bool {
        false
    }
    fn open_install_settings(&self) {}
    fn install_apk(&self, _path: &str) -> bool {
        false
    }
    /// Pede ao widget da tela inicial para reler o resumo (`application::widget`).
    fn update_widget(&self) {}
}
