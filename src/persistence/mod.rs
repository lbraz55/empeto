//! Persistência SQLite: abertura, migrações versionadas e transacionais, preferências.

use crate::domain::{now_ms, AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Migrações em ordem. A posição + 1 é a versão do schema (PRAGMA user_version).
pub const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_seed_exercises.sql"),
    include_str!("../../migrations/0003_rest_taken.sql"),
    include_str!("../../migrations/0004_library_favorites.sql"),
    include_str!("../../migrations/0005_exercise_media.sql"),
];

pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub struct Db {
    pub conn: Connection,
    pub path: Option<PathBuf>,
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         -- FULL: cada série registrada sobrevive a queda de energia/bateria (8.12).
         PRAGMA synchronous = FULL;
         PRAGMA busy_timeout = 3000;",
    )?;
    Ok(())
}

pub fn schema_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Aplica as migrações pendentes, cada uma em sua própria transação.
pub fn migrate(conn: &mut Connection) -> AppResult<()> {
    let current = schema_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(AppError::Storage(trf!("O banco de dados é de uma versão mais nova do app (schema {}). Atualize o aplicativo.", current)));
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate() {
        let version = idx as i64 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {version};"))?;
        tx.commit()?;
        log::info!("migração aplicada: v{version}");
    }
    Ok(())
}

impl Db {
    /// Abre (ou cria) o banco em `path`. Antes de migrar um banco existente, guarda uma cópia.
    pub fn open(path: &Path) -> AppResult<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let existed = path.exists();
        let mut conn = Connection::open(path)?;
        configure(&conn)?;
        let current = schema_version(&conn)?;
        if existed && current > 0 && current < SCHEMA_VERSION {
            let backup = path.with_extension(format!("pre-migration-v{current}.db"));
            let _ = std::fs::remove_file(&backup);
            conn.execute("VACUUM INTO ?1", params![backup.to_string_lossy()])?;
            log::info!("cópia de segurança antes da migração: {}", backup.display());
        }
        migrate(&mut conn)?;
        Ok(Db { conn, path: Some(path.to_path_buf()) })
    }

    pub fn open_in_memory() -> AppResult<Db> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate(&mut conn)?;
        Ok(Db { conn, path: None })
    }

    // ----------------------------------------------------------------------
    // Preferências (key/value)
    // ----------------------------------------------------------------------

    pub fn get_pref(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM app_preference WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_pref(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO app_preference(key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_ms()],
        )?;
        Ok(())
    }

    pub fn all_prefs(&self) -> AppResult<Vec<(String, String)>> {
        let mut st = self.conn.prepare("SELECT key, value FROM app_preference ORDER BY key")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn integrity_ok(&self) -> AppResult<bool> {
        let r: String = self.conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok(r == "ok")
    }

    /// Contagens para a tela "Dados e armazenamento" e manifesto de backup.
    pub fn counts(&self) -> AppResult<DbCounts> {
        let c = |sql: &str| -> AppResult<i64> { Ok(self.conn.query_row(sql, [], |r| r.get(0))?) };
        Ok(DbCounts {
            plans: c("SELECT COUNT(*) FROM workout_plan")?,
            exercises: c("SELECT COUNT(*) FROM exercise WHERE archived = 0")?,
            sessions: c("SELECT COUNT(*) FROM workout_session WHERE status = 'completed'")?,
            sets: c("SELECT COUNT(*) FROM session_set WHERE completed_at IS NOT NULL")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct DbCounts {
    pub plans: i64,
    pub exercises: i64,
    pub sessions: i64,
    pub sets: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_in_memory() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(schema_version(&db.conn).unwrap(), SCHEMA_VERSION);
        assert!(db.counts().unwrap().exercises > 30);
        assert!(db.integrity_ok().unwrap());
    }

    #[test]
    fn migration_is_idempotent_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("w.db");
        {
            let db = Db::open(&p).unwrap();
            db.set_pref("a", "1").unwrap();
        }
        let db = Db::open(&p).unwrap();
        assert_eq!(db.get_pref("a").unwrap().as_deref(), Some("1"));
        assert_eq!(schema_version(&db.conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn upgrade_from_v1_keeps_data_and_makes_backup() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("w.db");
        {
            let mut conn = Connection::open(&p).unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute_batch(MIGRATIONS[0]).unwrap();
            tx.execute_batch("PRAGMA user_version = 1;").unwrap();
            tx.commit().unwrap();
            conn.execute(
                "INSERT INTO app_preference(key,value,updated_at) VALUES ('x','y',0)",
                [],
            )
            .unwrap();
        }
        let db = Db::open(&p).unwrap();
        assert_eq!(db.get_pref("x").unwrap().as_deref(), Some("y"));
        assert!(dir.path().join("w.pre-migration-v1.db").exists());
    }
}
