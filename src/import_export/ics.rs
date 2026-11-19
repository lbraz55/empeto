//! Exportação de calendário em iCalendar (.ics) — recomendação provisória do documento (7.1):
//! calendário interno + exportação ICS, sem permissões READ/WRITE_CALENDAR.
//! O arquivo é uma cópia dos eventos; não há sincronização contínua (7.3).

use crate::calendar::{planned_on, today};
use crate::domain::*;
use crate::persistence::Db;
use chrono::{Duration, TimeZone, Utc};
use std::collections::HashMap;

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

fn utc_stamp(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().map(|d| d.format("%Y%m%dT%H%M%SZ").to_string()).unwrap_or_default()
}

/// Gera ICS com sessões realizadas e treinos planejados (dias da semana) para as próximas `weeks` semanas.
pub fn build_ics(db: &Db, weeks: i64) -> AppResult<String> {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//EMPETO//Treinos//PT-BR\r\nCALSCALE:GREGORIAN\r\n",
    );
    let stamp = utc_stamp(now_ms());
    for s in db.list_session_summaries(None, None)? {
        let end = s.ended_at.unwrap_or(s.started_at + 3_600_000);
        out.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:session-{}@empeto.local\r\nDTSTAMP:{stamp}\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:{}\r\nDESCRIPTION:{}\r\nEND:VEVENT\r\n",
            s.id,
            utc_stamp(s.started_at),
            utc_stamp(end),
            esc(&format!("Treino realizado: {} — {}", s.plan_name, s.schedule_name)),
            esc(&format!("{} exercícios, {} séries", s.exercises, s.sets)),
        ));
    }
    let plans = db.list_plans_full()?;
    let t = today();
    for i in 0..(weeks * 7) {
        let d = t + Duration::days(i);
        for p in planned_on(d, t, &plans, &HashMap::new(), &[]) {
            out.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:plan-{}-{}-{}@empeto.local\r\nDTSTAMP:{stamp}\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nSUMMARY:{}\r\nEND:VEVENT\r\n",
                p.plan_id,
                p.day_id,
                d.format("%Y%m%d"),
                d.format("%Y%m%d"),
                (d + Duration::days(1)).format("%Y%m%d"),
                esc(&format!("Treino planejado: {} — {}", p.plan_name, p.day_name)),
            ));
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    Ok(out)
}
