//! Atalhos personalizáveis da Home (documento EMPETO, 7.4).
//!
//! Cada atalho tem um código estável (gravado nas preferências). Regras: no máximo
//! `MAX_SHORTCUTS`, sem duplicados, códigos desconhecidos são ignorados (compatibilidade com
//! versões futuras/antigas) e "restaurar padrão" volta para `DEFAULT_SHORTCUTS`.

pub const MAX_SHORTCUTS: usize = 4;
pub const PREF_KEY: &str = "home.shortcuts";

/// (código estável, rótulo, ícone)
pub const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("next_workout", "Próximo treino", "play"),
    ("timer", "Cronômetro", "timer"),
    ("library", "Exercícios", "dumbbell"),
    ("search", "Buscar", "search"),
    ("evolution", "Evolução", "chart"),
    ("calendar", "Calendário", "calendar"),
    ("history", "Histórico", "history"),
    ("new_plan", "Nova ficha", "plus"),
    ("templates", "Modelos", "template"),
    ("backups", "Backups", "database"),
    ("tools", "Anilhas e aquecimento", "tools"),
    ("wellhub", "WellHub (check-in)", "link"),
];

pub const DEFAULT_SHORTCUTS: &[&str] = &["next_workout", "timer", "library", "evolution"];

pub fn is_known(code: &str) -> bool {
    SHORTCUTS.iter().any(|s| s.0 == code)
}

pub fn label(code: &str) -> &'static str {
    SHORTCUTS.iter().find(|s| s.0 == code).map(|s| crate::i18n::tr(s.1)).unwrap_or("")
}

pub fn icon(code: &str) -> &'static str {
    SHORTCUTS.iter().find(|s| s.0 == code).map(|s| s.2).unwrap_or("")
}

pub fn defaults() -> Vec<String> {
    DEFAULT_SHORTCUTS.iter().map(|s| s.to_string()).collect()
}

/// Normaliza uma lista: remove desconhecidos e duplicados, limita a `MAX_SHORTCUTS`.
pub fn normalize<S: AsRef<str>>(codes: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in codes {
        let c = c.as_ref().trim();
        if is_known(c) && !out.iter().any(|x| x == c) {
            out.push(c.to_string());
        }
        if out.len() == MAX_SHORTCUTS {
            break;
        }
    }
    out
}

/// Lê o valor da preferência. `None` (nunca configurado) → padrão. String vazia → nenhum atalho.
pub fn parse(pref: Option<&str>) -> Vec<String> {
    match pref {
        None => defaults(),
        Some(v) => normalize(&v.split(',').collect::<Vec<_>>()),
    }
}

pub fn serialize(codes: &[String]) -> String {
    normalize(codes).join(",")
}

/// Define o atalho da posição `slot`. `code` vazio remove o atalho. Se o código já está em outra
/// posição, as duas trocam de lugar (nunca há duplicados).
pub fn set_slot(current: &[String], slot: usize, code: &str) -> Vec<String> {
    let mut v: Vec<String> = normalize(current);
    if slot >= MAX_SHORTCUTS {
        return v;
    }
    if code.is_empty() {
        if slot < v.len() {
            v.remove(slot);
        }
        return v;
    }
    if !is_known(code) {
        return v;
    }
    if let Some(pos) = v.iter().position(|x| x == code) {
        if slot < v.len() {
            v.swap(pos, slot);
        }
        return v;
    }
    if slot < v.len() {
        v[slot] = code.to_string();
    } else {
        v.push(code.to_string());
    }
    normalize(&v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_normalize_and_defaults() {
        assert_eq!(parse(None), defaults());
        assert!(parse(Some("")).is_empty());
        assert_eq!(parse(Some("timer,timer,xyz,search")), vec!["timer", "search"]);
        let many = SHORTCUTS.iter().map(|s| s.0).collect::<Vec<_>>().join(",");
        assert_eq!(parse(Some(&many)).len(), MAX_SHORTCUTS);
    }

    #[test]
    fn set_slot_swaps_instead_of_duplicating() {
        let cur = defaults(); // next_workout, timer, library, evolution
        let v = set_slot(&cur, 0, "timer");
        assert_eq!(v, vec!["timer", "next_workout", "library", "evolution"]);
        let v = set_slot(&v, 3, "search");
        assert_eq!(v[3], "search");
        let v = set_slot(&v, 1, "");
        assert_eq!(v.len(), 3);
        let v = set_slot(&v, 3, "backups");
        assert_eq!(v.len(), 4);
        assert_eq!(serialize(&v).split(',').count(), 4);
        assert_eq!(set_slot(&v, 0, "inexistente"), v);
    }
}
