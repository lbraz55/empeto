//! Widget Android (documento EMPETO, 9.4).
//!
//! O widget não lê o banco: o app gera um resumo derivado (arquivo `widget.txt` no diretório
//! privado) sempre que a Home é atualizada, e o `AppWidgetProvider` em Java só exibe esse texto.
//! Assim não existe uma segunda fonte de dados — o banco continua sendo a única verdade.

use std::path::Path;

pub const WIDGET_FILE: &str = "widget.txt";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WidgetSummary {
    pub title: String,
    pub line1: String,
    pub line2: String,
}

fn clean(s: &str) -> String {
    s.replace(['\n', '\r'], " ").chars().take(120).collect()
}

pub fn render(s: &WidgetSummary) -> String {
    format!("title={}\nline1={}\nline2={}\n", clean(&s.title), clean(&s.line1), clean(&s.line2))
}

pub fn parse(text: &str) -> WidgetSummary {
    let mut s = WidgetSummary::default();
    for l in text.lines() {
        if let Some((k, v)) = l.split_once('=') {
            match k {
                "title" => s.title = v.to_string(),
                "line1" => s.line1 = v.to_string(),
                "line2" => s.line2 = v.to_string(),
                _ => {}
            }
        }
    }
    s
}

/// Grava o resumo de forma atômica; retorna `true` se o conteúdo mudou.
pub fn write(data_dir: &Path, s: &WidgetSummary) -> bool {
    let path = data_dir.join(WIDGET_FILE);
    let text = render(s);
    if std::fs::read_to_string(&path).map(|t| t == text).unwrap_or(false) {
        return false;
    }
    let tmp = data_dir.join(".widget.tmp");
    std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, &path)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_strips_newlines() {
        let s = WidgetSummary { title: "TREINO DE HOJE".into(), line1: "ABC — A\nPeito".into(), line2: "5 exercícios".into() };
        let p = parse(&render(&s));
        assert_eq!(p.line1, "ABC — A Peito");
        assert_eq!(p.title, s.title);
        let dir = std::env::temp_dir().join(format!("empeto-widget-{}", crate::domain::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(write(&dir, &s));
        assert!(!write(&dir, &s));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
