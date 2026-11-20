//! Backup e restauração local (RF-18, seção 5.3, CT-10).
//!
//! Formato: arquivo .zip contendo
//!   - `manifest.json` — metadados (formato, versões, data, contagens, SHA-256 do banco);
//!   - `workout.db`    — cópia consistente do SQLite gerada com `VACUUM INTO`.
//! A restauração valida tudo antes de substituir os dados atuais e mantém uma cópia do
//! banco anterior; qualquer falha preserva o banco anterior.

use crate::i18n::tr;
use crate::domain::*;
#[allow(unused_imports)]
use crate::domain::validation;
use crate::persistence::{migrate, schema_version, Db, DbCounts, SCHEMA_VERSION};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const BACKUP_FORMAT: &str = "workout-backup";
pub const BACKUP_FORMAT_VERSION: u32 = 1;
const MAX_BACKUP_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: i64,
    pub db_sha256: String,
    pub db_bytes: u64,
    pub counts: DbCounts,
    /// O banco dentro do zip está protegido por senha (AES-256).
    #[serde(default)]
    pub encrypted: bool,
}

fn sha256_file(path: &Path) -> AppResult<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        h.update(&buf[..n]);
    }
    let digest = h.finalize();
    Ok((digest.iter().map(|b| format!("{:02x}", b)).collect(), total))
}

fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::File(trf!("Arquivo de backup inválido: {}", e.to_string()))
}

/// Cria o backup em `out_zip`, usando `work_dir` para arquivos temporários.
pub fn create_backup(
    db: &Db,
    out_zip: &Path,
    work_dir: &Path,
    app_version: &str,
    password: Option<&str>,
) -> AppResult<BackupManifest> {
    let password = password.map(str::trim).filter(|p| !p.is_empty());
    if let Some(p) = password {
        if p.chars().count() < 4 {
            return validation(tr("A senha do backup deve ter pelo menos 4 caracteres."));
        }
    }
    std::fs::create_dir_all(work_dir)?;
    let tmp_db = work_dir.join(format!("backup-{}.db", now_ms()));
    let _ = std::fs::remove_file(&tmp_db);
    db.conn.execute("VACUUM INTO ?1", params![tmp_db.to_string_lossy()])?;
    let result = (|| {
        let (sha, bytes) = sha256_file(&tmp_db)?;
        let manifest = BackupManifest {
            format: BACKUP_FORMAT.into(),
            format_version: BACKUP_FORMAT_VERSION,
            app_version: app_version.into(),
            schema_version: schema_version(&db.conn)?,
            created_at: now_ms(),
            db_sha256: sha,
            db_bytes: bytes,
            counts: db.counts()?,
            encrypted: password.is_some(),
        };
        let file = std::fs::File::create(out_zip)?;
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("manifest.json", opts).map_err(zip_err)?;
        zw.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        let db_opts = match password {
            Some(p) => opts.with_aes_encryption(zip::AesMode::Aes256, p),
            None => opts,
        };
        zw.start_file("workout.db", db_opts).map_err(zip_err)?;
        let mut f = std::fs::File::open(&tmp_db)?;
        std::io::copy(&mut f, &mut zw)?;
        zw.finish().map_err(zip_err)?;
        Ok(manifest)
    })();
    let _ = std::fs::remove_file(&tmp_db);
    if result.is_err() {
        let _ = std::fs::remove_file(out_zip);
    }
    result
}

/// Backup validado e pronto para substituir o banco atual.
#[derive(Debug)]
pub struct ValidatedBackup {
    pub manifest: BackupManifest,
    pub db_path: PathBuf,
}

/// Valida o arquivo: estrutura do zip, manifesto, hash, integridade do SQLite e versão do schema.
/// Também aplica migrações na cópia extraída (backups de versões anteriores são aceitos).
/// Lê só o manifesto (para saber se o backup pede senha e mostrar o resumo).
pub fn read_manifest(zip_path: &Path) -> AppResult<BackupManifest> {
    let file = std::fs::File::open(zip_path)?;
    let mut za = zip::ZipArchive::new(file)
        .map_err(|_| AppError::File(tr("O arquivo selecionado não é um backup do EMPETO (.zip).").into()))?;
    let mut m = za.by_name("manifest.json").map_err(|_| AppError::File(tr("Backup sem manifesto (manifest.json).").into()))?;
    if m.size() > 64 * 1024 {
        return Err(AppError::File(tr("Manifesto do backup inválido.").into()));
    }
    let mut s = String::new();
    m.read_to_string(&mut s)?;
    serde_json::from_str(&s).map_err(|_| AppError::File(tr("Manifesto do backup inválido.").into()))
}

