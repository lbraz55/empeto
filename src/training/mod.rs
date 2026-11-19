//! Treinos: biblioteca de exercícios, fichas, dias e exercícios da ficha (RF-01..RF-03, RF-09).

pub mod session;
pub mod progression;
pub mod templates;

use crate::domain::*;
use crate::persistence::Db;
use rusqlite::{params, OptionalExtension, Row};

fn row_exercise(r: &Row, base: usize) -> rusqlite::Result<Exercise> {
    Ok(Exercise {
        id: r.get(base)?,
        name: r.get(base + 1)?,
        muscle_focus: r.get(base + 2)?,
        exercise_type: r.get(base + 3)?,
        equipment: r.get(base + 4)?,
        movement_pattern: r.get(base + 5)?,
        archived: r.get::<_, i64>(base + 6)? != 0,
        description: r.get(base + 7)?,
        notes: r.get(base + 8)?,
        favorite: r.get::<_, i64>(base + 9)? != 0,
        custom: r.get::<_, i64>(base + 10)? != 0,
    })
}

const EX_COLS: &str = "e.id, e.name, e.muscle_focus, e.exercise_type, e.equipment, e.movement_pattern, e.archived, \
     e.description, e.notes, e.is_favorite, e.is_custom";
/// Quantidade de colunas de EX_COLS (para ler colunas extras após o exercício).
const EX_N: usize = 11;

fn row_plan(r: &Row) -> rusqlite::Result<Plan> {
    Ok(Plan {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        schedule_type: ScheduleType::parse(&r.get::<_, String>(3)?),
        active: r.get::<_, i64>(4)? != 0,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
        favorite: r.get::<_, i64>(7)? != 0,
    })
}

const PLAN_COLS: &str = "id, name, description, schedule_type, active, created_at, updated_at, is_favorite";

/// Filtro de busca da biblioteca de exercícios (RF-09, 4.3).
#[derive(Debug, Clone, Default)]
pub struct ExerciseFilter {
    pub text: String,
    pub muscle: String,
    pub equipment: String,
    pub pattern: String,
    pub exercise_type: String,
    pub favorites_only: bool,
    pub custom_only: bool,
    /// Incluir arquivados junto com os ativos.
    pub include_archived: bool,
    /// Mostrar somente arquivados.
    pub archived_only: bool,
}

impl ExerciseFilter {
    /// Busca textual sem acentos (feita em Rust; os demais filtros vão no SQL).
    fn text_matches(&self, e: &Exercise) -> bool {
        if self.text.trim().is_empty() {
            return true;
        }
        let hay = normalize(&format!(
            "{} {} {} {} {} {}",
            e.name, e.muscle_focus, e.exercise_type, e.equipment, e.movement_pattern, e.description
        ));
        normalize(&self.text).split_whitespace().all(|w| hay.contains(w))
    }
}

/// Página de resultados da biblioteca (carregamento incremental — 4.3).
#[derive(Debug, Clone, Default)]
pub struct ExercisePage {
    pub items: Vec<Exercise>,
    pub total: usize,
}

impl Db {
    // ----------------------------------------------------------------------
    // Biblioteca de exercícios
    // ----------------------------------------------------------------------

    pub fn list_exercises(&self, filter: &ExerciseFilter) -> AppResult<Vec<Exercise>> {
        Ok(self.list_exercises_page(filter, usize::MAX)?.items)
    }

