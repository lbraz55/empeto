//! Formatação de textos para a interface (pt-BR).

use crate::calendar::{local_date_of, local_time_of, weekday_index};
use crate::i18n::tr;
use crate::domain::{format_load, LoadUnit, WEEKDAY_SHORT};
use chrono::{Datelike, NaiveDate};

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn fmt_minutes(ms: i64) -> String {
    let m = (ms / 60_000).max(0);
    if m >= 60 {
        trf!("{}h{:02}", m / 60, m % 60)
    } else {
        trf!("{} min", m)
    }
}

pub fn fmt_dm(d: NaiveDate) -> String {
    format!("{:02}/{:02}", d.day(), d.month())
}

pub fn fmt_date_short(ms: i64) -> String {
    let d = local_date_of(ms);
    format!("{:02}/{:02}/{}", d.day(), d.month(), d.year())
}

/// "seg, 18/11/2026 · 07:30"
pub fn fmt_date_time(ms: i64) -> String {
    let d = local_date_of(ms);
    format!(
        "{}, {:02}/{:02}/{} · {}",
        crate::i18n::lower_name(tr(WEEKDAY_SHORT[weekday_index(d) as usize])),
        d.day(),
        d.month(),
        d.year(),
        local_time_of(ms)
    )
}

/// Inteiro com separador de milhar (1.234.567).
pub fn fmt_int(v: i64) -> String {
    let s = v.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(if crate::i18n::decimal_sep() == '.' { ',' } else { '.' });
        }
        out.push(ch);
    }
    if v < 0 {
        format!("-{}", out)
    } else {
        out
    }
}

/// Volume (gramas × reps) na unidade escolhida, arredondado.
pub fn fmt_volume(g_reps: i64, unit: LoadUnit) -> String {
    let v = match unit {
        LoadUnit::Kg => g_reps as f64 / 1000.0,
        LoadUnit::Lb => g_reps as f64 / crate::domain::GRAMS_PER_LB,
    };
    if v < 100.0 {
        format!("{} {}", format_load((v * 1000.0) as i64, LoadUnit::Kg), unit.as_str())
    } else {
        format!("{} {}", fmt_int(v.round() as i64), unit.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints_and_paths() {
        assert_eq!(fmt_int(1234567), "1.234.567");
        assert_eq!(fmt_int(999), "999");
        assert_eq!(fmt_minutes(75 * 60_000), "1h15");
    }
}
