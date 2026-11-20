//! Verificação de atualizações (documento de evolução v1.1, 7.1 e 7.2) — módulo `updater`.
//!
//! Fonte oficial: o arquivo `empeto-update.json` publicado pelo GitHub Actions em cada Release do
//! repositório do projeto. A decisão (há versão nova? qual APK? o link é oficial?) fica aqui, em Rust;
//! a rede, o download, a verificação de SHA-256/assinatura e o instalador ficam na ponte Android
//! (HttpURLConnection, MessageDigest, PackageManager e PackageInstaller — sem bibliotecas novas).
//!
//! Regras: nada é baixado sem o usuário tocar em "Baixar e instalar"; nunca instala de fora do
//! repositório oficial; a instalação passa pela confirmação do Android; os dados locais são preservados
//! porque a atualização mantém o mesmo pacote e a mesma assinatura.

use crate::domain::*;
use crate::i18n::tr;
use serde::Deserialize;

/// Repositório oficial (o GitHub redireciona nomes antigos).
pub const REPO: &str = "lbraz55/empeto";
pub const METADATA_URL: &str = "https://github.com/lbraz55/empeto/releases/latest/download/empeto-update.json";
/// Prefixos aceitos para os APKs baixados (nome atual e nome antigo do mesmo repositório).
pub const ALLOWED_PREFIXES: [&str; 2] =
    ["https://github.com/lbraz55/empeto/releases/download/", "https://github.com/lbraz55/AcademiaLuis/releases/download/"];
/// Verificação automática no máximo uma vez por semana.
pub const AUTO_INTERVAL_MS: i64 = 7 * 86_400_000;
pub const PREF_AUTO: &str = "update.auto_check";
pub const PREF_LAST_CHECK: &str = "update.last_check";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ApkInfo {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub abis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UpdateMetadata {
    pub version: String,
    pub version_code: i64,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub notes: String,
    pub apks: Vec<ApkInfo>,
}

/// "0.8.0" → (0, 8, 0). Partes ausentes ou inválidas viram 0.
pub fn parse_version(v: &str) -> (u64, u64, u64) {
    let mut it = v.trim().trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

pub fn is_newer(available: &str, installed: &str) -> bool {
    parse_version(available) > parse_version(installed)
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn is_official_url(url: &str) -> bool {
    ALLOWED_PREFIXES.iter().any(|p| url.starts_with(p)) && !url.contains("..") && url.ends_with(".apk")
}

/// Lê e valida o arquivo de metadados.
pub fn parse_metadata(json: &str) -> AppResult<UpdateMetadata> {
    let m: UpdateMetadata =
        serde_json::from_str(json).map_err(|_| AppError::File(tr("Informações de atualização inválidas.").into()))?;
    if parse_version(&m.version) == (0, 0, 0) || m.version_code <= 0 {
        return Err(AppError::File(tr("Informações de atualização inválidas.").into()));
    }
    for a in &m.apks {
        if !is_official_url(&a.url) || !is_hex64(&a.sha256) || a.size == 0 || a.size > 200 * 1024 * 1024 {
            return Err(AppError::File(tr("Informações de atualização inválidas.").into()));
        }
    }
    Ok(m)
}

/// Escolhe o APK compatível com as arquiteturas do aparelho (na ordem de preferência do Android).
pub fn pick_apk<'a>(m: &'a UpdateMetadata, device_abis: &[String]) -> Option<&'a ApkInfo> {
    for abi in device_abis {
        if let Some(a) = m.apks.iter().find(|a| a.abis.iter().any(|x| x == abi)) {
            return Some(a);
        }
    }
    // APK sem lista de ABIs declarada é considerado universal.
    m.apks.iter().find(|a| a.abis.is_empty())
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckOutcome {
    UpToDate,
    Available { meta: UpdateMetadata, apk: ApkInfo },
    Incompatible { meta: UpdateMetadata },
}

pub fn evaluate(m: UpdateMetadata, installed_version: &str, installed_code: i64, device_abis: &[String]) -> CheckOutcome {
    if !is_newer(&m.version, installed_version) && m.version_code <= installed_code {
        return CheckOutcome::UpToDate;
    }
    match pick_apk(&m, device_abis).cloned() {
        Some(apk) => CheckOutcome::Available { meta: m, apk },
        None => CheckOutcome::Incompatible { meta: m },
    }
}

/// Verificação automática: só se ligada e se a última foi há 7 dias ou mais.
pub fn auto_check_due(enabled: bool, last_check: Option<i64>, now: i64) -> bool {
    enabled && last_check.map(|t| now - t >= AUTO_INTERVAL_MS || t > now).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(v: &str, code: i64) -> String {
        format!(
            r#"{{"version":"{v}","version_code":{code},"published_at":"2026-11-20","notes":"x",
               "apks":[{{"name":"a.apk","url":"https://github.com/lbraz55/empeto/releases/download/v{v}/a.apk",
                 "size":1000,"sha256":"{}","abis":["arm64-v8a","x86_64"]}}]}}"#,
            "a".repeat(64)
        )
    }

    #[test]
    fn versions() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.9.0"));
        assert!(!is_newer("0.8.0", "0.8.0"));
        assert!(!is_newer("0.7.9", "0.8.0"));
    }

    #[test]
    fn parse_evaluate_and_safety() {
        let m = parse_metadata(&meta("0.9.0", 900)).unwrap();
        let arm = vec!["arm64-v8a".to_string()];
        match evaluate(m.clone(), "0.8.0", 800, &arm) {
            CheckOutcome::Available { apk, .. } => assert_eq!(apk.name, "a.apk"),
            o => panic!("{o:?}"),
        }
        assert_eq!(evaluate(m.clone(), "0.9.0", 900, &arm), CheckOutcome::UpToDate);
        assert!(matches!(evaluate(m, "0.8.0", 800, &["armeabi-v7a".into()]), CheckOutcome::Incompatible { .. }));
        // Link fora do repositório oficial, hash inválido ou JSON quebrado são recusados.
        let bad = meta("0.9.0", 900).replace("github.com/lbraz55/empeto", "exemplo.com/x");
        assert!(parse_metadata(&bad).is_err());
        assert!(parse_metadata(&meta("0.9.0", 900).replace(&"a".repeat(64), "abc")).is_err());
        assert!(parse_metadata("{").is_err());
        assert!(!is_official_url("https://github.com/lbraz55/empeto/releases/download/../x.apk"));
    }

    #[test]
    fn auto_interval() {
        let now = 100 * 86_400_000;
        assert!(auto_check_due(true, None, now));
        assert!(!auto_check_due(false, None, now));
        assert!(!auto_check_due(true, Some(now - 86_400_000), now));
        assert!(auto_check_due(true, Some(now - AUTO_INTERVAL_MS), now));
    }
}
