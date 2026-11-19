//! Armazenamento local e backups no aparelho (documento EMPETO, Fase 6).
//!
//! - Painel de armazenamento: banco (com WAL/SHM), backups locais, temporários e total.
//! - Backups locais: pasta privada `backups/` dentro do diretório do app. Cada arquivo é um
//!   backup completo no mesmo formato do backup manual (`import_export::backup`), então a
//!   restauração passa pela mesma validação.
//! - Backup automático semanal: ao abrir o app, se o último automático tem 7 dias ou mais.
//!   Mantém apenas os 4 automáticos mais recentes; backups manuais nunca são apagados sozinhos.
//! - Uma cópia local NÃO protege contra perda/desinstalação: o app lembra o usuário de salvar uma
//!   cópia externa, mas nunca afirma que ela existe (só registra quando o próprio usuário salva).

use crate::domain::*;
use crate::import_export::backup::{create_backup, read_manifest};
use crate::persistence::Db;
use std::path::{Path, PathBuf};

pub const BACKUP_DIR: &str = "backups";
pub const AUTO_KEEP: usize = 4;
pub const AUTO_INTERVAL_MS: i64 = 7 * 86_400_000;
/// Depois deste prazo sem cópia externa salva pelo usuário, o lembrete aparece.
pub const EXTERNAL_REMINDER_MS: i64 = 30 * 86_400_000;
pub const PREF_LAST_EXTERNAL: &str = "backup.last_external";
pub const PREF_AUTO: &str = "backup.auto";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StorageReport {
    pub db_bytes: u64,
    pub backups_bytes: u64,
    pub backups_count: usize,
    pub temp_bytes: u64,
}

impl StorageReport {
    pub fn total(&self) -> u64 {
        self.db_bytes + self.backups_bytes + self.temp_bytes
    }
}

pub fn fmt_bytes(b: u64) -> String {
    if b < 1024 {
        format!("{b} B")
    } else if b < 1024 * 1024 {
        format!("{:.0} KB", b as f64 / 1024.0)
    } else {
        format!("{:.1} MB", b as f64 / 1_048_576.0).replace('.', ",")
    }
}

/// Tamanho total (recursivo) de uma pasta. Pasta inexistente = 0.
pub fn dir_size(dir: &Path) -> (u64, usize) {
    let mut total = 0;
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                let (s, c) = dir_size(&e.path());
                total += s;
                n += c;
            } else {
                total += md.len();
                n += 1;
            }
        }
    }
    (total, n)
}

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_DIR)
}

pub fn storage_report(data_dir: &Path, db_path: Option<&Path>, tmp_dir: &Path) -> StorageReport {
    let mut r = StorageReport::default();
    if let Some(p) = db_path {
        for suffix in ["", "-wal", "-shm"] {
            let f = PathBuf::from(format!("{}{}", p.display(), suffix));
            r.db_bytes += std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
        }
    }
    let (b, _) = dir_size(&backups_dir(data_dir));
    r.backups_bytes = b;
    r.backups_count = list_local_backups(data_dir).len();
    r.temp_bytes = dir_size(tmp_dir).0;
    r
}

/// Remove arquivos temporários (importações/exportações já concluídas). Nunca toca no banco
/// nem nos backups. Retorna quantos bytes foram liberados.
pub fn clear_temp(tmp_dir: &Path) -> u64 {
    let before = dir_size(tmp_dir).0;
    if let Ok(rd) = std::fs::read_dir(tmp_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    before.saturating_sub(dir_size(tmp_dir).0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalBackup {
    pub path: PathBuf,
    pub file_name: String,
    pub created_at: i64,
    pub bytes: u64,
    pub automatic: bool,
    pub encrypted: bool,
    pub sessions: i64,
    pub plans: i64,
    /// Manifesto ilegível: o arquivo aparece na lista (para poder ser excluído), mas não restaura.
    pub valid: bool,
}

/// Lista os backups locais, do mais recente para o mais antigo.
pub fn list_local_backups(data_dir: &Path) -> Vec<LocalBackup> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(backups_dir(data_dir)) else { return v };
    for e in rd.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("zip") {
            continue;
        }
        let file_name = e.file_name().to_string_lossy().to_string();
        let md = e.metadata().ok();
        let bytes = md.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = md
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let automatic = file_name.starts_with("empeto-auto-");
        match read_manifest(&path) {
            Ok(m) => v.push(LocalBackup {
                path,
                file_name,
                created_at: m.created_at,
                bytes,
                automatic,
                encrypted: m.encrypted,
                sessions: m.counts.sessions,
                plans: m.counts.plans,
                valid: true,
            }),
            Err(_) => v.push(LocalBackup {
                path,
                file_name,
                created_at: mtime,
                bytes,
                automatic,
                encrypted: false,
                sessions: 0,
                plans: 0,
                valid: false,
            }),
        }
    }
    v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.file_name.cmp(&a.file_name)));
    v
}

fn stamp(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%Y-%m-%d-%H%M%S").to_string())
        .unwrap_or_else(|| ms.to_string())
}

