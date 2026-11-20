//! Sugestão de progressão de carga (documento EMPETO, 5.4) — `ProgressionSuggestionService`.
//!
//! Regra: se, na última sessão concluída daquele exercício da ficha, todas as séries planejadas
//! foram feitas com pelo menos a carga-alvo e atingiram o topo da faixa de repetições, sugere
//! aumentar a carga pelo incremento configurado. É só uma sugestão: nunca altera a ficha sozinha.

use crate::domain::*;
use crate::persistence::Db;
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressionSuggestion {
    pub plan_exercise_id: Id,
    /// Sessão que fundamentou a sugestão (usada para "ignorar" até a próxima sessão).
    pub based_on_session: Id,
    pub current_load_g: i64,
    pub suggested_load_g: i64,
    pub justification: String,
    /// Valores usados no cálculo (repetições por série).
    pub reps_done: Vec<i64>,
}

/// Núcleo puro da regra (testável sem banco).
pub fn suggest(target: Target, step_g: i64, results: &[(i64, i64)]) -> Option<i64> {
    if step_g <= 0 || target.reps_max <= 0 {
        return None;
    }
    // Dados insuficientes: menos séries do que o planejado.
    if (results.len() as i64) < target.sets {
        return None;
    }
    let ok = results.iter().take(target.sets as usize).all(|(load, reps)| *reps >= target.reps_max && *load >= target.load_g);
    if !ok {
        return None;
    }
    Some((target.load_g + step_g).min(MAX_LOAD_G))
}

impl Db {
    pub fn progression_for(&self, plan_exercise_id: Id, step_g: i64, unit: LoadUnit) -> AppResult<Option<ProgressionSuggestion>> {
        let pe = self.get_plan_exercise(plan_exercise_id)?;
        let last: Option<(Id, Id)> = self
            .conn
            .query_row(
                "SELECT e.id, s.id FROM session_exercise e JOIN workout_session s ON s.id = e.session_id
                 WHERE e.plan_exercise_id = ?1 AND s.status = 'completed' AND e.performed_exercise_id = ?2
                 ORDER BY s.started_at DESC LIMIT 1",
                params![plan_exercise_id, pe.exercise.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((se_id, session_id)) = last else { return Ok(None) };
        let mut st = self.conn.prepare(
            "SELECT COALESCE(actual_load_g,0), COALESCE(actual_reps,0) FROM session_set
             WHERE session_exercise_id = ?1 AND completed_at IS NOT NULL AND set_kind <> 'warmup' ORDER BY set_number",
        )?;
        let results = st.query_map(params![se_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<(i64, i64)>, _>>()?;
        // Séries de aquecimento planejadas não entram na regra (evolução 8.1).
        let warmups = (1..=pe.target.sets).filter(|n| pe.kind_of(*n) == SetKind::Warmup).count() as i64;
        let work = Target { sets: (pe.target.sets - warmups).max(1), ..pe.target };
        let Some(new_load) = suggest(work, step_g, &results) else { return Ok(None) };
        let reps: Vec<i64> = results.iter().map(|r| r.1).collect();
        Ok(Some(ProgressionSuggestion {
            plan_exercise_id,
            based_on_session: session_id,
            current_load_g: pe.target.load_g,
            suggested_load_g: new_load,
            justification: trf!(
                "Na última sessão você fez {} série(s) com {} reps ({}), no topo da faixa {}.",
                results.len(),
                reps.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(", "),
                format_load_unit(results.iter().map(|r| r.0).min().unwrap_or(0), unit),
                format_reps(pe.target.reps_min, pe.target.reps_max)
            ),
            reps_done: reps,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Target = Target { sets: 3, reps_min: 8, reps_max: 12, load_g: 40_000 };

    #[test]
    fn rule() {
        assert_eq!(suggest(T, 2_500, &[(40_000, 12), (40_000, 12), (40_000, 12)]), Some(42_500));
        assert_eq!(suggest(T, 2_500, &[(40_000, 12), (40_000, 11), (40_000, 12)]), None);
        assert_eq!(suggest(T, 2_500, &[(40_000, 12), (40_000, 12)]), None); // insuficiente
        assert_eq!(suggest(T, 2_500, &[(37_500, 12), (40_000, 12), (40_000, 12)]), None); // carga abaixo do alvo
        assert_eq!(suggest(T, 0, &[(40_000, 12); 3]), None);
    }

    #[test]
    fn from_history_without_changing_plan() {
        let mut db = Db::open_in_memory().unwrap();
        let p = db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        let day = db.add_day(p, "A", 0).unwrap();
        let ex = db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap().id;
        let pe = db.add_plan_exercise(day, ex, T, 90, "").unwrap();
        assert!(db.progression_for(pe, 2_500, LoadUnit::Kg).unwrap().is_none());
        let sid = db.start_session(day).unwrap();
        for s in &db.get_session_full(sid).unwrap().exercises[0].sets {
            db.record_set(s.id, 12, 40_000).unwrap();
        }
        db.finish_session(sid, "").unwrap();
        let sug = db.progression_for(pe, 2_500, LoadUnit::Kg).unwrap().unwrap();
        assert_eq!(sug.suggested_load_g, 42_500);
        assert_eq!(sug.reps_done, vec![12, 12, 12]);
        assert_eq!(db.get_plan_exercise(pe).unwrap().target.load_g, 40_000); // não altera a ficha
    }
}
