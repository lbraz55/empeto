//! Calendário interno (RF-13): datas, treinos planejados e realizados.
//! Datas são gravadas em UTC (epoch ms) e convertidas para o fuso local aqui.

use crate::domain::*;
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use std::collections::HashMap;

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn local_date_of(ms: i64) -> NaiveDate {
    Local.timestamp_millis_opt(ms).single().map(|d| d.date_naive()).unwrap_or_else(today)
}

pub fn local_time_of(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

/// Início (inclusive) e fim (exclusivo) do dia local em epoch ms.
pub fn day_bounds_ms(d: NaiveDate) -> (i64, i64) {
    let start = local_midnight_ms(d);
    let end = local_midnight_ms(d + Duration::days(1));
    (start, end)
}

fn local_midnight_ms(d: NaiveDate) -> i64 {
    let naive = d.and_hms_opt(0, 0, 0).unwrap();
    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(t) => t.timestamp_millis(),
        chrono::LocalResult::Ambiguous(a, _) => a.timestamp_millis(),
        // Horário de verão pode "pular" a meia-noite: usa 01:00.
        chrono::LocalResult::None => Local
            .from_local_datetime(&d.and_hms_opt(1, 0, 0).unwrap())
            .earliest()
            .map(|t| t.timestamp_millis())
            .unwrap_or(0),
    }
}

/// Índice do dia da semana com segunda = 0.
pub fn weekday_index(d: NaiveDate) -> u32 {
    d.weekday().num_days_from_monday()
}

pub fn week_start(d: NaiveDate, starts_monday: bool) -> NaiveDate {
    let offset = if starts_monday { d.weekday().num_days_from_monday() } else { d.weekday().num_days_from_sunday() };
    d - Duration::days(offset as i64)
}

pub fn week_dates(d: NaiveDate, starts_monday: bool) -> Vec<NaiveDate> {
    let s = week_start(d, starts_monday);
    (0..7).map(|i| s + Duration::days(i)).collect()
}

/// 6 semanas × 7 dias cobrindo o mês.
pub fn month_grid(year: i32, month: u32, starts_monday: bool) -> Vec<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_else(today);
    let s = week_start(first, starts_monday);
    (0..42).map(|i| s + Duration::days(i)).collect()
}

pub const MONTHS: [&str; 12] = [
    "Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro",
    "Dezembro",
];

pub fn month_title(year: i32, month: u32) -> String {
    trf!("{} de {}", crate::i18n::tr(MONTHS[(month as usize + 11) % 12]), year)
}

pub fn date_long(d: NaiveDate) -> String {
    trf!(
        "{}, {} de {} de {}",
        crate::i18n::tr(WEEKDAY_LONG[weekday_index(d) as usize]),
        d.day(),
        crate::i18n::lower_name(crate::i18n::tr(MONTHS[d.month0() as usize])),
        d.year()
    )
}

pub fn date_short(d: NaiveDate) -> String {
    format!("{:02}/{:02}/{}", d.day(), d.month(), d.year())
}

pub fn date_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_date_key(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn add_months(d: NaiveDate, delta: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + delta;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(d)
}

/// Treino planejado em uma data (não é treino realizado — RN-02).
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedWorkout {
    pub plan_id: Id,
    pub plan_name: String,
    pub day_id: Id,
    pub day_name: String,
    pub exercises: usize,
    /// true quando vem de uma ficha em sequência (próximo da fila).
    pub from_sequence: bool,
}

/// Calcula os treinos planejados para `date`.
/// - Fichas por dia da semana: dias cujo bitmask contém o dia.
/// - Fichas em sequência: o próximo dia da sequência aparece em `today` (e não em outras datas,
///   pois a data real depende de quando o usuário treinar).
pub fn planned_on(
    date: NaiveDate,
    today: NaiveDate,
    plans: &[PlanFull],
    next_seq: &HashMap<Id, Id>,
    done_today_plans: &[Id],
) -> Vec<PlannedWorkout> {
    let mut out = Vec::new();
    let bit = weekday_bit(weekday_index(date));
    for p in plans.iter().filter(|p| p.plan.active) {
        match p.plan.schedule_type {
            ScheduleType::Weekday => {
                for d in p.days.iter().filter(|d| d.day.weekdays & bit != 0) {
                    out.push(PlannedWorkout {
                        plan_id: p.plan.id,
                        plan_name: p.plan.name.clone(),
                        day_id: d.day.id,
                        day_name: d.day.name.clone(),
                        exercises: d.exercises.len(),
                        from_sequence: false,
                    });
                }
            }
            ScheduleType::Sequence => {
                if date == today && !done_today_plans.contains(&p.plan.id) {
                    if let Some(day_id) = next_seq.get(&p.plan.id) {
                        if let Some(d) = p.days.iter().find(|d| d.day.id == *day_id) {
                            out.push(PlannedWorkout {
                                plan_id: p.plan.id,
                                plan_name: p.plan.name.clone(),
                                day_id: d.day.id,
                                day_name: d.day.name.clone(),
                                exercises: d.exercises.len(),
                                from_sequence: true,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_has_42_days_starting_on_monday() {
        let g = month_grid(2026, 11, true);
        assert_eq!(g.len(), 42);
        assert_eq!(weekday_index(g[0]), 0);
        assert!(g.contains(&NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()));
        let g = month_grid(2026, 11, false);
        assert_eq!(g[0].weekday(), chrono::Weekday::Sun);
    }

    #[test]
    fn add_months_wraps() {
        let d = NaiveDate::from_ymd_opt(2026, 12, 15).unwrap();
        assert_eq!(add_months(d, 1), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(add_months(d, -12), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
    }

    #[test]
    fn day_bounds_cover_24h_normally() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let (a, b) = day_bounds_ms(d);
        assert!(b > a);
        assert_eq!(local_date_of(a), d);
        assert_eq!(local_date_of(b - 1), d);
    }
}
