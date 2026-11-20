//! Preferências do usuário (RF-20). Guardadas em `app_preference` como texto.

use crate::domain::{AppResult, LoadUnit};
use crate::persistence::Db;

/// Tema: seguir o sistema, claro ou escuro (6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
    pub fn parse(s: &str) -> ThemeMode {
        match s {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

/// Animações: seguir o sistema, normais, reduzidas ou desativadas (6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimMode {
    System,
    Normal,
    Reduced,
    Off,
}

impl AnimMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AnimMode::System => "system",
            AnimMode::Normal => "normal",
            AnimMode::Reduced => "reduced",
            AnimMode::Off => "off",
        }
    }
    pub fn parse(s: &str) -> AnimMode {
        match s {
            "normal" => AnimMode::Normal,
            "reduced" => AnimMode::Reduced,
            "off" => AnimMode::Off,
            _ => AnimMode::System,
        }
    }
}

/// Ação ao fim do descanso (RN-06: nada acontece sem ação configurada).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestEndAction {
    None,
    /// Contagem curta de preparação, terminando com o alerta de início de série.
    PrepareCountdown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    // Sons e alertas
    pub sound_enabled: bool,
    pub tone_rest_end: i32,
    pub tone_set_start: i32,
    pub tone_pre_alert: i32,
    pub vibrate: bool,
    /// Vibração curta em ações (registrar série, desfazer) — 3.5.
    pub haptics: bool,
    /// Teclado numérico do app no modo foco (3.4); desligado = teclado do sistema.
    pub app_keypad: bool,
    /// Registrar RPE/RIR nas séries (evolução 8.4) — opcional.
    pub effort: bool,
    pub pre_alert: bool,
    pub pre_alert_seconds: i64,
    /// Alerta com app em segundo plano (notificação; requer POST_NOTIFICATIONS no Android 13+).
    pub background_alert: bool,
    /// Já explicamos e pedimos a permissão de notificação ao menos uma vez.
    pub notif_permission_asked: bool,
    // Cronômetro
    pub timer_default_seconds: i64,
    pub timer_adjust_step: i64,
    pub auto_start_rest: bool,
    pub rest_end_action: RestEndAction,
    pub prepare_seconds: i64,
    // Treinamento
    pub unit: LoadUnit,
    pub load_step_g: i64,
    pub prefill_last: bool,
    pub default_sets: i64,
    pub default_reps: i64,
    // Calendário
    pub week_starts_monday: bool,
    pub calendar_heatmap: bool,
    // Recursos opcionais (7.5) — desligar um módulo nunca apaga dados.
    pub module_records: bool,
    pub module_heatmap: bool,
    pub module_progression: bool,
    pub module_gestures: bool,
    pub module_decorative_anim: bool,
    pub module_backup_reminder: bool,
    pub calendar_week_view: bool,
    // Interface
    /// Tamanho da fonte (evolução 4.2): 0 pequena, 1 média (padrão), 2 grande, 3 extragrande.
    pub text_size: i32,
    /// Idioma da interface (evolução 4.1): "system", "pt-BR", "en" ou "es".
    pub language: String,
    /// Boas-vindas concluídas ou ignoradas (evolução 3.2).
    pub onboarding_done: bool,
    pub theme_mode: ThemeMode,
    /// 0 compacta, 1 padrão, 2 espaçosa (6.3).
    pub density: i32,
    pub anim_mode: AnimMode,
    /// 0 rápida, 1 normal, 2 lenta (6.6).
    pub anim_speed: i32,
    pub keep_screen_on: bool,
    pub confirm_finish: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            sound_enabled: true,
            tone_rest_end: 1,
            tone_set_start: 2,
            tone_pre_alert: 0,
            vibrate: true,
            haptics: true,
            app_keypad: true,
            effort: false,
            pre_alert: true,
            pre_alert_seconds: 10,
            background_alert: true,
            notif_permission_asked: false,
            timer_default_seconds: 90,
            timer_adjust_step: 15,
            auto_start_rest: false,
            rest_end_action: RestEndAction::None,
            prepare_seconds: 5,
            unit: LoadUnit::Kg,
            load_step_g: 2_500,
            prefill_last: false,
            default_sets: 3,
            default_reps: 10,
            week_starts_monday: true,
            calendar_heatmap: false,
            module_records: true,
            module_heatmap: true,
            module_progression: true,
            module_gestures: true,
            module_decorative_anim: true,
            module_backup_reminder: true,
            calendar_week_view: false,
            text_size: 1,
            language: "system".into(),
            onboarding_done: false,
            theme_mode: ThemeMode::System,
            density: 1,
            anim_mode: AnimMode::System,
            anim_speed: 1,
            keep_screen_on: true,
            confirm_finish: true,
        }
    }
}

