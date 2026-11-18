//! Preferências do usuário (RF-20). Guardadas em `app_preference` como texto.

use crate::domain::{AppResult, LoadUnit};
use crate::persistence::Db;

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
    pub calendar_week_view: bool,
    // Interface
    pub large_text: bool,
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
            prefill_last: true,
            default_sets: 3,
            default_reps: 10,
            week_starts_monday: true,
            calendar_week_view: false,
            large_text: false,
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
        for (k, v) in db.all_prefs()? {
            let bv = v == "1";
            let iv = v.parse::<i64>().ok();
            match k.as_str() {
                "sound.enabled" => p.sound_enabled = bv,
                "sound.tone_rest_end" => p.tone_rest_end = iv.unwrap_or(1).clamp(0, 3) as i32,
                "sound.tone_set_start" => p.tone_set_start = iv.unwrap_or(2).clamp(0, 3) as i32,
                "sound.tone_pre_alert" => p.tone_pre_alert = iv.unwrap_or(0).clamp(0, 3) as i32,
                "alert.vibrate" => p.vibrate = bv,
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
                "calendar.week_view" => p.calendar_week_view = bv,
                "ui.large_text" => p.large_text = bv,
                "ui.keep_screen_on" => p.keep_screen_on = bv,
                "ui.confirm_finish" => p.confirm_finish = bv,
                _ => {}
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
            ("calendar.week_view", b(self.calendar_week_view)),
            ("ui.large_text", b(self.large_text)),
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
        p.save(&db).unwrap();
        assert_eq!(Prefs::load(&db).unwrap(), p);
    }
}
