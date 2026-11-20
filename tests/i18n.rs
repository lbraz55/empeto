//! Localização (documento de evolução v1.1, 4.1): catálogos completos e coerentes.
//! A cobertura de frases (todo texto da interface tem tradução) é verificada por
//! `scripts/i18n.py check`, executado no CI.

use workout::i18n::{format_template, has_translation, parse_po, Lang};

fn placeholders(s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '{' {
            if it.peek() == Some(&'{') {
                it.next();
                continue;
            }
            let mut spec = String::new();
            for d in it.by_ref() {
                if d == '}' {
                    break;
                }
                spec.push(d);
            }
            v.push(spec);
        }
    }
    v
}

fn catalog(lang: &str) -> std::collections::HashMap<&'static str, &'static str> {
    let path = format!("{}/resources/i18n/{lang}/LC_MESSAGES/workout.po", env!("CARGO_MANIFEST_DIR"));
    let text: &'static str = Box::leak(std::fs::read_to_string(path).unwrap().into_boxed_str());
    parse_po(text)
}

#[test]
fn translations_keep_placeholders() {
    for lang in ["en", "es"] {
        let c = catalog(lang);
        assert!(c.len() > 500, "catálogo {lang} incompleto");
        for (k, v) in &c {
            assert_eq!(placeholders(k), placeholders(v), "[{lang}] marcadores diferentes em {k:?} → {v:?}");
            assert!(!v.trim().is_empty() || k.trim().is_empty(), "[{lang}] tradução vazia: {k:?}");
        }
    }
}

#[test]
fn core_phrases_translated_and_formatting_works() {
    for l in [Lang::En, Lang::Es] {
        for k in ["Treinos", "Histórico", "Registrar série", "Configurações", "Sessão interrompida"] {
            assert!(has_translation(l, k), "{k} sem tradução em {}", l.code());
        }
    }
    let en = catalog("en");
    let t = en.get("{} de {} séries · iniciada às {}").unwrap();
    assert_eq!(format_template(t, &[&3, &10, &"07:30"]), "3 of 10 sets · started at 07:30");
}
