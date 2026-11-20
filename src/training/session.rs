//! Execução de sessão (RF-04..RF-08, RN-01..RN-05).

use crate::i18n::tr;
use crate::domain::*;
use crate::persistence::Db;
use rusqlite::{params, OptionalExtension, Row};

fn row_session(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        plan_id: r.get(1)?,
        schedule_id: r.get(2)?,
        plan_name: r.get(3)?,
        schedule_name: r.get(4)?,
        started_at: r.get(5)?,
        ended_at: r.get(6)?,
        status: SessionStatus::parse(&r.get::<_, String>(7)?),
        notes: r.get(8)?,
        plan_snapshot_json: r.get(9)?,
        feeling: r.get(10)?,
    })
}

const SESSION_COLS: &str =
    "id, plan_id, schedule_id, plan_name, schedule_name, started_at, ended_at, status, notes, plan_snapshot_json, feeling";

/// Desempenho mais recente de um exercício (para pré-preencher cargas).
#[derive(Debug, Clone, PartialEq)]
pub struct LastPerformance {
    pub session_started_at: i64,
    pub sets: Vec<(i64, Option<i64>, Option<i64>)>, // (set_number, reps, load_g)
}

impl Db {
    pub fn active_session_id(&self) -> AppResult<Option<Id>> {
        Ok(self
            .conn
            .query_row("SELECT id FROM workout_session WHERE status = 'in_progress' LIMIT 1", [], |r| r.get(0))
            .optional()?)
    }

