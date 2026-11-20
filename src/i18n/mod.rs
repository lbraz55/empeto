//! Localização centralizada (documento de evolução v1.1, 4.1).
//!
//! - O texto-fonte é o português do Brasil; os catálogos ficam em `resources/i18n/<idioma>/LC_MESSAGES/workout.po`.
//! - Os mesmos arquivos `.po` são usados pela interface Slint (`@tr`, embutidos na compilação) e pelo
//!   Rust (`tr` / `trf!`), então cada frase é traduzida num único lugar.
//! - Dados do usuário (nomes de exercícios, fichas, anotações) nunca passam por aqui.
//! - Frase sem tradução cai no português (o teste `tests/i18n.rs` exige cobertura completa).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Pt,
    En,
    Es,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Pt => "pt-BR",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }
    /// Nome da pasta do catálogo Slint (vazio = texto-fonte em português).
    pub fn slint_code(self) -> &'static str {
        match self {
            Lang::Pt => "",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }
    /// Nome do idioma escrito nele mesmo (para a lista de escolha).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Pt => "Português (Brasil)",
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }
    /// Idioma automático a partir do código do sistema ("pt_BR", "es-AR", "en-US", "de"...).
    /// Português e espanhol são reconhecidos; qualquer outro idioma usa inglês.
    pub fn from_system(locale: &str) -> Lang {
        let l = locale.to_ascii_lowercase();
        if l.starts_with("pt") {
            Lang::Pt
        } else if l.starts_with("es") {
            Lang::Es
        } else if l.is_empty() {
            Lang::Pt
        } else {
            Lang::En
        }
    }
    pub fn decimal_sep(self) -> char {
        match self {
            Lang::En => '.',
            _ => ',',
        }
    }
    pub const ALL: [Lang; 3] = [Lang::Pt, Lang::En, Lang::Es];
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    CURRENT.store(
        match l {
            Lang::Pt => 0,
            Lang::En => 1,
            Lang::Es => 2,
        },
        Ordering::Relaxed,
    );
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Es,
        _ => Lang::Pt,
    }
}

/// Nomes de mês/dia no meio da frase: minúsculos em português e espanhol, maiúsculos em inglês.
pub fn lower_name(s: &str) -> String {
    if lang() == Lang::En { s.to_string() } else { s.to_lowercase() }
}

pub fn decimal_sep() -> char {
    lang().decimal_sep()
}

const EN_PO: &str = include_str!("../../resources/i18n/en/LC_MESSAGES/workout.po");
const ES_PO: &str = include_str!("../../resources/i18n/es/LC_MESSAGES/workout.po");

type Catalog = HashMap<&'static str, &'static str>;

fn catalog(l: Lang) -> Option<&'static Catalog> {
    static EN: OnceLock<Catalog> = OnceLock::new();
    static ES: OnceLock<Catalog> = OnceLock::new();
    match l {
        Lang::Pt => None,
        Lang::En => Some(EN.get_or_init(|| parse_po(EN_PO))),
        Lang::Es => Some(ES.get_or_init(|| parse_po(ES_PO))),
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o)
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Leitor mínimo de `.po`: `msgid`/`msgstr` com continuação em várias linhas. Entradas com
/// `msgstr` vazio são ignoradas (caem no português).
pub fn parse_po(text: &'static str) -> Catalog {
    let mut map = Catalog::new();
    let mut id: Option<String> = None;
    let mut cur_id = String::new();
    let mut cur_str = String::new();
    let mut in_str = false;
    let flush = |id: &mut Option<String>, s: &mut String, map: &mut Catalog| {
        if let Some(k) = id.take() {
            if !k.is_empty() && !s.is_empty() {
                let k: &'static str = Box::leak(k.into_boxed_str());
                let v: &'static str = Box::leak(std::mem::take(s).into_boxed_str());
                map.insert(k, v);
            }
        }
        s.clear();
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let quoted = |l: &str| -> Option<String> {
            let a = l.find('"')?;
            let b = l.rfind('"')?;
            if b > a { Some(unescape(&l[a + 1..b])) } else { None }
        };
        if line.starts_with("msgid ") {
            flush(&mut id, &mut cur_str, &mut map);
            cur_id = quoted(line).unwrap_or_default();
            in_str = false;
        } else if line.starts_with("msgstr ") {
            id = Some(std::mem::take(&mut cur_id));
            cur_str = quoted(line).unwrap_or_default();
            in_str = true;
        } else if line.starts_with('"') {
            let q = quoted(line).unwrap_or_default();
            if in_str {
                cur_str.push_str(&q);
            } else {
                cur_id.push_str(&q);
            }
        }
    }
    flush(&mut id, &mut cur_str, &mut map);
    map
}