pub fn validate_backup(zip_path: &Path, work_dir: &Path, password: Option<&str>) -> AppResult<ValidatedBackup> {
    let meta = std::fs::metadata(zip_path).map_err(|_| AppError::File(tr("Não foi possível ler o arquivo.").into()))?;
    if meta.len() > MAX_BACKUP_BYTES {
        return Err(AppError::File(tr("Arquivo grande demais para ser um backup do EMPETO.").into()));
    }
    let file = std::fs::File::open(zip_path)?;
    let mut za = zip::ZipArchive::new(file)
        .map_err(|_| AppError::File(tr("O arquivo selecionado não é um backup do EMPETO (.zip).").into()))?;
    if za.len() > 10 {
        return Err(AppError::File(tr("Backup com conteúdo inesperado.").into()));
    }
    let manifest: BackupManifest = {
        let mut m = za
            .by_name("manifest.json")
            .map_err(|_| AppError::File(tr("Backup sem manifesto (manifest.json).").into()))?;
        if m.size() > 64 * 1024 {
            return Err(AppError::File(tr("Manifesto do backup inválido.").into()));
        }
        let mut s = String::new();
        m.read_to_string(&mut s)?;
        serde_json::from_str(&s).map_err(|_| AppError::File(tr("Manifesto do backup inválido.").into()))?
    };
    if manifest.format != BACKUP_FORMAT {
        return Err(AppError::File(tr("O arquivo não é um backup do EMPETO.").into()));
    }
    if manifest.format_version > BACKUP_FORMAT_VERSION || manifest.schema_version > SCHEMA_VERSION {
        return Err(AppError::File(
            tr("Este backup foi criado por uma versão mais nova do app. Atualize o aplicativo para restaurá-lo.").into(),
        ));
    }
    std::fs::create_dir_all(work_dir)?;
    let db_path = work_dir.join(format!("restore-{}.db", now_ms()));
    {
        let mut entry = if manifest.encrypted {
            let Some(p) = password.filter(|p| !p.is_empty()) else {
                return Err(AppError::Validation(tr("Este backup é protegido por senha. Informe a senha.").into()));
            };
            za.by_name_decrypt("workout.db", p.as_bytes()).map_err(|e| match e {
                zip::result::ZipError::InvalidPassword => AppError::Validation(tr("Senha do backup incorreta.").into()),
                _ => AppError::File(tr("Backup sem banco de dados.").into()),
            })?
        } else {
            za.by_name("workout.db").map_err(|_| AppError::File(tr("Backup sem banco de dados.").into()))?
        };
        if entry.size() > MAX_BACKUP_BYTES {
            return Err(AppError::File(tr("Banco de dados do backup grande demais.").into()));
        }
        let mut out = std::fs::File::create(&db_path)?;
        std::io::copy(&mut entry.by_ref().take(MAX_BACKUP_BYTES), &mut out)?;
        out.sync_all()?;
    }
    let check = (|| -> AppResult<()> {
        let (sha, _) = sha256_file(&db_path)?;
        if sha != manifest.db_sha256 {
            return Err(AppError::File(tr("Backup corrompido: a verificação de integridade (SHA-256) falhou.").into()));
        }
        let mut conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(AppError::File(tr("Backup corrompido: o banco de dados não passou na verificação.").into()));
        }
        let v = schema_version(&conn)?;
        if v < 1 || v > SCHEMA_VERSION {
            return Err(AppError::File(tr("Versão de banco de dados não suportada no backup.").into()));
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate(&mut conn)?;
        let fk: Option<String> = conn
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .map(Some)
            .or_else(|e| if matches!(e, rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(e) })?;
        if fk.is_some() {
            return Err(AppError::File(tr("Backup inconsistente: referências inválidas.").into()));
        }
        Ok(())
    })();
    if let Err(e) = check {
        let _ = std::fs::remove_file(&db_path);
        return Err(e);
    }
    Ok(ValidatedBackup { manifest, db_path })
}

fn remove_sidecars(p: &Path) {
    for ext in ["-wal", "-shm", "-journal"] {
        let mut s = p.as_os_str().to_owned();
        s.push(ext);
        let _ = std::fs::remove_file(PathBuf::from(s));
    }
}

impl Db {
    /// Substitui o banco atual pelo backup validado. Guarda o anterior em `<db>.pre-restore`.
    /// Em caso de falha, o banco anterior é recolocado.
    pub fn restore_from(&mut self, validated: ValidatedBackup) -> AppResult<()> {
        let Some(path) = self.path.clone() else {
            return Err(AppError::Storage(tr("Restauração indisponível para banco em memória.").into()));
        };
        let pre = path.with_extension("pre-restore.db");
        // Cópia consistente do estado atual (para desfazer, se necessário).
        let _ = std::fs::remove_file(&pre);
        self.conn.execute("VACUUM INTO ?1", params![pre.to_string_lossy()])?;
        // Fecha a conexão atual trocando por uma em memória.
        let old = std::mem::replace(&mut self.conn, Connection::open_in_memory()?);
        old.close().map_err(|(_, e)| AppError::from(e))?;
        remove_sidecars(&path);
        let swap = (|| -> AppResult<Db> {
            std::fs::copy(&validated.db_path, &path)?;
            let db = Db::open(&path)?;
            if !db.integrity_ok()? {
                return Err(AppError::Storage(tr("Falha na verificação após restaurar.").into()));
            }
            Ok(db)
        })();
        let _ = std::fs::remove_file(&validated.db_path);
        match swap {
            Ok(db) => {
                *self = db;
                Ok(())
            }
            Err(e) => {
                log::error!("restauração falhou, recolocando banco anterior: {e}");
                remove_sidecars(&path);
                std::fs::copy(&pre, &path)?;
                *self = Db::open(&path)?;
                Err(e)
            }
        }
    }
}