pub const TONE_NAMES: [&str; 4] = ["Bipe curto", "Bipe duplo", "Bipe longo", "Sinal suave"];

fn b(v: bool) -> String {
    if v { "1".into() } else { "0".into() }
}

impl Prefs {
    pub fn load(db: &Db) -> AppResult<Prefs> {
        let mut p = Prefs::default();
        let mut legacy_large = false;
        let mut has_text_size = false;
        let mut old_text_size: Option<i32> = None;
        for (k, v) in db.all_prefs()? {
            let bv = v == "1";
            let iv = v.parse::<i64>().ok();
            match k.as_str() {
                "sound.enabled" => p.sound_enabled = bv,
                "sound.tone_rest_end" => p.tone_rest_end = iv.unwrap_or(1).clamp(0, 3) as i32,
                "sound.tone_set_start" => p.tone_set_start = iv.unwrap_or(2).clamp(0, 3) as i32,
                "sound.tone_pre_alert" => p.tone_pre_alert = iv.unwrap_or(0).clamp(0, 3) as i32,
                "alert.vibrate" => p.vibrate = bv,
                "alert.haptics" => p.haptics = bv,
                "ui.app_keypad" => p.app_keypad = bv,
                "training.effort" => p.effort = bv,
                "alert.pre_alert" => p.pre_alert = bv,
                "alert.pre_alert_seconds" => p.pre_alert_seconds = iv.unwrap_or(10).clamp(3, 60),
                "alert.background" => p.background_alert = bv,
                "alert.permission_asked" => p.notif_permission_asked = bv,
                "timer.default_seconds" => p.timer_default_seconds = iv.unwrap_or(90).clamp(5, 3600),
                "timer.adjust_step" => p.timer_adjust_step = iv.unwrap_or(15).clamp(5, 60),
                "timer.auto_start_rest" => p.auto_start_rest = bv,
                "timer.rest_end_action" => {
                    p.rest_end_action = if v == "prepare" { RestEndAction::PrepareCountdown } else { RestEndAction::None }
                }
                "timer.prepare_seconds" => p.prepare_seconds = iv.unwrap_or(5).clamp(3, 30),
                "training.unit" => p.unit = LoadUnit::parse(&v),
                "training.load_step_g" => p.load_step_g = iv.unwrap_or(2_500).clamp(250, 10_000),
                "training.prefill_last" => p.prefill_last = bv,
                "training.default_sets" => p.default_sets = iv.unwrap_or(3).clamp(1, 20),
                "training.default_reps" => p.default_reps = iv.unwrap_or(10).clamp(1, 100),
                "calendar.week_starts_monday" => p.week_starts_monday = bv,
                "calendar.heatmap" => p.calendar_heatmap = bv,
                "module.records" => p.module_records = bv,
                "module.heatmap" => p.module_heatmap = bv,
                "module.progression" => p.module_progression = bv,
                "module.gestures" => p.module_gestures = bv,
                "module.decorative_anim" => p.module_decorative_anim = bv,
                "module.backup_reminder" => p.module_backup_reminder = bv,
                "calendar.week_view" => p.calendar_week_view = bv,
                // Compatibilidade com versões até 0.1.4 (liga/desliga "texto grande").
                "ui.large_text" => legacy_large = bv,
                // Escala antiga (até 0.3.x): 0 normal, 1 grande, 2 extragrande → nova escala +1.
                "ui.text_size" => old_text_size = Some(iv.unwrap_or(0).clamp(0, 2) as i32),
                "ui.font_size" => {
                    has_text_size = true;
                    p.text_size = iv.unwrap_or(1).clamp(0, 3) as i32
                }
                "ui.language" => {
                    p.language = match v.as_str() {
                        "pt-BR" | "en" | "es" => v.clone(),
                        _ => "system".into(),
                    }
                }
                "onboarding.done" => p.onboarding_done = bv,
                "ui.theme" => p.theme_mode = ThemeMode::parse(&v),
                "ui.density" => p.density = iv.unwrap_or(1).clamp(0, 2) as i32,
                "ui.anim_mode" => p.anim_mode = AnimMode::parse(&v),
                "ui.anim_speed" => p.anim_speed = iv.unwrap_or(1).clamp(0, 2) as i32,
                "ui.keep_screen_on" => p.keep_screen_on = bv,
                "ui.confirm_finish" => p.confirm_finish = bv,
                _ => {}
            }
        }
        if !has_text_size {
            if let Some(t) = old_text_size {
                p.text_size = t + 1;
            } else if legacy_large {
                p.text_size = 2;
            }
        }
        Ok(p)
    }

