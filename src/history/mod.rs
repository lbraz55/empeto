//! Histórico: sessões concluídas e evolução por exercício (RF-14, RF-15).

pub mod analytics;
pub mod records;

use crate::domain::*;
use crate::persistence::Db;
use rusqlite::params;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: Id,
    pub plan_name: String,
    pub schedule_name: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub exercises: i64,
    pub sets: i64,
    pub volume_g: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvolutionMetric {
    MaxLoad,
    Volume,
    Estimated1Rm,
    BestReps,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvolutionPoint {
    pub session_id: Id,
    pub started_at: i64,
    pub max_load_g: i64,
    pub volume_g: i64,
    pub est_1rm_g: i64,
    pub best_reps: i64,
    pub sets: i64,
}

impl EvolutionPoint {
    pub fn value(&self, m: EvolutionMetric) -> i64 {
        match m {
            EvolutionMetric::MaxLoad => self.max_load_g,
            EvolutionMetric::Volume => self.volume_g,
            EvolutionMetric::Estimated1Rm => self.est_1rm_g,
            EvolutionMetric::BestReps => self.best_reps,
        }
    }
}

/// Exercício que aparece no histórico (para escolher na tela de evolução).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryExercise {
    pub key: String,
    pub exercise_id: Option<Id>,
    pub name: String,
    pub sessions: i64,
    pub last_at: i64,
}

impl Db {
    pub fn list_session_summaries(&self, from_ms: Option<i64>, to_ms: Option<i64>) -> AppResult<Vec<SessionSummary>> {
        self.list_session_summaries_limit(from_ms, to_ms, None)
    }

    /// Quantidade de sessões concluídas (sem carregar o histórico).
    pub fn count_completed_sessions(&self) -> AppResult<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM workout_session WHERE status = 'completed'", [], |r| r.get(0))?)
    }

    /// Como `list_session_summaries`, mas só as `limit` mais recentes (12.1: não carregar o histórico inteiro).
    pub fn list_session_summaries_limit(&self, from_ms: Option<i64>, to_ms: Option<i64>, limit: Option<usize>) -> AppResult<Vec<SessionSummary>> {
        let mut st = self.conn.prepare(
            "SELECT s.id, s.plan_name, s.schedule_name, s.started_at, s.ended_at,
                    (SELECT COUNT(*) FROM session_exercise e WHERE e.session_id = s.id
                        AND EXISTS (SELECT 1 FROM session_set ss WHERE ss.session_exercise_id = e.id AND ss.completed_at IS NOT NULL)),
                    (SELECT COUNT(*) FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
                        WHERE e.session_id = s.id AND ss.completed_at IS NOT NULL),
                    (SELECT COALESCE(SUM(COALESCE(ss.actual_load_g,0) * COALESCE(ss.actual_reps,0)),0)
                        FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
                        WHERE e.session_id = s.id AND ss.completed_at IS NOT NULL)
             FROM workout_session s
             WHERE s.status = 'completed' AND s.started_at >= ?1 AND s.started_at < ?2
             ORDER BY s.started_at DESC LIMIT ?3",
        )?;
        let v = st
            .query_map(params![from_ms.unwrap_or(i64::MIN), to_ms.unwrap_or(i64::MAX), limit.map(|l| l as i64).unwrap_or(-1)], |r| {
                Ok(SessionSummary {
                    id: r.get(0)?,
                    plan_name: r.get(1)?,
                    schedule_name: r.get(2)?,
                    started_at: r.get(3)?,
                    ended_at: r.get(4)?,
                    exercises: r.get(5)?,
                    sets: r.get(6)?,
                    volume_g: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Exercícios realizados em sessões concluídas, agrupados por id (ou nome, se o id foi perdido).
    pub fn history_exercises(&self) -> AppResult<Vec<HistoryExercise>> {
        let mut st = self.conn.prepare(
            "SELECT COALESCE('id:' || e.performed_exercise_id, 'name:' || e.performed_name) AS k,
                    e.performed_exercise_id, MAX(e.performed_name), COUNT(DISTINCT s.id), MAX(s.started_at)
             FROM session_exercise e JOIN workout_session s ON s.id = e.session_id
             WHERE s.status = 'completed'
               AND EXISTS (SELECT 1 FROM session_set ss WHERE ss.session_exercise_id = e.id AND ss.completed_at IS NOT NULL)
             GROUP BY k ORDER BY MAX(s.started_at) DESC",
        )?;
        let mut v = st
            .query_map([], |r| {
                Ok(HistoryExercise {
                    key: r.get(0)?,
                    exercise_id: r.get(1)?,
                    name: r.get(2)?,
                    sessions: r.get(3)?,
                    last_at: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // Usa o nome atual do catálogo quando disponível.
        for h in v.iter_mut() {
            if let Some(id) = h.exercise_id {
                if let Ok(e) = self.get_exercise(id) {
                    h.name = e.name;
                }
            }
        }
        Ok(v)
    }

    /// Evolução do exercício identificado por `key` ("id:N" ou "name:X") desde `from_ms`.
    pub fn exercise_evolution(&self, key: &str, from_ms: Option<i64>) -> AppResult<Vec<EvolutionPoint>> {
        let (cond, arg): (&str, String) = if let Some(id) = key.strip_prefix("id:") {
            ("e.performed_exercise_id = CAST(?1 AS INTEGER)", id.to_string())
        } else {
            ("e.performed_exercise_id IS NULL AND e.performed_name = ?1", key.trim_start_matches("name:").to_string())
        };
        let sql = format!(
            "SELECT s.id, s.started_at, ss.actual_load_g, ss.actual_reps
             FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
             JOIN workout_session s ON s.id = e.session_id
             WHERE s.status = 'completed' AND ss.completed_at IS NOT NULL AND {cond} AND s.started_at >= ?2
             ORDER BY s.started_at, ss.set_number"
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st
            .query_map(params![arg, from_ms.unwrap_or(i64::MIN)], |r| {
                Ok((r.get::<_, Id>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out: Vec<EvolutionPoint> = Vec::new();
        for (sid, at, load, reps) in rows {
            let load = load.unwrap_or(0);
            let reps = reps.unwrap_or(0);
            if out.last().map(|p| p.session_id) != Some(sid) {
                out.push(EvolutionPoint {
                    session_id: sid,
                    started_at: at,
                    max_load_g: 0,
                    volume_g: 0,
                    est_1rm_g: 0,
                    best_reps: 0,
                    sets: 0,
                });
            }
            let p = out.last_mut().unwrap();
            p.max_load_g = p.max_load_g.max(load);
            p.volume_g += load * reps;
            p.est_1rm_g = p.est_1rm_g.max(estimated_1rm_g(load, reps));
            p.best_reps = p.best_reps.max(reps);
            p.sets += 1;
        }
        Ok(out)
    }
}
