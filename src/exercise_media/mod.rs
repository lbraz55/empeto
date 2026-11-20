//! Mídia dos exercícios (documento de evolução v1.1, 5.1 e 5.2) — módulo `exercise_media`.
//!
//! - **Ilustrações**: desenhos próprios, monocromáticos, em `assets/exercises/<id>.svg`, embutidos no app
//!   (funcionam offline e seguem o tema por recoloração). Cada exercício guarda um id estável; quando o
//!   id está vazio, a ilustração é escolhida pelo padrão de movimento; sem correspondência, usa `generic`.
//! - **Vídeos**: apenas um link externo opcional (https) informado pelo usuário, aberto no app/navegador
//!   do aparelho. O EMPETO não baixa nem redistribui vídeos e nunca depende deles para funcionar.
//! - Futuro (documento 5.2, alternativa C): pacotes opcionais entrariam como outra `MediaSource`
//!   sem mudar quem consome este módulo.

use crate::domain::*;
use crate::i18n::tr;

/// Ilustrações disponíveis: (id estável, nome exibido).
pub const ILLUSTRATIONS: &[(&str, &str)] = &[
    ("bench-press", "Supino"),
    ("push-up", "Flexão de braços"),
    ("fly", "Crucifixo"),
    ("pull-up", "Barra fixa"),
    ("pulldown", "Puxada"),
    ("row", "Remada"),
    ("overhead-press", "Desenvolvimento"),
    ("dip", "Mergulho"),
    ("lateral-raise", "Elevação lateral"),
    ("curl", "Rosca"),
    ("triceps-extension", "Extensão de tríceps"),
    ("squat", "Agachamento"),
    ("leg-press", "Leg press"),
    ("leg-extension", "Cadeira extensora"),
    ("lunge", "Afundo"),
    ("deadlift", "Levantamento terra"),
    ("leg-curl", "Flexora"),
    ("hip-thrust", "Elevação pélvica"),
    ("hip-abduction", "Abdução de quadril"),
    ("calf-raise", "Panturrilha"),
    ("plank", "Prancha"),
    ("crunch", "Abdominal"),
    ("leg-raise", "Elevação de pernas"),
    ("cardio-run", "Corrida"),
    ("cardio-bike", "Bicicleta"),
    ("generic", "Genérica"),
];

pub const GENERIC: &str = "generic";

pub fn is_known(id: &str) -> bool {
    ILLUSTRATIONS.iter().any(|(k, _)| *k == id)
}

pub fn label(id: &str) -> &'static str {
    ILLUSTRATIONS.iter().find(|(k, _)| *k == id).map(|(_, l)| tr(l)).unwrap_or_else(|| tr("Automática"))
}

/// Sugere uma ilustração a partir do padrão de movimento e do nome (exercícios personalizados).
pub fn guess(pattern: &str, name: &str) -> &'static str {
    let p = normalize(pattern);
    let n = normalize(name);
    let has = |s: &str, k: &str| s.contains(k);
    if has(&n, "supino") || has(&n, "bench") {
        return "bench-press";
    }
    if has(&n, "flexao de bracos") || has(&n, "push") {
        return "push-up";
    }
    if has(&n, "barra fixa") || has(&n, "pull-up") || has(&n, "chin") {
        return "pull-up";
    }
    if has(&n, "terra") || has(&n, "stiff") || has(&n, "deadlift") {
        return "deadlift";
    }
    if has(&n, "leg press") {
        return "leg-press";
    }
    if has(&n, "panturrilha") || has(&n, "calf") {
        return "calf-raise";
    }
    if has(&n, "rosca") || has(&n, "curl") && !has(&n, "leg") {
        return "curl";
    }
    if has(&n, "esteira") || has(&n, "corrida") {
        return "cardio-run";
    }
    if has(&n, "bicicleta") || has(&n, "bike") {
        return "cardio-bike";
    }
    match () {
        _ if has(&p, "empurrar horizontal") => "bench-press",
        _ if has(&p, "empurrar vertical") => "overhead-press",
        _ if has(&p, "aducao horizontal") => "fly",
        _ if has(&p, "puxar vertical") || has(&p, "extensao de ombro") => "pulldown",
        _ if has(&p, "puxar horizontal") => "row",
        _ if has(&p, "abducao de ombro") || has(&p, "abducao horizontal") => "lateral-raise",
        _ if has(&p, "flexao de cotovelo") => "curl",
        _ if has(&p, "extensao de cotovelo") => "triceps-extension",
        _ if has(&p, "agachar") => "squat",
        _ if has(&p, "avanco") => "lunge",
        _ if has(&p, "dobradica") => "deadlift",
        _ if has(&p, "extensao de joelho") => "leg-extension",
        _ if has(&p, "flexao de joelho") => "leg-curl",
        _ if has(&p, "extensao de quadril") => "hip-thrust",
        _ if has(&p, "abducao de quadril") => "hip-abduction",
        _ if has(&p, "flexao plantar") => "calf-raise",
        _ if has(&p, "core") => "crunch",
        _ if has(&p, "cardio") => "cardio-run",
        _ => GENERIC,
    }
}

/// Ilustração efetiva: a escolhida, ou a sugerida, ou a genérica.
pub fn resolve(e: &Exercise) -> &'static str {
    if let Some((k, _)) = ILLUSTRATIONS.iter().find(|(k, _)| *k == e.illustration) {
        return k;
    }
    guess(&e.movement_pattern, &e.name)
}

/// Valida o link de vídeo informado pelo usuário. Vazio é permitido (sem vídeo).
pub fn validate_video_url(url: &str) -> AppResult<String> {
    let u = url.trim();
    if u.is_empty() {
        return Ok(String::new());
    }
    if u.len() > 500 || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return validation(tr("Link de vídeo inválido."));
    }
    let lower = u.to_ascii_lowercase();
    if !lower.starts_with("https://") || lower.len() < "https://a.b".len() || !u[8..].contains('.') {
        return validation(tr("Use um link que comece com https://"));
    }
    Ok(u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_and_resolve() {
        assert_eq!(guess("Empurrar horizontal", "Supino declinado"), "bench-press");
        assert_eq!(guess("Agachar", "Agachamento sumô"), "squat");
        assert_eq!(guess("", "Rosca Scott"), "curl");
        assert_eq!(guess("", "Algo novo"), GENERIC);
        let mut e = Exercise { name: "X".into(), movement_pattern: "Avanço".into(), ..Default::default() };
        assert_eq!(resolve(&e), "lunge");
        e.illustration = "plank".into();
        assert_eq!(resolve(&e), "plank");
        e.illustration = "inexistente".into();
        assert_eq!(resolve(&e), "lunge");
        for (id, _) in ILLUSTRATIONS {
            let path = format!("{}/assets/exercises/{id}.svg", env!("CARGO_MANIFEST_DIR"));
            assert!(std::path::Path::new(&path).exists(), "falta {path}");
        }
    }

    #[test]
    fn video_links() {
        assert_eq!(validate_video_url("  ").unwrap(), "");
        assert!(validate_video_url("https://www.youtube.com/watch?v=abc").is_ok());
        assert!(validate_video_url("http://exemplo.com/v").is_err());
        assert!(validate_video_url("javascript:alert(1)").is_err());
        assert!(validate_video_url("https://com").is_err());
        assert!(validate_video_url("https://a.com/ x").is_err());
    }
}
