//! Recordes pessoais (documento EMPETO, 5.1) — calculados a partir do histórico, sem tabela própria.
//!
//! Regras:
//! - Compara apenas o mesmo exercício (o exercício *realizado*; em substituições o recorde vai para
//!   o exercício que de fato foi executado).
//! - Séries inválidas (não registradas ou com 0 repetições) não contam.
//! - O primeiro registro de um exercício não é recorde (não há marca anterior para superar).
//! - Nada é gravado: sessões antigas nunca são alteradas.

use crate::domain::*;
use crate::persistence::Db;
use rusqlite::params;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    MaxLoad,
    MaxReps,
    SetVolume,
    Estimated1Rm,
}

impl RecordKind {
    pub fn label(self) -> &'static str {
        match self {
            RecordKind::MaxLoad => "maior carga",
            RecordKind::MaxReps => "mais repetições",
            RecordKind::SetVolume => "maior volume em uma série",
            RecordKind::Estimated1Rm => "maior 1RM estimada",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bests {
    pub load_g: i64,
    pub reps: i64,
    pub set_volume: i64,
    pub e1rm_g: i64,
    pub count: usize,
}

impl Bests {
    fn push(&mut self, load: i64, reps: i64) {
        self.load_g = self.load_g.max(load);
        self.reps = self.reps.max(reps);
        self.set_volume = self.set_volume.max(load * reps);
        self.e1rm_g = self.e1rm_g.max(estimated_1rm_g(load, reps));
        self.count += 1;
    }

    /// Recordes que uma série nova (carga, reps) bate em relação a estes melhores.
    pub fn beaten_by(&self, load: i64, reps: i64) -> Vec<RecordKind> {
        if self.count == 0 || reps <= 0 {
            return vec![];
        }
        let mut v = Vec::new();
        if load > self.load_g {
            v.push(RecordKind::MaxLoad);
        }
        if reps > self.reps {
            v.push(RecordKind::MaxReps);
        }
        if load * reps > self.set_volume && load > 0 {
            v.push(RecordKind::SetVolume);
        }
        if estimated_1rm_g(load, reps) > self.e1rm_g && load > 0 && reps > 1 {
            v.push(RecordKind::Estimated1Rm);
        }
        v
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordEvent {
    pub exercise_name: String,
    pub kinds: Vec<RecordKind>,
    pub load_g: i64,
    pub reps: i64,
    pub at_ms: i64,
    pub session_id: Id,
}

/// Chave do exercício: id realizado (ou nome, se o id foi perdido).
fn key_cond(exercise_id: Option<Id>) -> &'static str {
    if exercise_id.is_some() {
        "e.performed_exercise_id = ?1"
    } else {
        "e.performed_exercise_id IS NULL AND e.performed_name = ?1"
    }
}

impl Db {
    /// Melhores marcas do exercício em séries válidas registradas antes de `before_ms`
    /// (sessões concluídas + a sessão em andamento `current_session`, se informada).
    pub fn bests_before(
        &self,
        exercise_id: Option<Id>,
        name: &str,
        before_ms: i64,
        current_session: Option<Id>,
    ) -> AppResult<Bests> {
        let sql = format!(
            "SELECT COALESCE(ss.actual_load_g,0), COALESCE(ss.actual_reps,0)
             FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
             JOIN workout_session s ON s.id = e.session_id
             WHERE {} AND ss.completed_at IS NOT NULL AND ss.completed_at < ?2 AND COALESCE(ss.actual_reps,0) > 0
               AND (s.status = 'completed' OR s.id = ?3)",
            key_cond(exercise_id)
        );
        let key: rusqlite::types::Value = match exercise_id {
            Some(id) => id.into(),
            None => name.to_string().into(),
        };
        let mut st = self.conn.prepare(&sql)?;
        let mut b = Bests::default();
        let rows = st.query_map(params![key, before_ms, current_session.unwrap_or(-1)], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (l, r) = row?;
            b.push(l, r);
        }
        Ok(b)
    }

    /// Recordes batidos pela série recém-registrada (5.1, "Comparar com a série recém-registrada").
    pub fn records_for_set(&self, set_id: Id) -> AppResult<Vec<RecordKind>> {
        let (ex_id, name, sid, load, reps, at): (Option<Id>, String, Id, Option<i64>, Option<i64>, Option<i64>) =
            self.conn.query_row(
                "SELECT e.performed_exercise_id, e.performed_name, e.session_id, ss.actual_load_g, ss.actual_reps, ss.completed_at
                 FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id WHERE ss.id = ?1",
                params![set_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?;
        let (Some(load), Some(reps), Some(at)) = (load, reps, at) else { return Ok(vec![]) };
        let bests = self.bests_before(ex_id, &name, at, Some(sid))?;
        Ok(bests.beaten_by(load, reps))
    }

    /// Recordes alcançados em sessões concluídas desde `since_ms` (cartão da Home, histórico).
    pub fn recent_records(&self, since_ms: i64, exercise_key: Option<&str>) -> AppResult<Vec<RecordEvent>> {
        let mut st = self.conn.prepare(
            "SELECT COALESCE('id:' || e.performed_exercise_id, 'name:' || e.performed_name), e.performed_name,
                    COALESCE(ss.actual_load_g,0), COALESCE(ss.actual_reps,0), ss.completed_at, s.id
             FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
             JOIN workout_session s ON s.id = e.session_id
             WHERE s.status = 'completed' AND ss.completed_at IS NOT NULL AND COALESCE(ss.actual_reps,0) > 0
             ORDER BY ss.completed_at, ss.id",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, Id>(5)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut bests: std::collections::HashMap<String, Bests> = Default::default();
        let mut out = Vec::new();
        for (key, name, load, reps, at, sid) in rows {
            let b = bests.entry(key.clone()).or_default();
            let kinds = b.beaten_by(load, reps);
            b.push(load, reps);
            if !kinds.is_empty() && at >= since_ms && exercise_key.map(|k| k == key).unwrap_or(true) {
                out.push(RecordEvent { exercise_name: name, kinds, load_g: load, reps, at_ms: at, session_id: sid });
            }
        }
        out.reverse();
        Ok(out)
    }

    /// Melhores marcas atuais de um exercício (chave "id:N" ou "name:X").
    pub fn current_bests(&self, key: &str) -> AppResult<Bests> {
        match key.strip_prefix("id:").and_then(|v| v.parse::<Id>().ok()) {
            Some(id) => self.bests_before(Some(id), "", i64::MAX, None),
            None => self.bests_before(None, key.trim_start_matches("name:"), i64::MAX, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(db: &mut Db, day: Id, values: &[(i64, i64)]) -> Id {
        let sid = db.start_session(day).unwrap();
        let f = db.get_session_full(sid).unwrap();
        for (s, (l, r)) in f.exercises[0].sets.iter().zip(values) {
            db.record_set(s.id, *r, *l).unwrap();
        }
        sid
    }

    fn setup() -> (Db, Id) {
        let mut db = Db::open_in_memory().unwrap();
        let p = db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        let day = db.add_day(p, "A", 0).unwrap();
        let ex = db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap().id;
        db.add_plan_exercise(day, ex, Target { sets: 2, reps_min: 8, reps_max: 12, load_g: 40_000 }, 90, "").unwrap();
        let _ = &mut db;
        (db, day)
    }

    #[test]
    fn first_session_is_not_record_then_new_records_detected() {
        let (mut db, day) = setup();
        let s1 = session(&mut db, day, &[(40_000, 10), (40_000, 8)]);
        let sets = db.get_session_full(s1).unwrap().exercises[0].sets.clone();
        assert!(db.records_for_set(sets[0].id).unwrap().is_empty()); // primeira série: sem marca anterior
        assert!(db.records_for_set(sets[1].id).unwrap().is_empty()); // não supera a anterior
        db.finish_session(s1, "").unwrap();
        let s2 = session(&mut db, day, &[(42_500, 10), (40_000, 12)]);
        let sets = db.get_session_full(s2).unwrap().exercises[0].sets.clone();
        let r1 = db.records_for_set(sets[0].id).unwrap();
        assert!(r1.contains(&RecordKind::MaxLoad) && r1.contains(&RecordKind::Estimated1Rm));
        let r2 = db.records_for_set(sets[1].id).unwrap();
        assert!(r2.contains(&RecordKind::MaxReps));
        assert!(!r2.contains(&RecordKind::MaxLoad));
        db.finish_session(s2, "").unwrap();
        let recent = db.recent_records(0, None).unwrap();
        assert_eq!(recent.len(), 2);
        let b = db.current_bests(&format!("id:{}", db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap().id)).unwrap();
        assert_eq!((b.load_g, b.reps), (42_500, 12));
    }

    #[test]
    fn invalid_sets_ignored() {
        let b = Bests { count: 1, load_g: 10, reps: 5, set_volume: 50, e1rm_g: 11 };
        assert!(b.beaten_by(100, 0).is_empty());
        assert!(Bests::default().beaten_by(100, 10).is_empty());
    }
}
