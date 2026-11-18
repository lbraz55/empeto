//! Formatação de textos para a interface (pt-BR).

use crate::calendar::{local_date_of, local_time_of, weekday_index};
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
        format!("{}h{:02}", m / 60, m % 60)
    } else {
        format!("{m} min")
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
        WEEKDAY_SHORT[weekday_index(d) as usize].to_lowercase(),
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
            out.push('.');
        }
        out.push(ch);
    }
    if v < 0 {
        format!("-{out}")
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

/// Caminhos SVG (viewbox 1000×1000) para o gráfico de linha e os pontos.
pub fn chart_paths(values: &[i64]) -> (String, String) {
    if values.is_empty() {
        return (String::new(), String::new());
    }
    let mn = *values.iter().min().unwrap() as f64;
    let mx = *values.iter().max().unwrap() as f64;
    let span = if mx > mn { mx - mn } else { 1.0 };
    let n = values.len();
    let pts: Vec<(f64, f64)> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = if n == 1 { 500.0 } else { 20.0 + i as f64 * 960.0 / (n - 1) as f64 };
            let y = if mx > mn { 950.0 - (*v as f64 - mn) / span * 900.0 } else { 500.0 };
            (x, y)
        })
        .collect();
    let line = if n >= 2 {
        let mut s = format!("M {:.1} {:.1}", pts[0].0, pts[0].1);
        for p in &pts[1..] {
            s.push_str(&format!(" L {:.1} {:.1}", p.0, p.1));
        }
        s
    } else {
        String::new()
    };
    let dots = pts.iter().map(|p| format!("M {:.1} {:.1} L {:.1} {:.1}", p.0, p.1, p.0 + 0.5, p.1)).collect::<Vec<_>>().join(" ");
    (line, dots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints_and_paths() {
        assert_eq!(fmt_int(1234567), "1.234.567");
        assert_eq!(fmt_int(999), "999");
        let (l, d) = chart_paths(&[1, 2, 3]);
        assert!(l.starts_with("M 20.0 950.0"));
        assert_eq!(d.matches('M').count(), 3);
        assert_eq!(fmt_minutes(75 * 60_000), "1h15");
    }
}