/// Cria um backup local (sem senha: fica na área privada do app). Retorna o caminho.
pub fn create_local_backup(db: &Db, data_dir: &Path, work_dir: &Path, version: &str, automatic: bool) -> AppResult<PathBuf> {
    let dir = backups_dir(data_dir);
    std::fs::create_dir_all(&dir)?;
    let kind = if automatic { "auto" } else { "manual" };
    let mut path = dir.join(format!("empeto-{kind}-{}.zip", stamp(now_ms())));
    let mut i = 2;
    while path.exists() {
        path = dir.join(format!("empeto-{kind}-{}-{i}.zip", stamp(now_ms())));
        i += 1;
    }
    let tmp = dir.join(format!(".parcial-{}.zip", now_ms()));
    create_backup(db, &tmp, work_dir, version, None)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Mantém só os `keep` backups automáticos mais recentes. Backups manuais não são tocados.
pub fn prune_automatic(data_dir: &Path, keep: usize) -> usize {
    let autos: Vec<LocalBackup> = list_local_backups(data_dir).into_iter().filter(|b| b.automatic).collect();
    let mut removed = 0;
    for b in autos.into_iter().skip(keep) {
        if std::fs::remove_file(&b.path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Exclui um backup local. Só aceita arquivos dentro da pasta de backups.
pub fn delete_local_backup(data_dir: &Path, path: &Path) -> AppResult<()> {
    let dir = backups_dir(data_dir).canonicalize()?;
    let p = path.canonicalize().map_err(|_| AppError::File("Backup não encontrado.".into()))?;
    if p.parent() != Some(dir.as_path()) {
        return Err(AppError::File("Arquivo fora da pasta de backups.".into()));
    }
    std::fs::remove_file(&p)?;
    Ok(())
}

/// Último backup automático (ms), se houver.
pub fn last_automatic(data_dir: &Path) -> Option<i64> {
    list_local_backups(data_dir).into_iter().filter(|b| b.automatic && b.valid).map(|b| b.created_at).max()
}

pub fn auto_backup_due(last_auto: Option<i64>, now: i64) -> bool {
    match last_auto {
        None => true,
        Some(t) => now - t >= AUTO_INTERVAL_MS || t > now + 86_400_000, // relógio voltou: refaz
    }
}

/// Executa o backup automático semanal se estiver na hora e houver dados. Retorna o caminho criado.
pub fn run_auto_backup(db: &Db, data_dir: &Path, work_dir: &Path, version: &str) -> AppResult<Option<PathBuf>> {
    if db.get_pref(PREF_AUTO)?.as_deref() == Some("0") {
        return Ok(None);
    }
    if !auto_backup_due(last_automatic(data_dir), now_ms()) {
        return Ok(None);
    }
    let c = db.counts()?;
    if c.plans == 0 && c.sessions == 0 {
        return Ok(None); // nada a proteger ainda
    }
    let p = create_local_backup(db, data_dir, work_dir, version, true)?;
    prune_automatic(data_dir, AUTO_KEEP);
    Ok(Some(p))
}

/// Lembrete de cópia externa: só aparece se há dados e o usuário não salvou uma cópia fora do
/// app nos últimos 30 dias (registrado apenas quando ele próprio salva pelo seletor do sistema).
pub fn external_copy_reminder_due(last_external: Option<i64>, has_data: bool, now: i64) -> bool {
    has_data && last_external.map(|t| now - t >= EXTERNAL_REMINDER_MS).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("empeto-storage-{tag}-{}", now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn db_with_data() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        db
    }

    #[test]
    fn auto_backup_weekly_keeps_four_and_spares_manual() {
        let dir = tmpdir("auto");
        let work = dir.join("tmp");
        let db = db_with_data();
        create_local_backup(&db, &dir, &work, "t", false).unwrap();
        for _ in 0..6 {
            create_local_backup(&db, &dir, &work, "t", true).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(list_local_backups(&dir).len(), 7);
        assert_eq!(prune_automatic(&dir, AUTO_KEEP), 2);
        let list = list_local_backups(&dir);
        assert_eq!(list.iter().filter(|b| b.automatic).count(), 4);
        assert_eq!(list.iter().filter(|b| !b.automatic).count(), 1);
        assert!(list.iter().all(|b| b.valid && b.plans == 1));
        // Já há um automático recente: não cria outro.
        assert!(run_auto_backup(&db, &dir, &work, "t").unwrap().is_none());
        let r = storage_report(&dir, None, &work);
        assert_eq!(r.backups_count, 5);
        assert!(r.backups_bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn due_rules_and_empty_db() {
        let now = 100 * 86_400_000;
        assert!(auto_backup_due(None, now));
        assert!(!auto_backup_due(Some(now - 86_400_000), now));
        assert!(auto_backup_due(Some(now - AUTO_INTERVAL_MS), now));
        assert!(external_copy_reminder_due(None, true, now));
        assert!(!external_copy_reminder_due(None, false, now));
        assert!(!external_copy_reminder_due(Some(now - 86_400_000), true, now));
        let dir = tmpdir("empty");
        let db = Db::open_in_memory().unwrap();
        assert!(run_auto_backup(&db, &dir, &dir.join("tmp"), "t").unwrap().is_none());
        let db = db_with_data();
        db.set_pref(PREF_AUTO, "0").unwrap();
        assert!(run_auto_backup(&db, &dir, &dir.join("tmp"), "t").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_only_inside_backup_dir_and_clear_temp() {
        let dir = tmpdir("del");
        let work = dir.join("tmp");
        let db = db_with_data();
        let p = create_local_backup(&db, &dir, &work, "t", false).unwrap();
        let outside = dir.join("outro.zip");
        std::fs::write(&outside, b"x").unwrap();
        assert!(delete_local_backup(&dir, &outside).is_err());
        delete_local_backup(&dir, &p).unwrap();
        assert!(list_local_backups(&dir).is_empty());
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("a.xlsx"), vec![0u8; 2048]).unwrap();
        assert_eq!(clear_temp(&work), 2048);
        assert!(outside.exists());
        assert_eq!(fmt_bytes(2048), "2 KB");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