    /// Lista com filtros aplicados no SQL e até `limit` itens (o total é informado).
    pub fn list_exercises_page(&self, filter: &ExerciseFilter, limit: usize) -> AppResult<ExercisePage> {
        let mut conds: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        if filter.archived_only {
            conds.push("e.archived = 1".into());
        } else if !filter.include_archived {
            conds.push("e.archived = 0".into());
        }
        if filter.favorites_only {
            conds.push("e.is_favorite = 1".into());
        }
        if filter.custom_only {
            conds.push("e.is_custom = 1".into());
        }
        for (col, v) in [
            ("muscle_focus", &filter.muscle),
            ("equipment", &filter.equipment),
            ("movement_pattern", &filter.pattern),
            ("exercise_type", &filter.exercise_type),
        ] {
            if !v.trim().is_empty() {
                args.push(v.trim().to_string());
                conds.push(format!("e.{col} = ?{} COLLATE NOCASE", args.len()));
            }
        }
        let where_ = if conds.is_empty() { String::new() } else { format!("WHERE {}", conds.join(" AND ")) };
        let mut st = self.conn.prepare(&format!(
            "SELECT {EX_COLS} FROM exercise e {where_} ORDER BY e.is_favorite DESC, e.name COLLATE NOCASE"
        ))?;
        let rows = st
            .query_map(rusqlite::params_from_iter(args.iter()), |r| row_exercise(r, 0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut v: Vec<Exercise> = rows.into_iter().filter(|e| filter.text_matches(e)).collect();
        v.sort_by_key(|e| (!e.favorite, normalize(&e.name)));
        let total = v.len();
        v.truncate(limit);
        Ok(ExercisePage { items: v, total })
    }

    pub fn set_exercise_favorite(&self, id: Id, favorite: bool) -> AppResult<()> {
        let n = self.conn.execute(
            "UPDATE exercise SET is_favorite = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, favorite as i64, now_ms()],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("Exercício não encontrado.".into()));
        }
        Ok(())
    }

    /// Favoritar não altera o conteúdo da ficha (apenas o marcador).
    pub fn set_plan_favorite(&self, id: Id, favorite: bool) -> AppResult<()> {
        let n = self.conn.execute("UPDATE workout_plan SET is_favorite = ?2 WHERE id = ?1", params![id, favorite as i64])?;
        if n == 0 {
            return Err(AppError::NotFound("Ficha não encontrada.".into()));
        }
        Ok(())
    }

    pub fn favorite_plans(&self) -> AppResult<Vec<Plan>> {
        Ok(self.list_plans()?.into_iter().filter(|p| p.favorite).collect())
    }

    pub fn get_exercise(&self, id: Id) -> AppResult<Exercise> {
        self.conn
            .query_row(&format!("SELECT {EX_COLS} FROM exercise e WHERE e.id = ?1"), params![id], |r| {
                row_exercise(r, 0)
            })
            .optional()?
            .ok_or_else(|| AppError::NotFound("Exercício não encontrado.".into()))
    }

    pub fn find_exercise_by_name(&self, name: &str) -> AppResult<Option<Exercise>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {EX_COLS} FROM exercise e WHERE e.name = ?1 COLLATE NOCASE"),
                params![name.trim()],
                |r| row_exercise(r, 0),
            )
            .optional()?)
    }

    pub fn create_exercise(&self, input: &ExerciseInput) -> AppResult<Id> {
        let v = input.validated()?;
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO exercise(name, muscle_focus, exercise_type, equipment, movement_pattern, description, notes,
               is_custom, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8)",
            params![v.name, v.muscle_focus, v.exercise_type, v.equipment, v.movement_pattern, v.description, v.notes, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Atualiza dados de catálogo. Sessões passadas guardam seus próprios nomes (histórico intacto).
    pub fn update_exercise(&self, id: Id, input: &ExerciseInput) -> AppResult<()> {
        let v = input.validated()?;
        let n = self.conn.execute(
            "UPDATE exercise SET name=?2, muscle_focus=?3, exercise_type=?4, equipment=?5, movement_pattern=?6,
               description=?7, notes=?8, updated_at=?9 WHERE id = ?1",
            params![id, v.name, v.muscle_focus, v.exercise_type, v.equipment, v.movement_pattern, v.description, v.notes, now_ms()],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("Exercício não encontrado.".into()));
        }
        Ok(())
    }

    /// Arquiva (não apaga) para preservar referências históricas.
    pub fn set_exercise_archived(&self, id: Id, archived: bool) -> AppResult<()> {
        if archived {
            let used: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM plan_exercise WHERE exercise_id = ?1",
                params![id],
                |r| r.get(0),
            )?;
            if used > 0 {
                return Err(AppError::Conflict(
                    "Este exercício está em uma ficha. Remova-o das fichas antes de arquivar.".into(),
                ));
            }
        }
        self.conn.execute(
            "UPDATE exercise SET archived = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, archived as i64, now_ms()],
        )?;
        Ok(())
    }

    /// Busca nome exato (sem diferenciar maiúsculas) ou cria um novo exercício.
    pub fn find_or_create_exercise(&self, input: &ExerciseInput) -> AppResult<Id> {
        let v = input.validated()?;
        if let Some(e) = self.find_exercise_by_name(&v.name)? {
            if e.archived {
                self.set_exercise_archived(e.id, false)?;
            }
            return Ok(e.id);
        }
        self.create_exercise(&v)
    }

    /// Valores distintos de um campo, para filtros (grupo muscular, equipamento, padrão).
    pub fn exercise_field_values(&self, field: &str) -> AppResult<Vec<String>> {
        let col = match field {
            "muscle" => "muscle_focus",
            "equipment" => "equipment",
            "pattern" => "movement_pattern",
            "type" => "exercise_type",
            _ => return Ok(vec![]),
        };
        let mut st = self.conn.prepare(&format!(
            "SELECT DISTINCT {col} FROM exercise WHERE archived = 0 AND {col} <> '' ORDER BY {col} COLLATE NOCASE"
        ))?;
        let v = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok(v)
    }

    /// Alternativas sugeridas (RF-09): cadastradas + mesmo grupo muscular.
    /// Nunca são apresentadas como equivalentes — apenas como sugestões organizacionais.
    pub fn suggested_alternatives(&self, exercise_id: Id) -> AppResult<Vec<(Exercise, String)>> {
        let base = self.get_exercise(exercise_id)?;
        let mut out: Vec<(Exercise, String)> = Vec::new();
        let mut st = self.conn.prepare(&format!(
            "SELECT {EX_COLS}, a.reason FROM exercise_alternative a JOIN exercise e ON e.id = a.alternative_id
             WHERE a.exercise_id = ?1 AND e.archived = 0 ORDER BY e.name COLLATE NOCASE"
        ))?;
        for r in st.query_map(params![exercise_id], |r| Ok((row_exercise(r, 0)?, r.get::<_, String>(EX_N)?)))? {
            out.push(r?);
        }
        if !base.muscle_focus.is_empty() {
            let f = ExerciseFilter { muscle: base.muscle_focus.clone(), ..Default::default() };
            for e in self.list_exercises(&f)? {
                if e.id != exercise_id && !out.iter().any(|(x, _)| x.id == e.id) {
                    out.push((e, "Mesmo grupo muscular".into()));
                }
            }
        }
        Ok(out)
    }

    /// Alternativas cadastradas explicitamente para o exercício.
    pub fn list_alternatives(&self, exercise_id: Id) -> AppResult<Vec<(Exercise, String)>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {EX_COLS}, a.reason FROM exercise_alternative a JOIN exercise e ON e.id = a.alternative_id
             WHERE a.exercise_id = ?1 ORDER BY e.name COLLATE NOCASE"
        ))?;
        let v = st
            .query_map(params![exercise_id], |r| Ok((row_exercise(r, 0)?, r.get::<_, String>(EX_N)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn remove_alternative(&self, exercise_id: Id, alternative_id: Id) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM exercise_alternative WHERE exercise_id = ?1 AND alternative_id = ?2",
            params![exercise_id, alternative_id],
        )?;
        Ok(())
    }

    pub fn add_alternative(&self, exercise_id: Id, alternative_id: Id, reason: &str) -> AppResult<()> {
        if exercise_id == alternative_id {
            return validation("Um exercício não pode ser alternativa de si mesmo.");
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO exercise_alternative(exercise_id, alternative_id, reason) VALUES (?1, ?2, ?3)",
            params![exercise_id, alternative_id, clean_text(reason, 120)],
        )?;
        Ok(())
    }

    // ----------------------------------------------------------------------
    // Fichas
    // ----------------------------------------------------------------------

    pub fn list_plans(&self) -> AppResult<Vec<Plan>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {PLAN_COLS} FROM workout_plan ORDER BY is_favorite DESC, active DESC, name COLLATE NOCASE"
        ))?;
        let v = st.query_map([], row_plan)?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn get_plan(&self, id: Id) -> AppResult<Plan> {
        self.conn
            .query_row(&format!("SELECT {PLAN_COLS} FROM workout_plan WHERE id = ?1"), params![id], row_plan)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Ficha não encontrada.".into()))
    }

    pub fn find_plan_by_name(&self, name: &str) -> AppResult<Option<Plan>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {PLAN_COLS} FROM workout_plan WHERE name = ?1 COLLATE NOCASE"),
                params![name.trim()],
                row_plan,
            )
            .optional()?)
    }

    pub fn create_plan(&self, name: &str, description: &str, schedule: ScheduleType) -> AppResult<Id> {
        let name = clean_text(name, 80);
        if name.is_empty() {
            return validation("Informe o nome da ficha.");
        }
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO workout_plan(name, description, schedule_type, active, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            params![name, clean_notes(description), schedule.as_str(), now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_plan(&self, id: Id, name: &str, description: &str, schedule: ScheduleType) -> AppResult<()> {
        let name = clean_text(name, 80);
        if name.is_empty() {
            return validation("Informe o nome da ficha.");
        }
        let n = self.conn.execute(
            "UPDATE workout_plan SET name=?2, description=?3, schedule_type=?4, updated_at=?5 WHERE id=?1",
            params![id, name, clean_notes(description), schedule.as_str(), now_ms()],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("Ficha não encontrada.".into()));
        }
        Ok(())
    }

    pub fn set_plan_active(&self, id: Id, active: bool) -> AppResult<()> {
        self.conn.execute(
            "UPDATE workout_plan SET active=?2, updated_at=?3 WHERE id=?1",
            params![id, active as i64, now_ms()],
        )?;
        Ok(())
    }

    fn touch_plan(&self, id: Id) -> AppResult<()> {
        self.conn.execute("UPDATE workout_plan SET updated_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    }

    /// Exclui a ficha. Sessões históricas permanecem (RN-08): as referências viram NULL
    /// e a sessão mantém nome da ficha, snapshot e registros.
    pub fn delete_plan(&mut self, id: Id) -> AppResult<()> {
        let active: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM workout_session WHERE plan_id = ?1 AND status = 'in_progress'",
            params![id],
            |r| r.get(0),
        )?;
        if active > 0 {
            return Err(AppError::Conflict(
                "Há uma sessão em andamento com esta ficha. Finalize ou descarte a sessão antes de excluir.".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        let n = tx.execute("DELETE FROM workout_plan WHERE id = ?1", params![id])?;
        tx.commit()?;
        if n == 0 {
            return Err(AppError::NotFound("Ficha não encontrada.".into()));
        }
        Ok(())
    }

    /// Duplica a ficha com todos os dias e exercícios (RF-01).
    pub fn duplicate_plan(&mut self, id: Id) -> AppResult<Id> {
        let full = self.get_plan_full(id)?;
        let mut name = format!("{} (cópia)", full.plan.name);
        let mut i = 2;
        while self.find_plan_by_name(&name)?.is_some() {
            name = format!("{} (cópia {i})", full.plan.name);
            i += 1;
        }
        let tx = self.conn.transaction()?;
        let now = now_ms();
        tx.execute(
            "INSERT INTO workout_plan(name, description, schedule_type, active, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![name, full.plan.description, full.plan.schedule_type.as_str(), full.plan.active as i64, now],
        )?;
        let new_plan = tx.last_insert_rowid();
        for d in &full.days {
            tx.execute(
                "INSERT INTO plan_schedule(plan_id, name, weekdays, sequence_order) VALUES (?1, ?2, ?3, ?4)",
                params![new_plan, d.day.name, d.day.weekdays, d.day.sequence_order],
            )?;
            let new_day = tx.last_insert_rowid();
            for pe in &d.exercises {
                tx.execute(
                    "INSERT INTO plan_exercise(plan_id, schedule_id, exercise_id, position, target_sets, target_reps_min,
                       target_reps_max, target_load_g, rest_seconds, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        new_plan, new_day, pe.exercise.id, pe.position, pe.target.sets, pe.target.reps_min,
                        pe.target.reps_max, pe.target.load_g, pe.rest_seconds, pe.notes
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(new_plan)
    }

    pub fn get_plan_full(&self, id: Id) -> AppResult<PlanFull> {
        let plan = self.get_plan(id)?;
        let days = self.list_days(id)?;
        let mut out = Vec::with_capacity(days.len());
        for d in days {
            let exercises = self.list_plan_exercises(d.id)?;
            out.push(DayFull { day: d, exercises });
        }
        Ok(PlanFull { plan, days: out })
    }

    pub fn list_plans_full(&self) -> AppResult<Vec<PlanFull>> {
        self.list_plans()?.iter().map(|p| self.get_plan_full(p.id)).collect()
    }

    // ----------------------------------------------------------------------
    // Dias da ficha
    // ----------------------------------------------------------------------

    pub fn list_days(&self, plan_id: Id) -> AppResult<Vec<PlanDay>> {
        let mut st = self.conn.prepare(
            "SELECT id, plan_id, name, weekdays, sequence_order FROM plan_schedule WHERE plan_id = ?1
             ORDER BY sequence_order, id",
        )?;
        let v = st
            .query_map(params![plan_id], |r| {
                Ok(PlanDay {
                    id: r.get(0)?,
                    plan_id: r.get(1)?,
                    name: r.get(2)?,
                    weekdays: r.get::<_, i64>(3)? as u8,
                    sequence_order: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn get_day(&self, day_id: Id) -> AppResult<PlanDay> {
        self.conn
            .query_row(
                "SELECT id, plan_id, name, weekdays, sequence_order FROM plan_schedule WHERE id = ?1",
                params![day_id],
                |r| {
                    Ok(PlanDay {
                        id: r.get(0)?,
                        plan_id: r.get(1)?,
                        name: r.get(2)?,
                        weekdays: r.get::<_, i64>(3)? as u8,
                        sequence_order: r.get(4)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("Dia da ficha não encontrado.".into()))
    }

    pub fn add_day(&self, plan_id: Id, name: &str, weekdays: u8) -> AppResult<Id> {
        let name = clean_text(name, 60);
        if name.is_empty() {
            return validation("Informe o nome do dia (ex.: Treino A, Peito e tríceps).");
        }
        self.get_plan(plan_id)?;
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(sequence_order), -1) + 1 FROM plan_schedule WHERE plan_id = ?1",
            params![plan_id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO plan_schedule(plan_id, name, weekdays, sequence_order) VALUES (?1, ?2, ?3, ?4)",
            params![plan_id, name, (weekdays & 0x7f) as i64, next],
        )?;
        let id = self.conn.last_insert_rowid();
        self.touch_plan(plan_id)?;
        Ok(id)
    }

    pub fn update_day(&self, day_id: Id, name: &str, weekdays: u8) -> AppResult<()> {
        let name = clean_text(name, 60);
        if name.is_empty() {
            return validation("Informe o nome do dia.");
        }
        let d = self.get_day(day_id)?;
        self.conn.execute(
            "UPDATE plan_schedule SET name=?2, weekdays=?3 WHERE id=?1",
            params![day_id, name, (weekdays & 0x7f) as i64],
        )?;
        self.touch_plan(d.plan_id)
    }

    pub fn delete_day(&mut self, day_id: Id) -> AppResult<()> {
        let d = self.get_day(day_id)?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM plan_schedule WHERE id = ?1", params![day_id])?;
        tx.commit()?;
        self.renumber_days(d.plan_id)?;
        self.touch_plan(d.plan_id)
    }

    fn renumber_days(&self, plan_id: Id) -> AppResult<()> {
        let days = self.list_days(plan_id)?;
        for (i, d) in days.iter().enumerate() {
            self.conn.execute("UPDATE plan_schedule SET sequence_order=?2 WHERE id=?1", params![d.id, i as i64])?;
        }
        Ok(())
    }

    /// Move o dia uma posição para cima (-1) ou para baixo (+1) na sequência.
    pub fn move_day(&mut self, day_id: Id, delta: i64) -> AppResult<()> {
        let d = self.get_day(day_id)?;
        self.renumber_days(d.plan_id)?;
        let mut days = self.list_days(d.plan_id)?;
        let idx = days.iter().position(|x| x.id == day_id).unwrap_or(0) as i64;
        let target = (idx + delta).clamp(0, days.len() as i64 - 1);
        if target == idx {
            return Ok(());
        }
        let item = days.remove(idx as usize);
        days.insert(target as usize, item);
        let tx = self.conn.transaction()?;
        for (i, x) in days.iter().enumerate() {
            tx.execute("UPDATE plan_schedule SET sequence_order=?2 WHERE id=?1", params![x.id, i as i64])?;
        }
        tx.commit()?;
        self.touch_plan(d.plan_id)
    }

    // ----------------------------------------------------------------------
    // Exercícios da ficha
    // ----------------------------------------------------------------------

    pub fn list_plan_exercises(&self, day_id: Id) -> AppResult<Vec<PlanExercise>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT pe.id, pe.plan_id, pe.schedule_id, pe.position, pe.target_sets, pe.target_reps_min, pe.target_reps_max,
                    pe.target_load_g, pe.rest_seconds, pe.notes, {EX_COLS}
             FROM plan_exercise pe JOIN exercise e ON e.id = pe.exercise_id
             WHERE pe.schedule_id = ?1 ORDER BY pe.position, pe.id"
        ))?;
        let v = st
            .query_map(params![day_id], |r| {
                Ok(PlanExercise {
                    id: r.get(0)?,
                    plan_id: r.get(1)?,
                    schedule_id: r.get(2)?,
                    position: r.get(3)?,
                    target: Target { sets: r.get(4)?, reps_min: r.get(5)?, reps_max: r.get(6)?, load_g: r.get(7)? },
                    rest_seconds: r.get(8)?,
                    notes: r.get(9)?,
                    exercise: row_exercise(r, 10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn get_plan_exercise(&self, id: Id) -> AppResult<PlanExercise> {
        let day_id: Id = self
            .conn
            .query_row("SELECT schedule_id FROM plan_exercise WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::NotFound("Exercício da ficha não encontrado.".into()))?;
        self.list_plan_exercises(day_id)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::NotFound("Exercício da ficha não encontrado.".into()))
    }

    pub fn add_plan_exercise(
        &self,
        day_id: Id,
        exercise_id: Id,
        target: Target,
        rest_seconds: i64,
        notes: &str,
    ) -> AppResult<Id> {
        let t = target.validated()?;
        let rest = validate_rest(rest_seconds)?;
        let day = self.get_day(day_id)?;
        let ex = self.get_exercise(exercise_id)?;
        if ex.archived {
            return validation("Este exercício está arquivado.");
        }
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM plan_exercise WHERE schedule_id = ?1",
            params![day_id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO plan_exercise(plan_id, schedule_id, exercise_id, position, target_sets, target_reps_min,
               target_reps_max, target_load_g, rest_seconds, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![day.plan_id, day_id, exercise_id, next, t.sets, t.reps_min, t.reps_max, t.load_g, rest, clean_notes(notes)],
        )?;
        let id = self.conn.last_insert_rowid();
        self.touch_plan(day.plan_id)?;
        Ok(id)
    }

    pub fn update_plan_exercise(
        &self,
        id: Id,
        exercise_id: Id,
        target: Target,
        rest_seconds: i64,
        notes: &str,
    ) -> AppResult<()> {
        let t = target.validated()?;
        let rest = validate_rest(rest_seconds)?;
        let pe = self.get_plan_exercise(id)?;
        self.get_exercise(exercise_id)?;
        self.conn.execute(
            "UPDATE plan_exercise SET exercise_id=?2, target_sets=?3, target_reps_min=?4, target_reps_max=?5,
               target_load_g=?6, rest_seconds=?7, notes=?8 WHERE id=?1",
            params![id, exercise_id, t.sets, t.reps_min, t.reps_max, t.load_g, rest, clean_notes(notes)],
        )?;
        self.touch_plan(pe.plan_id)
    }

    pub fn delete_plan_exercise(&self, id: Id) -> AppResult<()> {
        let pe = self.get_plan_exercise(id)?;
        self.conn.execute("DELETE FROM plan_exercise WHERE id = ?1", params![id])?;
        self.touch_plan(pe.plan_id)
    }

    pub fn move_plan_exercise(&mut self, id: Id, delta: i64) -> AppResult<()> {
        let pe = self.get_plan_exercise(id)?;
        let list = self.list_plan_exercises(pe.schedule_id)?;
        let idx = list.iter().position(|x| x.id == id).unwrap_or(0) as i64;
        self.move_plan_exercise_to(id, (idx + delta).max(0) as usize)
    }

    /// Move o exercício da ficha para a posição `index` (0 = primeiro), em uma transação (4.4).
    /// Não altera sessões históricas: a ordem delas fica em `session_exercise.position`.
    pub fn move_plan_exercise_to(&mut self, id: Id, index: usize) -> AppResult<()> {
        let pe = self.get_plan_exercise(id)?;
        let mut list: Vec<Id> = self.list_plan_exercises(pe.schedule_id)?.iter().map(|x| x.id).collect();
        let from = list.iter().position(|x| *x == id).ok_or_else(|| AppError::NotFound("Exercício não encontrado.".into()))?;
        let item = list.remove(from);
        list.insert(index.min(list.len()), item);
        let tx = self.conn.transaction()?;
        for (i, x) in list.iter().enumerate() {
            tx.execute("UPDATE plan_exercise SET position=?2 WHERE id=?1", params![x, i as i64])?;
        }
        tx.commit()?;
        self.touch_plan(pe.plan_id)
    }

    /// Move o dia da ficha para a posição `index` na sequência, em uma transação (4.4).
    pub fn move_day_to(&mut self, day_id: Id, index: usize) -> AppResult<()> {
        let d = self.get_day(day_id)?;
        let mut list: Vec<Id> = self.list_days(d.plan_id)?.iter().map(|x| x.id).collect();
        let from = list.iter().position(|x| *x == day_id).unwrap_or(0);
        let item = list.remove(from);
        list.insert(index.min(list.len()), item);
        let tx = self.conn.transaction()?;
        for (i, x) in list.iter().enumerate() {
            tx.execute("UPDATE plan_schedule SET sequence_order=?2 WHERE id=?1", params![x, i as i64])?;
        }
        tx.commit()?;
        self.touch_plan(d.plan_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_ignores_accents() {
        let db = Db::open_in_memory().unwrap();
        let f = ExerciseFilter { text: "supino maquina".into(), ..Default::default() };
        let r = db.list_exercises(&f).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "Supino na máquina");
    }

    #[test]
    fn filters_and_favorites() {
        let db = Db::open_in_memory().unwrap();
        let all = db.list_exercises(&ExerciseFilter::default()).unwrap();
        assert!(all.iter().all(|e| !e.custom));
        let id = db
            .create_exercise(&ExerciseInput { name: "Meu exercício".into(), muscle_focus: "Peito".into(), description: "Cotovelos a 45°".into(), ..Default::default() })
            .unwrap();
        assert!(db.get_exercise(id).unwrap().custom);
        let f = ExerciseFilter { custom_only: true, ..Default::default() };
        assert_eq!(db.list_exercises(&f).unwrap().len(), 1);
        let f = ExerciseFilter { text: "cotovelos".into(), ..Default::default() }; // busca na descrição
        assert_eq!(db.list_exercises(&f).unwrap()[0].id, id);
        db.set_exercise_favorite(id, true).unwrap();
        let fav = ExerciseFilter { favorites_only: true, ..Default::default() };
        assert_eq!(db.list_exercises(&fav).unwrap().len(), 1);
        assert_eq!(db.list_exercises(&ExerciseFilter::default()).unwrap()[0].id, id); // favoritos primeiro
        let page = db.list_exercises_page(&ExerciseFilter::default(), 5).unwrap();
        assert_eq!(page.items.len(), 5);
        assert!(page.total > 50);
        let t = ExerciseFilter { exercise_type: "isolado".into(), muscle: "peito".into(), ..Default::default() };
        assert!(db.list_exercises(&t).unwrap().iter().all(|e| e.exercise_type == "Isolado" && e.muscle_focus == "Peito"));
        db.set_exercise_archived(id, true).unwrap();
        let arch = ExerciseFilter { archived_only: true, ..Default::default() };
        assert_eq!(db.list_exercises(&arch).unwrap().len(), 1);
        let p = db.create_plan("P", "", ScheduleType::Weekday).unwrap();
        db.set_plan_favorite(p, true).unwrap();
        assert_eq!(db.favorite_plans().unwrap().len(), 1);
    }

    #[test]
    fn move_to_position() {
        let mut db = Db::open_in_memory().unwrap();
        let plan = db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        let day = db.add_day(plan, "A", 0).unwrap();
        let t = Target { sets: 1, reps_min: 1, reps_max: 1, load_g: 0 };
        let names = ["Prancha", "Esteira", "Mesa flexora", "Cadeira extensora"];
        let ids: Vec<Id> = names
            .iter()
            .map(|n| db.add_plan_exercise(day, db.find_exercise_by_name(n).unwrap().unwrap().id, t, 0, "").unwrap())
            .collect();
        db.move_plan_exercise_to(ids[3], 0).unwrap();
        db.move_plan_exercise_to(ids[0], 99).unwrap(); // além do fim = último
        let order: Vec<Id> = db.list_plan_exercises(day).unwrap().iter().map(|x| x.id).collect();
        assert_eq!(order, vec![ids[3], ids[1], ids[2], ids[0]]);
        let b = db.add_day(plan, "B", 0).unwrap();
        let c = db.add_day(plan, "C", 0).unwrap();
        db.move_day_to(c, 0).unwrap();
        let days: Vec<Id> = db.list_days(plan).unwrap().iter().map(|d| d.id).collect();
        assert_eq!(days, vec![c, day, b]);
    }

    #[test]
    fn manual_alternatives() {
        let db = Db::open_in_memory().unwrap();
        let a = db.find_exercise_by_name("Prancha").unwrap().unwrap().id;
        let b = db.find_exercise_by_name("Esteira").unwrap().unwrap().id;
        assert!(db.add_alternative(a, a, "").is_err());
        let before = db.list_alternatives(a).unwrap().len(); // sementes do mesmo grupo/padrão
        db.add_alternative(a, b, "Cadastrada pelo usuário").unwrap();
        db.add_alternative(a, b, "dup").unwrap(); // ignorado
        let l = db.list_alternatives(a).unwrap();
        assert_eq!(l.len(), before + 1);
        assert!(l.iter().any(|(e, r)| e.id == b && r == "Cadastrada pelo usuário"));
        assert!(db.suggested_alternatives(a).unwrap().iter().any(|(e, _)| e.id == b));
        db.remove_alternative(a, b).unwrap();
        assert_eq!(db.list_alternatives(a).unwrap().len(), before);
    }

    #[test]
    fn alternatives_are_suggestions() {
        let db = Db::open_in_memory().unwrap();
        let e = db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap();
        let alts = db.suggested_alternatives(e.id).unwrap();
        assert!(alts.iter().any(|(a, _)| a.name == "Supino reto com halteres"));
        assert!(alts.iter().all(|(a, _)| a.id != e.id));
    }
}
