//! Operações reversíveis ("Desfazer", documento EMPETO 3.6).
//! Cada operação guarda os dados necessários para reverter — não só uma mensagem.
//! Nunca contorna a imutabilidade: se a sessão já foi concluída, o banco recusa e a UI não oferece.

use crate::domain::*;
use crate::persistence::Db;

#[derive(Debug, Clone, PartialEq)]
pub enum UndoKind {
    /// Registro (ou novo registro) de série; `before` é o estado anterior completo.
    RecordSet { set_id: Id, before: SessionSet },
    /// "Desfazer registro" da série; `before` é o estado registrado.
    ClearSet { set_id: Id, before: SessionSet },
    /// Remoção da última série (ainda não registrada) de um exercício da sessão.
    RemoveSet { session_exercise_id: Id },
    /// Exercício removido da ficha.
    DeletePlanExercise { day_id: Id, exercise_id: Id, target: Target, rest_seconds: i64, notes: String, position: usize },
    /// Exercício arquivado/reativado na biblioteca.
    ArchiveExercise { exercise_id: Id, was_archived: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoOp {
    pub kind: UndoKind,
    pub label: String,
    pub at_ms: i64,
}

/// Janela em que o aviso "Desfazer" fica disponível.
pub const UNDO_WINDOW_MS: i64 = 5_000;

impl UndoOp {
    pub fn new(kind: UndoKind, label: impl Into<String>) -> UndoOp {
        UndoOp { kind, label: label.into(), at_ms: now_ms() }
    }

    pub fn expired(&self, now: i64) -> bool {
        now - self.at_ms > UNDO_WINDOW_MS
    }

    /// Reverte a operação. Erros (ex.: sessão já concluída) são devolvidos e nada é alterado.
    pub fn revert(&self, db: &mut Db) -> AppResult<()> {
        match &self.kind {
            UndoKind::RecordSet { set_id, before } | UndoKind::ClearSet { set_id, before } => db.restore_set_state(
                *set_id,
                before.actual_reps,
                before.actual_load_g,
                before.completed_at,
                before.rest_taken_s,
            ),
            UndoKind::RemoveSet { session_exercise_id } => db.add_session_set(*session_exercise_id).map(|_| ()),
            UndoKind::DeletePlanExercise { day_id, exercise_id, target, rest_seconds, notes, position } => {
                let id = db.add_plan_exercise(*day_id, *exercise_id, *target, *rest_seconds, notes)?;
                db.move_plan_exercise_to(id, *position)
            }
            UndoKind::ArchiveExercise { exercise_id, was_archived } => {
                db.set_exercise_archived(*exercise_id, *was_archived)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Db, Id, Id) {
        let mut db = Db::open_in_memory().unwrap();
        let plan = db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        let day = db.add_day(plan, "A", 0).unwrap();
        let ex = db.find_exercise_by_name("Prancha").unwrap().unwrap().id;
        let ex2 = db.find_exercise_by_name("Esteira").unwrap().unwrap().id;
        let t = Target { sets: 2, reps_min: 10, reps_max: 10, load_g: 0 };
        db.add_plan_exercise(day, ex, t, 60, "").unwrap();
        db.add_plan_exercise(day, ex2, t, 60, "obs").unwrap();
        let sid = db.start_session(day).unwrap();
        (db, sid, day)
    }

    #[test]
    fn undo_record_and_clear() {
        let (mut db, sid, _) = setup();
        let set = db.get_session_full(sid).unwrap().exercises[0].sets[0].clone();
        let op = UndoOp::new(UndoKind::RecordSet { set_id: set.id, before: set.clone() }, "Série registrada");
        db.record_set(set.id, 12, 5_000).unwrap();
        op.revert(&mut db).unwrap();
        assert_eq!(db.get_session_set(set.id).unwrap(), set);

        db.record_set(set.id, 12, 5_000).unwrap();
        let rec = db.get_session_set(set.id).unwrap();
        let op = UndoOp::new(UndoKind::ClearSet { set_id: set.id, before: rec.clone() }, "Registro desfeito");
        db.clear_set(set.id).unwrap();
        op.revert(&mut db).unwrap();
        assert_eq!(db.get_session_set(set.id).unwrap(), rec);
    }

    #[test]
    fn undo_cannot_touch_completed_session() {
        let (mut db, sid, _) = setup();
        let set = db.get_session_full(sid).unwrap().exercises[0].sets[0].clone();
        let op = UndoOp::new(UndoKind::RecordSet { set_id: set.id, before: set.clone() }, "x");
        db.record_set(set.id, 12, 5_000).unwrap();
        db.finish_session(sid, "").unwrap();
        assert!(op.revert(&mut db).is_err());
        assert!(db.get_session_set(set.id).unwrap().completed_at.is_some());
    }

    #[test]
    fn undo_delete_plan_exercise_restores_position() {
        let (mut db, _sid, day) = setup();
        let list = db.list_plan_exercises(day).unwrap();
        let first = list[0].clone();
        let op = UndoOp::new(
            UndoKind::DeletePlanExercise {
                day_id: day,
                exercise_id: first.exercise.id,
                target: first.target,
                rest_seconds: first.rest_seconds,
                notes: first.notes.clone(),
                position: 0,
            },
            "Removido",
        );
        db.delete_plan_exercise(first.id).unwrap();
        op.revert(&mut db).unwrap();
        let after = db.list_plan_exercises(day).unwrap();
        assert_eq!(after[0].exercise.id, first.exercise.id);
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn fill_values_and_progress() {
        let (db, sid, _) = setup();
        let f = db.get_session_full(sid).unwrap();
        let (s1, s2) = (f.exercises[0].sets[0].id, f.exercises[0].sets[1].id);
        assert_eq!(db.previous_set_values(s2).unwrap(), None);
        db.record_set(s1, 9, 2_500).unwrap();
        assert_eq!(db.previous_set_values(s2).unwrap(), Some((2_500, 9)));
        assert_eq!(db.planned_set_values(s2).unwrap(), (0, 10));
        let f = db.get_session_full(sid).unwrap();
        assert_eq!(f.exercises[0].progress(), ExerciseProgress::Partial);
        assert_eq!(f.exercises[1].progress(), ExerciseProgress::NotStarted);
        // preencher não registra
        assert!(db.get_session_set(s2).unwrap().completed_at.is_none());
    }

    #[test]
    fn expiry() {
        let mut op = UndoOp::new(UndoKind::RemoveSet { session_exercise_id: 1 }, "x");
        op.at_ms = 0;
        assert!(op.expired(UNDO_WINDOW_MS + 1));
        assert!(!op.expired(UNDO_WINDOW_MS));
    }
}