    pub fn save(&self, db: &Db) -> AppResult<()> {
        let pairs: Vec<(&str, String)> = vec![
            ("sound.enabled", b(self.sound_enabled)),
            ("sound.tone_rest_end", self.tone_rest_end.to_string()),
            ("sound.tone_set_start", self.tone_set_start.to_string()),
            ("sound.tone_pre_alert", self.tone_pre_alert.to_string()),
            ("alert.vibrate", b(self.vibrate)),
            ("alert.haptics", b(self.haptics)),
            ("ui.app_keypad", b(self.app_keypad)),
            ("training.effort", b(self.effort)),
            ("alert.pre_alert", b(self.pre_alert)),
            ("alert.pre_alert_seconds", self.pre_alert_seconds.to_string()),
            ("alert.background", b(self.background_alert)),
            ("alert.permission_asked", b(self.notif_permission_asked)),
            ("timer.default_seconds", self.timer_default_seconds.to_string()),
            ("timer.adjust_step", self.timer_adjust_step.to_string()),
            ("timer.auto_start_rest", b(self.auto_start_rest)),
            (
                "timer.rest_end_action",
                match self.rest_end_action {
                    RestEndAction::None => "none".into(),
                    RestEndAction::PrepareCountdown => "prepare".into(),
                },
            ),
            ("timer.prepare_seconds", self.prepare_seconds.to_string()),
            ("training.unit", self.unit.as_str().into()),
            ("training.load_step_g", self.load_step_g.to_string()),
            ("training.prefill_last", b(self.prefill_last)),
            ("training.default_sets", self.default_sets.to_string()),
            ("training.default_reps", self.default_reps.to_string()),
            ("calendar.week_starts_monday", b(self.week_starts_monday)),
            ("calendar.heatmap", b(self.calendar_heatmap)),
            ("module.records", b(self.module_records)),
            ("module.heatmap", b(self.module_heatmap)),
            ("module.progression", b(self.module_progression)),
            ("module.gestures", b(self.module_gestures)),
            ("module.decorative_anim", b(self.module_decorative_anim)),
            ("module.backup_reminder", b(self.module_backup_reminder)),
            ("calendar.week_view", b(self.calendar_week_view)),
            ("ui.font_size", self.text_size.to_string()),
            ("ui.language", self.language.clone()),
            ("onboarding.done", b(self.onboarding_done)),
            ("ui.theme", self.theme_mode.as_str().into()),
            ("ui.density", self.density.to_string()),
            ("ui.anim_mode", self.anim_mode.as_str().into()),
            ("ui.anim_speed", self.anim_speed.to_string()),
            ("ui.keep_screen_on", b(self.keep_screen_on)),
            ("ui.confirm_finish", b(self.confirm_finish)),
        ];
        let tx = db.conn.unchecked_transaction()?;
        for (k, v) in pairs {
            db.set_pref(k, &v)?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let db = Db::open_in_memory().unwrap();
        let mut p = Prefs::default();
        p.unit = LoadUnit::Lb;
        p.rest_end_action = RestEndAction::PrepareCountdown;
        p.pre_alert_seconds = 7;
        p.theme_mode = ThemeMode::Light;
        p.density = 0;
        p.text_size = 3;
        p.language = "es".into();
        p.onboarding_done = true;
        p.anim_mode = AnimMode::Reduced;
        p.save(&db).unwrap();
        assert_eq!(Prefs::load(&db).unwrap(), p);
    }

    #[test]
    fn legacy_large_text_maps_to_grande() {
        let db = Db::open_in_memory().unwrap();
        db.set_pref("ui.large_text", "1").unwrap();
        assert_eq!(Prefs::load(&db).unwrap().text_size, 2); // grande
        db.set_pref("ui.text_size", "2").unwrap(); // extragrande na escala antiga
        assert_eq!(Prefs::load(&db).unwrap().text_size, 3);
        db.set_pref("ui.font_size", "0").unwrap(); // nova chave prevalece
        assert_eq!(Prefs::load(&db).unwrap().text_size, 0);
        assert_eq!(Prefs::default().text_size, 1); // média é o padrão
    }
}
