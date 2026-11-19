//! Validação do teclado numérico próprio (documento EMPETO, 3.4).
//! O teclado só edita o texto de rascunho; nada é gravado antes da confirmação.

use super::{format_load, parse_load, LoadUnit, MAX_LOAD_G};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Carga: decimal com vírgula, até 2 casas, 0..2000 kg (ou equivalente em lb).
    Load,
    /// Repetições: inteiro 0..1000.
    Reps,
}

const MAX_DECIMALS: usize = 2;
const MAX_REPS: i64 = 1000;

/// Aplica uma tecla ao texto atual. Teclas: "0".."9", "," (vírgula decimal), "back", "clear".
/// Teclas que produziriam valor inválido são ignoradas (o texto anterior é mantido).
pub fn apply_key(current: &str, key: &str, kind: FieldKind, unit: LoadUnit) -> String {
    let cur = current.trim().replace('.', ",");
    let candidate = match key {
        "back" => {
            let mut c = cur.clone();
            c.pop();
            return c;
        }
        "clear" => return String::new(),
        "," | "." => {
            if kind == FieldKind::Reps || cur.contains(',') {
                return cur;
            }
            if cur.is_empty() { "0,".to_string() } else { format!("{cur},") }
        }
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
            if cur == "0" { d.to_string() } else { format!("{cur}{d}") }
        }
        _ => return cur,
    };
    if is_valid(&candidate, kind, unit) { candidate } else { cur }
}

/// O texto (possivelmente incompleto, ex.: "42,") representa um valor aceitável?
pub fn is_valid(text: &str, kind: FieldKind, unit: LoadUnit) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    match kind {
        FieldKind::Reps => t.chars().all(|c| c.is_ascii_digit()) && t.parse::<i64>().map(|v| v <= MAX_REPS).unwrap_or(false),
        FieldKind::Load => {
            let mut parts = t.split(',');
            let int = parts.next().unwrap_or("");
            let dec = parts.next();
            if parts.next().is_some() || int.is_empty() || !int.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            if let Some(d) = dec {
                if d.len() > MAX_DECIMALS || !d.chars().all(|c| c.is_ascii_digit()) {
                    return false;
                }
            }
            let complete = t.trim_end_matches(',');
            parse_load(complete, unit).map(|g| g <= MAX_LOAD_G).unwrap_or(false)
        }
    }
}

/// Incremento/decremento: carga pelo passo configurado (gramas), repetições de 1 em 1.
pub fn step(current: &str, dir: i32, kind: FieldKind, unit: LoadUnit, load_step_g: i64) -> String {
    match kind {
        FieldKind::Reps => {
            let v: i64 = current.trim().parse().unwrap_or(0);
            (v + dir as i64).clamp(0, MAX_REPS).to_string()
        }
        FieldKind::Load => {
            let g = parse_load(current.trim().trim_end_matches(','), unit).unwrap_or(0);
            format_load((g + dir as i64 * load_step_g).clamp(0, MAX_LOAD_G), unit)
        }
    }
}

/// Converte o rascunho para os tipos do banco (gramas, repetições). `None` = campo vazio.
pub fn to_values(load: &str, reps: &str, unit: LoadUnit) -> Result<(i64, i64), String> {
    let load_g = parse_load(load.trim().trim_end_matches(','), unit).map_err(|e| e.to_string())?;
    let reps = reps.trim();
    if reps.is_empty() {
        return Err("Informe as repetições realizadas.".into());
    }
    let r: i64 = reps.parse().map_err(|_| "Repetições inválidas.".to_string())?;
    if !(0..=MAX_REPS).contains(&r) {
        return Err("Repetições devem estar entre 0 e 1000.".into());
    }
    Ok((load_g, r))
}

#[cfg(test)]
mod tests {
    use super::*;
    const KG: LoadUnit = LoadUnit::Kg;

    fn typed(keys: &[&str], kind: FieldKind) -> String {
        keys.iter().fold(String::new(), |acc, k| apply_key(&acc, k, kind, KG))
    }

    #[test]
    fn load_keys() {
        assert_eq!(typed(&["4", "2", ",", "5"], FieldKind::Load), "42,5");
        assert_eq!(typed(&[",", "5"], FieldKind::Load), "0,5");
        assert_eq!(typed(&["1", ",", "2", "5", "7"], FieldKind::Load), "1,25"); // máx. 2 casas
        assert_eq!(typed(&["1", ",", ","], FieldKind::Load), "1,"); // uma vírgula só
        assert_eq!(typed(&["0", "7"], FieldKind::Load), "7"); // sem zero à esquerda
        assert_eq!(typed(&["2", "0", "0", "0", "1"], FieldKind::Load), "2000"); // limite do domínio
        assert_eq!(apply_key("42,5", "back", FieldKind::Load, KG), "42,");
        assert_eq!(apply_key("42,5", "clear", FieldKind::Load, KG), "");
        assert_eq!(apply_key("4", "x", FieldKind::Load, KG), "4");
    }

    #[test]
    fn reps_keys() {
        assert_eq!(typed(&["1", "2"], FieldKind::Reps), "12");
        assert_eq!(typed(&["1", ","], FieldKind::Reps), "1"); // sem decimal
        assert_eq!(typed(&["1", "0", "0", "0", "0"], FieldKind::Reps), "1000");
    }

    #[test]
    fn steps_and_conversion() {
        assert_eq!(step("40", 1, FieldKind::Load, KG, 2_500), "42,5");
        assert_eq!(step("1", -1, FieldKind::Load, KG, 2_500), "0");
        assert_eq!(step("", 1, FieldKind::Reps, KG, 0), "1");
        assert_eq!(to_values("42,", "8", KG).unwrap(), (42_000, 8));
        assert!(to_values("40", "", KG).is_err());
        assert!(is_valid("", FieldKind::Load, KG));
        assert!(!is_valid("1,234", FieldKind::Load, KG));
    }
}
