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
    /// Pede ao widget da tela inicial para reler o resumo (`application::widget`).
    fn update_widget(&self) {}
}