/// Traduz uma frase da interface para o idioma atual.
pub fn tr(s: &'static str) -> &'static str {
    match catalog(lang()) {
        Some(c) => c.get(s).copied().unwrap_or(s),
        None => s,
    }
}

/// Indica se a frase tem tradução no idioma informado (usado pelos testes de cobertura).
pub fn has_translation(l: Lang, s: &str) -> bool {
    match catalog(l) {
        Some(c) => c.contains_key(s),
        None => true,
    }
}

/// Argumento de `trf!`: formatação em tempo de execução com as especificações usadas no app
/// (`{}`, `{:02}`, `{:.1}`, `{:>3}`...).
pub trait FmtArg {
    fn fmt_spec(&self, spec: &str) -> String;
}

fn pad(s: String, spec: &str) -> String {
    let (zero, rest) = match spec.strip_prefix('0') {
        Some(r) if !r.is_empty() => (true, r),
        _ => (false, spec),
    };
    let rest = rest.trim_start_matches(['>', '<']);
    match rest.parse::<usize>() {
        Ok(w) if s.chars().count() < w => {
            let fill = if zero { '0' } else { ' ' };
            let n = w - s.chars().count();
            if zero && s.starts_with('-') {
                format!("-{}{}", fill.to_string().repeat(n), &s[1..])
            } else {
                format!("{}{}", fill.to_string().repeat(n), s)
            }
        }
        _ => s,
    }
}

macro_rules! fmt_int {
    ($($t:ty),*) => {$(
        impl FmtArg for $t {
            fn fmt_spec(&self, spec: &str) -> String { pad(self.to_string(), spec) }
        }
    )*};
}
fmt_int!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

macro_rules! fmt_float {
    ($($t:ty),*) => {$(
        impl FmtArg for $t {
            fn fmt_spec(&self, spec: &str) -> String {
                let (width, prec) = match spec.split_once('.') {
                    Some((w, p)) => (w, p.parse::<usize>().ok()),
                    None => (spec, None),
                };
                let s = match prec { Some(p) => format!("{:.*}", p, self), None => self.to_string() };
                pad(s, width)
            }
        }
    )*};
}
fmt_float!(f32, f64);

impl FmtArg for str {
    fn fmt_spec(&self, spec: &str) -> String {
        pad(self.to_string(), spec)
    }
}
impl FmtArg for String {
    fn fmt_spec(&self, spec: &str) -> String {
        pad(self.clone(), spec)
    }
}
impl FmtArg for char {
    fn fmt_spec(&self, spec: &str) -> String {
        pad(self.to_string(), spec)
    }
}
impl FmtArg for bool {
    fn fmt_spec(&self, spec: &str) -> String {
        pad(self.to_string(), spec)
    }
}
impl FmtArg for slint::SharedString {
    fn fmt_spec(&self, spec: &str) -> String {
        pad(self.to_string(), spec)
    }
}
impl<T: FmtArg + ?Sized> FmtArg for &T {
    fn fmt_spec(&self, spec: &str) -> String {
        (**self).fmt_spec(spec)
    }
}
impl<T: FmtArg> FmtArg for Option<T> {
    fn fmt_spec(&self, spec: &str) -> String {
        match self {
            Some(v) => v.fmt_spec(spec),
            None => String::new(),
        }
    }
}

/// Substitui os marcadores `{…}` do modelo (já traduzido) pelos argumentos, na ordem.
/// `{{` e `}}` produzem chaves literais.
pub fn format_template(template: &str, args: &[&dyn FmtArg]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars().peekable();
    let mut i = 0;
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut spec = String::new();
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                    spec.push(d);
                }
                let spec = spec.strip_prefix(':').unwrap_or(&spec).to_string();
                if let Some(a) = args.get(i) {
                    out.push_str(&a.fmt_spec(&spec));
                }
                i += 1;
            }
            _ => out.push(c),
        }
    }
    out
}

/// `trf!("Série {} de {}", a, b)`: traduz o modelo e formata os argumentos em ordem.
#[macro_export]
macro_rules! trf {
    ($fmt:literal $(,)?) => {
        $crate::i18n::format_template($crate::i18n::tr($fmt), &[])
    };
    ($fmt:literal, $($arg:expr),+ $(,)?) => {
        $crate::i18n::format_template($crate::i18n::tr($fmt), &[$(&$arg as &dyn $crate::i18n::FmtArg),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_specs() {
        assert_eq!(format_template("{:02}/{:02} {} {:.1}", &[&3, &11, &"x", &2.25f64]), "03/11 x 2.2");
        assert_eq!(format_template("{{}} {}", &[&5]), "{} 5");
        assert_eq!(Lang::from_system("pt_BR"), Lang::Pt);
        assert_eq!(Lang::from_system("es-AR"), Lang::Es);
        assert_eq!(Lang::from_system("de-DE"), Lang::En);
    }

    #[test]
    fn po_parser() {
        let po: &'static str = "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n\nmsgid \"Olá\\n\"\n\"mundo\"\nmsgstr \"Hello\\n\"\n\"world\"\n\nmsgid \"Vazio\"\nmsgstr \"\"\n";
        let c = parse_po(po);
        assert_eq!(c.get("Olá\nmundo"), Some(&"Hello\nworld"));
        assert!(!c.contains_key("Vazio"));
    }
}

impl FmtArg for crate::domain::AppError {
    fn fmt_spec(&self, spec: &str) -> String {
        pad(self.to_string(), spec)
    }
}
