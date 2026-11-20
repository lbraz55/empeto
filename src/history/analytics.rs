//! Análise do histórico (documento de evolução v1.1, 8.4, 8.7, 8.8 e 8.9) — `training_analytics`.
//!
//! Tudo é calculado a partir das sessões concluídas, com agregações no SQLite; nada é gravado e os
//! registros originais nunca mudam.

use crate::calendar::{add_months, day_bounds_ms, weekday_index};
use crate::domain::*;
use crate::i18n::tr;
use crate::persistence::Db;
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, OptionalExtension};

/// Estatísticas de um exercício dentro de uma sessão (séries registradas, sem aquecimento no volume).
#[derive(Debug, Clone, PartialEq)]
pub struct ExerciseStats {
    pub key: String,
    pub name: String,
    pub sets: i64,
    pub max_load_g: i64,
    pub total_reps: i64,
    pub volume_g: i64,
    pub e1rm_g: i64,
    pub avg_rpe: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionStats {
    pub id: Id,
    pub title: String,
    pub started_at: i64,
    pub duration_ms: Option<i64>,
    pub sets: i64,
    pub volume_g: i64,
    pub avg_rpe: Option<f64>,
    pub feeling: Option<i64>,
    pub exercises: Vec<ExerciseStats>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonthReport {
    pub year: i32,
    pub month: u32,
    pub sessions: i64,
    pub days_trained: i64,
    pub total_duration_ms: i64,
    pub avg_duration_ms: i64,
    pub distinct_exercises: i64,
    pub sets: i64,
    pub volume_g: i64,
    pub avg_rpe: Option<f64>,
    pub avg_feeling: Option<f64>,
    /// Sessões por semana do mês (início da semana, quantidade).
    pub weeks: Vec<(NaiveDate, i64)>,
    /// Séries registradas por grupo muscular, da maior para a menor.
    pub muscles: Vec<(String, i64)>,
    pub records: Vec<crate::history::records::RecordEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiaryEntry {
    pub session_id: Id,
    pub started_at: i64,
    pub title: String,
    pub feeling: Option<i64>,
    pub notes: String,
    pub exercise_notes: Vec<(String, String)>,
}

pub fn feeling_label(f: i64) -> &'static str {
    match f {
        1 => tr("Muito ruim"),
        2 => tr("Ruim"),
        3 => tr("Normal"),
        4 => tr("Bom"),
        5 => tr("Excelente"),
        _ => "",
    }
}

pub const RPE_VALUES: [f64; 9] = [6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.0];

/// Valida RPE (1–10 em meios pontos) e RIR (0–10).
pub fn validate_effort(rpe: Option<f64>, rir: Option<i64>) -> AppResult<()> {
    if let Some(r) = rpe {
        if !(1.0..=10.0).contains(&r) || (r * 2.0).fract() != 0.0 {
            return validation(tr("RPE deve estar entre 1 e 10 (de meio em meio ponto)."));
        }
    }
    if let Some(r) = rir {
        if !(0..=10).contains(&r) {
            return validation(tr("RIR deve estar entre 0 e 10."));
        }
    }
    Ok(())
}

fn ensure_in_progress(db: &Db, session_id: Id) -> AppResult<()> {
    let st: Option<String> =
        db.conn.query_row("SELECT status FROM workout_session WHERE id = ?1", params![session_id], |r| r.get(0)).optional()?;
    match st.as_deref() {
        Some("in_progress") => Ok(()),
        Some(_) => Err(AppError::Conflict(tr("Sessões concluídas fazem parte do histórico e não podem ser alteradas.").into())),
        None => Err(AppError::NotFound(tr("Sessão não encontrada.").into())),
    }
}

impl Db {
    /// Registra RPE/RIR de uma série da sessão em andamento (ambos opcionais).
    pub fn set_set_effort(&self, set_id: Id, rpe: Option<f64>, rir: Option<i64>) -> AppResult<()> {
        validate_effort(rpe, rir)?;
        let sid: Id = self.conn.query_row(
            "SELECT e.session_id FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id WHERE ss.id = ?1",
            params![set_id],
            |r| r.get(0),
        )?;
        ensure_in_progress(self, sid)?;
        self.conn.execute("UPDATE session_set SET rpe = ?2, rir = ?3 WHERE id = ?1", params![set_id, rpe, rir])?;
        Ok(())
    }

    /// Diário: avaliação do treino (1–5) da sessão em andamento.
    pub fn set_session_feeling(&self, session_id: Id, feeling: Option<i64>) -> AppResult<()> {
        if let Some(f) = feeling {
            if !(1..=5).contains(&f) {
                return validation(tr("Avaliação inválida."));
            }
        }
        ensure_in_progress(self, session_id)?;
        self.conn.execute("UPDATE workout_session SET feeling = ?2 WHERE id = ?1", params![session_id, feeling])?;
        Ok(())
    }

    /// Estatísticas de uma sessão concluída (8.7). Aquecimento não entra em volume, carga máxima e 1RM.
    pub fn session_stats(&self, id: Id) -> AppResult<SessionStats> {
        let s = self.get_session(id)?;
        let mut st = self.conn.prepare(
            "SELECT COALESCE('id:' || e.performed_exercise_id, 'name:' || e.performed_name), e.performed_name,
                    COUNT(ss.id),
                    COALESCE(MAX(CASE WHEN ss.set_kind <> 'warmup' THEN ss.actual_load_g END), 0),
                    COALESCE(SUM(ss.actual_reps), 0),
                    COALESCE(SUM(CASE WHEN ss.set_kind <> 'warmup' THEN COALESCE(ss.actual_load_g,0) * COALESCE(ss.actual_reps,0) END), 0),
                    AVG(ss.rpe)
             FROM session_exercise e JOIN session_set ss ON ss.session_exercise_id = e.id AND ss.completed_at IS NOT NULL
             WHERE e.session_id = ?1
             GROUP BY e.id ORDER BY e.position, e.id",
        )?;
        let mut exercises: Vec<ExerciseStats> = st
            .query_map(params![id], |r| {
                Ok(ExerciseStats {
                    key: r.get(0)?,
                    name: r.get(1)?,
                    sets: r.get(2)?,
                    max_load_g: r.get(3)?,
                    total_reps: r.get(4)?,
                    volume_g: r.get(5)?,
                    e1rm_g: 0,
                    avg_rpe: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // 1RM estimada: melhor série válida de cada exercício.
        let mut st = self.conn.prepare(
            "SELECT COALESCE('id:' || e.performed_exercise_id, 'name:' || e.performed_name), ss.actual_load_g, ss.actual_reps
             FROM session_exercise e JOIN session_set ss ON ss.session_exercise_id = e.id
             WHERE e.session_id = ?1 AND ss.completed_at IS NOT NULL AND ss.set_kind <> 'warmup'",
        )?;
        let rows = st
            .query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (k, l, rp) in rows {
            if let Some(e) = exercises.iter_mut().find(|e| e.key == k) {
                e.e1rm_g = e.e1rm_g.max(estimated_1rm_g(l.unwrap_or(0), rp.unwrap_or(0)));
            }
        }
        let avg_rpe: Option<f64> = self.conn.query_row(
            "SELECT AVG(ss.rpe) FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
             WHERE e.session_id = ?1 AND ss.completed_at IS NOT NULL",
            params![id],
            |r| r.get(0),
        )?;
        Ok(SessionStats {
            id,
            title: format!("{} — {}", s.plan_name, s.schedule_name),
            started_at: s.started_at,
            duration_ms: s.ended_at.map(|e| e - s.started_at),
            sets: exercises.iter().map(|e| e.sets).sum(),
            volume_g: exercises.iter().map(|e| e.volume_g).sum(),
            avg_rpe,
            feeling: s.feeling,
            exercises,
        })
    }

    /// Sessões que podem ser comparadas com `id`: primeiro as do mesmo dia da mesma ficha, depois as demais.
    pub fn comparable_sessions(&self, id: Id) -> AppResult<Vec<crate::history::SessionSummary>> {
        let s = self.get_session(id)?;
        let mut all = self.list_session_summaries(None, None)?;
        all.retain(|x| x.id != id);
        all.sort_by_key(|x| (!(x.plan_name == s.plan_name && x.schedule_name == s.schedule_name), -x.started_at));
        Ok(all)
    }

    /// Relatório mensal (8.8), com agregações no SQLite.
    pub fn month_report(&self, year: i32, month: u32) -> AppResult<MonthReport> {
        let first = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| AppError::Validation(tr("Mês inválido.").into()))?;
        let next = add_months(first, 1);
        let (a, _) = day_bounds_ms(first);
        let (b, _) = day_bounds_ms(next);
        let (sessions, total_dur, avg_feeling): (i64, i64, Option<f64>) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(COALESCE(ended_at, started_at) - started_at), 0), AVG(feeling)
             FROM workout_session WHERE status = 'completed' AND started_at >= ?1 AND started_at < ?2",
            params![a, b],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let (sets, volume, distinct, avg_rpe): (i64, i64, i64, Option<f64>) = self.conn.query_row(
            "SELECT COUNT(ss.id),
                    COALESCE(SUM(CASE WHEN ss.set_kind <> 'warmup' THEN COALESCE(ss.actual_load_g,0) * COALESCE(ss.actual_reps,0) END), 0),
                    COUNT(DISTINCT COALESCE('id:' || e.performed_exercise_id, 'name:' || e.performed_name)),
                    AVG(ss.rpe)
             FROM workout_session s JOIN session_exercise e ON e.session_id = s.id
             JOIN session_set ss ON ss.session_exercise_id = e.id AND ss.completed_at IS NOT NULL
             WHERE s.status = 'completed' AND s.started_at >= ?1 AND s.started_at < ?2",
            params![a, b],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let mut st = self.conn.prepare(
            "SELECT CASE WHEN e.muscle_focus = '' THEN '—' ELSE e.muscle_focus END AS m, COUNT(ss.id) AS n
             FROM workout_session s JOIN session_exercise e ON e.session_id = s.id
             JOIN session_set ss ON ss.session_exercise_id = e.id AND ss.completed_at IS NOT NULL AND ss.set_kind <> 'warmup'
             WHERE s.status = 'completed' AND s.started_at >= ?1 AND s.started_at < ?2
             GROUP BY m ORDER BY n DESC, m",
        )?;
        let muscles = st.query_map(params![a, b], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        let starts: Vec<i64> = {
            let mut st = self.conn.prepare(
                "SELECT started_at FROM workout_session WHERE status = 'completed' AND started_at >= ?1 AND started_at < ?2",
            )?;
            let v = st.query_map(params![a, b], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
            v
        };
        let mut days: Vec<NaiveDate> = starts.iter().map(|ms| crate::calendar::local_date_of(*ms)).collect();
        days.sort();
        days.dedup();
        // Semanas (segunda a domingo) que tocam o mês.
        let mut weeks: Vec<(NaiveDate, i64)> = Vec::new();
        let mut ws = first - Duration::days(weekday_index(first) as i64);
        while ws < next {
            let we = ws + Duration::days(7);
            let n = starts
                .iter()
                .filter(|ms| {
                    let d = crate::calendar::local_date_of(**ms);
                    d >= ws && d < we
                })
                .count() as i64;
            weeks.push((ws, n));
            ws = we;
        }
        let records = self.recent_records(a, None)?.into_iter().filter(|r| r.at_ms < b).collect();
        Ok(MonthReport {
            year,
            month,
            sessions,
            days_trained: days.len() as i64,
            total_duration_ms: total_dur,
            avg_duration_ms: if sessions > 0 { total_dur / sessions } else { 0 },
            distinct_exercises: distinct,
            sets,
            volume_g: volume,
            avg_rpe,
            avg_feeling,
            weeks,
            muscles,
            records,
        })
    }

    /// Diário de treinamento (8.9): sessões com avaliação ou observações, da mais recente para a mais antiga.
    pub fn diary(&self, limit: usize) -> AppResult<Vec<DiaryEntry>> {
        let mut st = self.conn.prepare(
            "SELECT s.id, s.started_at, s.plan_name || ' — ' || s.schedule_name, s.feeling, s.notes
             FROM workout_session s
             WHERE s.status = 'completed' AND (s.feeling IS NOT NULL OR s.notes <> ''
                   OR EXISTS (SELECT 1 FROM session_exercise e WHERE e.session_id = s.id AND e.notes <> ''))
             ORDER BY s.started_at DESC LIMIT ?1",
        )?;
        let mut v: Vec<DiaryEntry> = st
            .query_map(params![limit as i64], |r| {
                Ok(DiaryEntry {
                    session_id: r.get(0)?,
                    started_at: r.get(1)?,
                    title: r.get(2)?,
                    feeling: r.get(3)?,
                    notes: r.get(4)?,
                    exercise_notes: vec![],
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut st = self.conn.prepare(
            "SELECT performed_name, notes FROM session_exercise WHERE session_id = ?1 AND notes <> '' ORDER BY position",
        )?;
        for d in v.iter_mut() {
            d.exercise_notes = st.query_map(params![d.session_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        }
        Ok(v)
    }
}

/// Primeiro dia do mês de uma data.
pub fn month_of(d: NaiveDate) -> (i32, u32) {
    (d.year(), d.month())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Db, Id) {
        let db = Db::open_in_memory().unwrap();
        let p = db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        let day = db.add_day(p, "A", 0).unwrap();
        let ex = db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap().id;
        let pe = db.add_plan_exercise(day, ex, Target { sets: 3, reps_min: 8, reps_max: 10, load_g: 40_000 }, 90, "").unwrap();
        db.set_plan_set_kind(pe, 1, SetKind::Warmup).unwrap();
        (db, day)
    }

    fn run(db: &mut Db, day: Id, loads: [i64; 3], rpe: Option<f64>, feeling: Option<i64>, notes: &str) -> Id {
        let sid = db.start_session(day).unwrap();
        let f = db.get_session_full(sid).unwrap();
        for (s, l) in f.exercises[0].sets.iter().zip(loads) {
            db.record_set(s.id, 10, l).unwrap();
            db.set_set_effort(s.id, rpe, Some(2)).unwrap();
        }
        db.set_session_feeling(sid, feeling).unwrap();
        db.finish_session(sid, notes).unwrap();
        sid
    }

    #[test]
    fn effort_validation_and_immutability() {
        assert!(validate_effort(Some(8.5), Some(2)).is_ok());
        assert!(validate_effort(Some(8.3), None).is_err());
        assert!(validate_effort(Some(11.0), None).is_err());
        assert!(validate_effort(None, Some(11)).is_err());
        let (mut db, day) = setup();
        let sid = run(&mut db, day, [20_000, 40_000, 40_000], Some(8.0), Some(4), "bom treino");
        let set = db.get_session_full(sid).unwrap().exercises[0].sets[1].clone();
        assert_eq!((set.rpe, set.rir), (Some(8.0), Some(2)));
        assert!(db.set_set_effort(set.id, Some(9.0), None).is_err(), "histórico imutável");
        assert!(db.set_session_feeling(sid, Some(1)).is_err());
        // Mesmo direto no banco, a avaliação não muda após concluir.
        assert!(db.conn.execute("UPDATE workout_session SET feeling = 1 WHERE id = ?1", params![sid]).is_err());
    }

    #[test]
    fn stats_compare_report_diary() {
        let (mut db, day) = setup();
        let a = run(&mut db, day, [20_000, 40_000, 40_000], Some(8.0), Some(3), "");
        let b = run(&mut db, day, [20_000, 42_500, 42_500], Some(9.0), Some(5), "mais forte");
        let sa = db.session_stats(a).unwrap();
        let sb = db.session_stats(b).unwrap();
        // Aquecimento (série 1) fora do volume e da carga máxima.
        assert_eq!(sa.volume_g, 2 * 40_000 * 10);
        assert_eq!(sb.exercises[0].max_load_g, 42_500);
        assert_eq!(sa.sets, 3);
        assert_eq!(sb.avg_rpe, Some(9.0));
        assert!(sb.exercises[0].e1rm_g > sa.exercises[0].e1rm_g);
        assert_eq!(db.comparable_sessions(b).unwrap()[0].id, a);
        let today = crate::calendar::today();
        let r = db.month_report(today.year(), today.month()).unwrap();
        assert_eq!(r.sessions, 2);
        assert_eq!(r.days_trained, 1);
        assert_eq!(r.sets, 6);
        assert_eq!(r.muscles[0].1, 4); // só séries válidas (sem aquecimento)
        assert_eq!(r.avg_feeling, Some(4.0));
        assert_eq!(r.weeks.iter().map(|w| w.1).sum::<i64>(), 2);
        assert!(!r.records.is_empty());
        let d = db.diary(10).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].notes, "mais forte");
        assert_eq!(d[0].feeling, Some(5));
        let empty = db.month_report(2001, 1).unwrap();
        assert_eq!(empty.sessions, 0);
    }
}