    /// Inicia sessão a partir de um dia da ficha: grava snapshot + exercícios + séries planejadas.
    /// Tudo em uma transação. O planejado só vira "realizado" com o registro das séries (RN-02).
    pub fn start_session(&mut self, day_id: Id) -> AppResult<Id> {
        if self.active_session_id()?.is_some() {
            return Err(AppError::Conflict(
                tr("Já existe uma sessão em andamento. Finalize-a ou descarte-a antes de iniciar outra.").into(),
            ));
        }
        let day = self.get_day(day_id)?;
        let plan = self.get_plan(day.plan_id)?;
        let exercises = self.list_plan_exercises(day_id)?;
        if exercises.is_empty() {
            return validation(tr("Este dia não tem exercícios. Adicione exercícios antes de iniciar."));
        }
        let now = now_ms();
        let day_full = DayFull { day: day.clone(), exercises };
        let snapshot = PlanSnapshot::build(&plan, &day_full, now);
        let json = serde_json::to_string(&snapshot)?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO workout_session(plan_id, schedule_id, plan_name, schedule_name, started_at, status, plan_snapshot_json)
             VALUES (?1, ?2, ?3, ?4, ?5, 'in_progress', ?6)",
            params![plan.id, day.id, plan.name, day.name, now, json],
        )?;
        let sid = tx.last_insert_rowid();
        for (pos, pe) in day_full.exercises.iter().enumerate() {
            tx.execute(
                "INSERT INTO session_exercise(session_id, plan_exercise_id, original_exercise_id, performed_exercise_id,
                   original_name, performed_name, muscle_focus, exercise_type, equipment, substitution_type, position, rest_seconds,
                   group_tag)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?4, ?5, ?6, ?7, 'none', ?8, ?9, ?10)",
                params![
                    sid, pe.id, pe.exercise.id, pe.exercise.name, pe.exercise.muscle_focus, pe.exercise.exercise_type,
                    pe.exercise.equipment, pos as i64, pe.rest_seconds, pe.group_tag
                ],
            )?;
            let seid = tx.last_insert_rowid();
            for n in 1..=pe.target.sets {
                tx.execute(
                    "INSERT INTO session_set(session_exercise_id, set_number, planned_reps_min, planned_reps_max, planned_load_g,
                       set_kind)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![seid, n, pe.target.reps_min, pe.target.reps_max, pe.target.load_g, pe.kind_of(n).as_str()],
                )?;
            }
        }
        tx.commit()?;
        Ok(sid)
    }

    pub fn get_session(&self, id: Id) -> AppResult<Session> {
        self.conn
            .query_row(&format!("SELECT {SESSION_COLS} FROM workout_session WHERE id = ?1"), params![id], row_session)
            .optional()?
            .ok_or_else(|| AppError::NotFound(tr("Sessão não encontrada.").into()))
    }

    pub fn get_session_full(&self, id: Id) -> AppResult<SessionFull> {
        let session = self.get_session(id)?;
        let mut st = self.conn.prepare(
            "SELECT id, session_id, plan_exercise_id, original_exercise_id, performed_exercise_id, original_name,
                    performed_name, muscle_focus, exercise_type, equipment, substitution_type, position, rest_seconds,
                    notes, completed, group_tag
             FROM session_exercise WHERE session_id = ?1 ORDER BY position, id",
        )?;
        let mut exercises = st
            .query_map(params![id], |r| {
                Ok(SessionExercise {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    plan_exercise_id: r.get(2)?,
                    original_exercise_id: r.get(3)?,
                    performed_exercise_id: r.get(4)?,
                    original_name: r.get(5)?,
                    performed_name: r.get(6)?,
                    muscle_focus: r.get(7)?,
                    exercise_type: r.get(8)?,
                    equipment: r.get(9)?,
                    substitution: SubstitutionType::parse(&r.get::<_, String>(10)?),
                    position: r.get(11)?,
                    rest_seconds: r.get(12)?,
                    notes: r.get(13)?,
                    completed: r.get::<_, i64>(14)? != 0,
                    sets: vec![],
                    group_tag: r.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut st = self.conn.prepare(
            "SELECT id, set_number, planned_reps_min, planned_reps_max, planned_load_g, actual_reps, actual_load_g, completed_at,
                    rest_taken_s, set_kind, rpe, rir
             FROM session_set WHERE session_exercise_id = ?1 ORDER BY set_number",
        )?;
        for e in exercises.iter_mut() {
            e.sets = st
                .query_map(params![e.id], |r| {
                    Ok(SessionSet {
                        id: r.get(0)?,
                        set_number: r.get(1)?,
                        planned_reps_min: r.get(2)?,
                        planned_reps_max: r.get(3)?,
                        planned_load_g: r.get(4)?,
                        actual_reps: r.get(5)?,
                        actual_load_g: r.get(6)?,
                        completed_at: r.get(7)?,
                        rest_taken_s: r.get(8)?,
                        kind: SetKind::parse(&r.get::<_, String>(9)?),
                        rpe: r.get(10)?,
                        rir: r.get(11)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(SessionFull { session, exercises })
    }

    fn ensure_in_progress_for_set(&self, set_id: Id) -> AppResult<Id> {
        let (status, seid): (String, Id) = self
            .conn
            .query_row(
                "SELECT s.status, e.id FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
                 JOIN workout_session s ON s.id = e.session_id WHERE ss.id = ?1",
                params![set_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(tr("Série não encontrada.").into()))?;
        if status != "in_progress" {
            return Err(AppError::Conflict(tr("A sessão já foi concluída.").into()));
        }
        Ok(seid)
    }

    fn ensure_in_progress_for_exercise(&self, se_id: Id) -> AppResult<Id> {
        let (status, sid): (String, Id) = self
            .conn
            .query_row(
                "SELECT s.status, s.id FROM session_exercise e JOIN workout_session s ON s.id = e.session_id WHERE e.id = ?1",
                params![se_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(tr("Exercício da sessão não encontrado.").into()))?;
        if status != "in_progress" {
            return Err(AppError::Conflict(tr("A sessão já foi concluída.").into()));
        }
        Ok(sid)
    }

    /// Registra carga e repetições realizadas na série (RF-05). Valores planejados ficam separados (RN-05).
    pub fn record_set(&self, set_id: Id, reps: i64, load_g: i64) -> AppResult<()> {
        self.ensure_in_progress_for_set(set_id)?;
        if !(0..=1000).contains(&reps) {
            return validation(tr("Repetições devem estar entre 0 e 1000."));
        }
        if !(0..=MAX_LOAD_G).contains(&load_g) {
            return validation(tr("Carga fora do intervalo permitido."));
        }
        self.conn.execute(
            "UPDATE session_set SET actual_reps=?2, actual_load_g=?3, completed_at=COALESCE(completed_at, ?4) WHERE id=?1",
            params![set_id, reps, load_g, now_ms()],
        )?;
        Ok(())
    }

    /// Grava o descanso cumprido após a série (medido pelo cronômetro). Só em sessão em andamento.
    pub fn set_rest_taken(&self, set_id: Id, seconds: i64) -> AppResult<()> {
        self.ensure_in_progress_for_set(set_id)?;
        self.conn.execute(
            "UPDATE session_set SET rest_taken_s = ?2 WHERE id = ?1 AND completed_at IS NOT NULL",
            params![set_id, seconds.clamp(0, 10_800)],
        )?;
        Ok(())
    }

    /// Desfaz o registro de uma série (ainda na sessão em andamento).
    pub fn clear_set(&self, set_id: Id) -> AppResult<()> {
        self.ensure_in_progress_for_set(set_id)?;
        self.conn.execute(
            "UPDATE session_set SET actual_reps=NULL, actual_load_g=NULL, completed_at=NULL, rest_taken_s=NULL WHERE id=?1",
            params![set_id],
        )?;
        Ok(())
    }

    /// Acrescenta uma série extra ao exercício da sessão.
    pub fn add_session_set(&self, se_id: Id) -> AppResult<Id> {
        self.ensure_in_progress_for_exercise(se_id)?;
        let last: Option<(i64, i64, i64, i64)> = self
            .conn
            .query_row(
                "SELECT set_number, planned_reps_min, planned_reps_max, planned_load_g FROM session_set
                 WHERE session_exercise_id = ?1 ORDER BY set_number DESC LIMIT 1",
                params![se_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (n, a, b, l) = last.map(|(n, a, b, l)| (n + 1, a, b, l)).unwrap_or((1, 10, 10, 0));
        if n > 50 {
            return validation(tr("Limite de 50 séries por exercício."));
        }
        self.conn.execute(
            "INSERT INTO session_set(session_exercise_id, set_number, planned_reps_min, planned_reps_max, planned_load_g)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![se_id, n, a, b, l],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Remove a última série se ela ainda não foi registrada.
    pub fn remove_last_session_set(&self, se_id: Id) -> AppResult<()> {
        self.ensure_in_progress_for_exercise(se_id)?;
        let last: Option<(Id, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT id, completed_at FROM session_set WHERE session_exercise_id = ?1 ORDER BY set_number DESC LIMIT 1",
                params![se_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match last {
            Some((id, None)) => {
                self.conn.execute("DELETE FROM session_set WHERE id = ?1", params![id])?;
                Ok(())
            }
            Some((_, Some(_))) => validation(tr("A última série já foi registrada; desfaça o registro antes de removê-la.")),
            None => Ok(()),
        }
    }

    pub fn set_session_exercise_notes(&self, se_id: Id, notes: &str) -> AppResult<()> {
        self.ensure_in_progress_for_exercise(se_id)?;
        self.conn.execute("UPDATE session_exercise SET notes=?2 WHERE id=?1", params![se_id, clean_notes(notes)])?;
        Ok(())
    }

    pub fn set_session_exercise_completed(&self, se_id: Id, completed: bool) -> AppResult<()> {
        self.ensure_in_progress_for_exercise(se_id)?;
        self.conn.execute(
            "UPDATE session_exercise SET completed=?2 WHERE id=?1",
            params![se_id, completed as i64],
        )?;
        Ok(())
    }

    pub fn set_session_notes(&self, session_id: Id, notes: &str) -> AppResult<()> {
        let s = self.get_session(session_id)?;
        if s.status != SessionStatus::InProgress {
            return Err(AppError::Conflict(tr("A sessão já foi concluída.").into()));
        }
        self.conn.execute("UPDATE workout_session SET notes=?2 WHERE id=?1", params![session_id, clean_notes(notes)])?;
        Ok(())
    }

    /// Substitui o exercício (RF-08, RN-03, RN-04).
    /// - Temporária: só a sessão atual muda; a ficha permanece igual.
    /// - Permanente: a ficha passa a usar o substituto daqui em diante; sessões antigas intactas.
    ///   O exercício original continua registrado na sessão.
    pub fn substitute_exercise(&mut self, se_id: Id, new_exercise_id: Id, permanent: bool) -> AppResult<()> {
        self.ensure_in_progress_for_exercise(se_id)?;
        let ex = self.get_exercise(new_exercise_id)?;
        let plan_exercise_id: Option<Id> = self.conn.query_row(
            "SELECT plan_exercise_id FROM session_exercise WHERE id = ?1",
            params![se_id],
            |r| r.get(0),
        )?;
        let kind = if permanent { SubstitutionType::Permanent } else { SubstitutionType::Temporary };
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE session_exercise SET performed_exercise_id=?2, performed_name=?3, muscle_focus=?4, exercise_type=?5,
               equipment=?6, substitution_type=?7 WHERE id=?1",
            params![se_id, ex.id, ex.name, ex.muscle_focus, ex.exercise_type, ex.equipment, kind.as_str()],
        )?;
        if permanent {
            if let Some(pe) = plan_exercise_id {
                tx.execute("UPDATE plan_exercise SET exercise_id=?2 WHERE id=?1", params![pe, ex.id])?;
                tx.execute(
                    "UPDATE workout_plan SET updated_at=?2 WHERE id=(SELECT plan_id FROM plan_exercise WHERE id=?1)",
                    params![pe, now_ms()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Volta a sessão para o exercício original (desfaz substituição temporária ou permanente na sessão).
    pub fn undo_substitution(&mut self, se_id: Id) -> AppResult<()> {
        self.ensure_in_progress_for_exercise(se_id)?;
        let (orig_id, orig_name, sub): (Option<Id>, String, String) = self.conn.query_row(
            "SELECT original_exercise_id, original_name, substitution_type FROM session_exercise WHERE id=?1",
            params![se_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if sub == "permanent" {
            return Err(AppError::Conflict(
                tr("A substituição permanente já alterou a ficha. Edite a ficha para voltar ao exercício anterior.").into(),
            ));
        }
        let (m, t, e) = match orig_id.and_then(|id| self.get_exercise(id).ok()) {
            Some(x) => (x.muscle_focus, x.exercise_type, x.equipment),
            None => (String::new(), String::new(), String::new()),
        };
        self.conn.execute(
            "UPDATE session_exercise SET performed_exercise_id=?2, performed_name=?3, muscle_focus=?4, exercise_type=?5,
               equipment=?6, substitution_type='none' WHERE id=?1",
            params![se_id, orig_id, orig_name, m, t, e],
        )?;
        Ok(())
    }

    /// Conclui a sessão; a partir daqui ela é imutável (triggers no banco).
    pub fn finish_session(&mut self, session_id: Id, notes: &str) -> AppResult<()> {
        let s = self.get_session(session_id)?;
        if s.status != SessionStatus::InProgress {
            return Err(AppError::Conflict(tr("A sessão já foi concluída.").into()));
        }
        let full = self.get_session_full(session_id)?;
        if full.completed_sets() == 0 {
            return validation(tr("Registre pelo menos uma série antes de finalizar, ou descarte a sessão."));
        }
        let tx = self.conn.transaction()?;
        // Séries não registradas são removidas: o que fica no histórico é o que foi realizado.
        tx.execute(
            "DELETE FROM session_set WHERE completed_at IS NULL AND session_exercise_id IN
               (SELECT id FROM session_exercise WHERE session_id = ?1)",
            params![session_id],
        )?;
        tx.execute(
            "UPDATE session_exercise SET completed = 1 WHERE session_id = ?1 AND id IN
               (SELECT session_exercise_id FROM session_set)",
            params![session_id],
        )?;
        tx.execute(
            "UPDATE workout_session SET status='completed', ended_at=?2, notes=?3 WHERE id=?1",
            params![session_id, now_ms(), clean_notes(notes)],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Descarta a sessão em andamento (nada é gravado no histórico).
    pub fn discard_session(&mut self, session_id: Id) -> AppResult<()> {
        let s = self.get_session(session_id)?;
        if s.status != SessionStatus::InProgress {
            return Err(AppError::Conflict(tr("Sessões concluídas não podem ser descartadas.").into()));
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM workout_session WHERE id = ?1", params![session_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Carga/repetições da série anterior registrada do mesmo exercício (3.3 — "repetir última série").
    pub fn previous_set_values(&self, set_id: Id) -> AppResult<Option<(i64, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT p.actual_load_g, p.actual_reps FROM session_set s
                 JOIN session_set p ON p.session_exercise_id = s.session_exercise_id
                 WHERE s.id = ?1 AND p.set_number < s.set_number AND p.completed_at IS NOT NULL
                 ORDER BY p.set_number DESC LIMIT 1",
                params![set_id],
                |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(0), r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
            .optional()?)
    }

    /// Valores planejados da série (carga, repetições máximas da faixa).
    pub fn planned_set_values(&self, set_id: Id) -> AppResult<(i64, i64)> {
        self.conn
            .query_row(
                "SELECT planned_load_g, planned_reps_max FROM session_set WHERE id = ?1",
                params![set_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(tr("Série não encontrada.").into()))
    }

    /// Valores da mesma série na última execução concluída do exercício.
    pub fn last_execution_values(&self, set_id: Id) -> AppResult<Option<(i64, i64)>> {
        let found: Option<(Option<Id>, i64, Id)> = self
            .conn
            .query_row(
                "SELECT e.performed_exercise_id, s.set_number, e.session_id FROM session_set s
                 JOIN session_exercise e ON e.id = s.session_exercise_id WHERE s.id = ?1",
                params![set_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((Some(ex), n, sid)) = found else { return Ok(None) };
        let Some(last) = self.last_performance(ex, sid)? else { return Ok(None) };
        let pick = last.sets.iter().find(|x| x.0 == n).or(last.sets.last());
        Ok(pick.and_then(|(_, r, l)| Some((l.unwrap_or(0), (*r)?))))
    }

    /// Restaura exatamente um estado anterior da série (usado pelo "Desfazer"; só em sessão em andamento).
    pub fn restore_set_state(
        &self,
        set_id: Id,
        reps: Option<i64>,
        load_g: Option<i64>,
        completed_at: Option<i64>,
        rest_taken_s: Option<i64>,
    ) -> AppResult<()> {
        self.ensure_in_progress_for_set(set_id)?;
        self.conn.execute(
            "UPDATE session_set SET actual_reps=?2, actual_load_g=?3, completed_at=?4, rest_taken_s=?5 WHERE id=?1",
            params![set_id, reps, load_g, completed_at, rest_taken_s],
        )?;
        Ok(())
    }

    pub fn get_session_set(&self, set_id: Id) -> AppResult<SessionSet> {
        self.conn
            .query_row(
                "SELECT id, set_number, planned_reps_min, planned_reps_max, planned_load_g, actual_reps, actual_load_g,
                        completed_at, rest_taken_s, set_kind, rpe, rir FROM session_set WHERE id = ?1",
                params![set_id],
                |r| {
                    Ok(SessionSet {
                        id: r.get(0)?,
                        set_number: r.get(1)?,
                        planned_reps_min: r.get(2)?,
                        planned_reps_max: r.get(3)?,
                        planned_load_g: r.get(4)?,
                        actual_reps: r.get(5)?,
                        actual_load_g: r.get(6)?,
                        completed_at: r.get(7)?,
                        rest_taken_s: r.get(8)?,
                        kind: SetKind::parse(&r.get::<_, String>(9)?),
                        rpe: r.get(10)?,
                        rir: r.get(11)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(tr("Série não encontrada.").into()))
    }

    /// Último desempenho concluído de um exercício (por id do exercício realizado).
    pub fn last_performance(&self, exercise_id: Id, before_session: Id) -> AppResult<Option<LastPerformance>> {
        let found: Option<(Id, i64)> = self
            .conn
            .query_row(
                "SELECT e.id, s.started_at FROM session_exercise e JOIN workout_session s ON s.id = e.session_id
                 WHERE e.performed_exercise_id = ?1 AND s.status = 'completed' AND s.id <> ?2
                 ORDER BY s.started_at DESC LIMIT 1",
                params![exercise_id, before_session],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((seid, started)) = found else { return Ok(None) };
        let mut st = self.conn.prepare(
            "SELECT set_number, actual_reps, actual_load_g FROM session_set WHERE session_exercise_id = ?1 ORDER BY set_number",
        )?;
        let sets = st
            .query_map(params![seid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(LastPerformance { session_started_at: started, sets }))
    }

    /// Próximo dia de uma ficha em sequência (A → B → C → A…), baseado na última sessão concluída.
    pub fn next_sequence_day(&self, plan_id: Id) -> AppResult<Option<PlanDay>> {
        let days = self.list_days(plan_id)?;
        if days.is_empty() {
            return Ok(None);
        }
        let last: Option<Option<Id>> = self
            .conn
            .query_row(
                "SELECT schedule_id FROM workout_session WHERE plan_id = ?1 AND status = 'completed'
                 ORDER BY started_at DESC LIMIT 1",
                params![plan_id],
                |r| r.get(0),
            )
            .optional()?;
        let idx = match last.flatten().and_then(|sid| days.iter().position(|d| d.id == sid)) {
            Some(i) => (i + 1) % days.len(),
            None => 0,
        };
        Ok(Some(days[idx].clone()))
    }
}
