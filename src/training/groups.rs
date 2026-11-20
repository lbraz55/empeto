//! Superséries, trisséries e circuitos (documento de evolução v1.1, 8.2) — `training_groups`.
//!
//! Um grupo é uma sequência de exercícios **consecutivos** com a mesma etiqueta (`group_tag`).
//! Cada exercício continua com suas próprias séries, cargas e repetições; o grupo só muda a ordem de
//! execução: registrar uma série leva ao próximo exercício do grupo sem descanso, e o descanso acontece
//! depois do último exercício do grupo (o descanso configurado nele), voltando ao primeiro para a
//! rodada seguinte.

use crate::domain::*;
use crate::i18n::tr;
use crate::persistence::Db;
use rusqlite::params;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Superset,
    Triset,
    Circuit,
}

impl GroupKind {
    pub fn of_size(n: usize) -> Option<GroupKind> {
        match n {
            0 | 1 => None,
            2 => Some(GroupKind::Superset),
            3 => Some(GroupKind::Triset),
            _ => Some(GroupKind::Circuit),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            GroupKind::Superset => tr("Supersérie"),
            GroupKind::Triset => tr("Trissérie"),
            GroupKind::Circuit => tr("Circuito"),
        }
    }
}

/// `linked[i]` = o exercício i está ligado ao i+1.
pub fn links_of(tags: &[String]) -> Vec<bool> {
    (0..tags.len()).map(|i| i + 1 < tags.len() && !tags[i].is_empty() && tags[i] == tags[i + 1]).collect()
}

/// Recalcula etiquetas a partir das ligações: G1, G2… para grupos de 2+, vazio para exercícios soltos.
pub fn tags_from_links(links: &[bool]) -> Vec<String> {
    let n = links.len();
    let mut out = vec![String::new(); n];
    let mut g = 0;
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && links[j] {
            j += 1;
        }
        if j > i {
            g += 1;
            for t in out.iter_mut().take(j + 1).skip(i) {
                *t = format!("G{g}");
            }
        }
        i = j + 1;
    }
    out
}

/// Normaliza etiquetas (grupos só com vizinhos, numeração estável pela ordem).
pub fn normalize(tags: &[String]) -> Vec<String> {
    tags_from_links(&links_of(tags))
}

/// Liga/desliga o exercício `i` ao seguinte.
pub fn toggle_link(tags: &[String], i: usize) -> Vec<String> {
    let mut links = links_of(tags);
    if i + 1 < links.len() {
        links[i] = !links[i];
    }
    tags_from_links(&links)
}

/// Índices (início, fim inclusive) do grupo que contém `i`.
pub fn members(tags: &[String], i: usize) -> (usize, usize) {
    if i >= tags.len() || tags[i].is_empty() {
        return (i, i);
    }
    let mut a = i;
    while a > 0 && tags[a - 1] == tags[i] {
        a -= 1;
    }
    let mut b = i;
    while b + 1 < tags.len() && tags[b + 1] == tags[i] {
        b += 1;
    }
    (a, b)
}

/// O que fazer depois de registrar uma série no exercício `i` (`open[k]` = séries em aberto do exercício k).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AfterSet {
    /// Exercício para onde a sessão deve ir (None = permanecer).
    pub go_to: Option<usize>,
    /// Iniciar descanso agora?
    pub rest: bool,
}

pub fn after_set(tags: &[String], open: &[usize], i: usize) -> AfterSet {
    let (a, b) = members(tags, i);
    if a == b {
        return AfterSet { go_to: None, rest: true };
    }
    // Próximo do grupo, depois de i, que ainda tem séries.
    if let Some(k) = (i + 1..=b).find(|k| open.get(*k).copied().unwrap_or(0) > 0) {
        return AfterSet { go_to: Some(k), rest: false };
    }
    // Fim da rodada: descanso e volta ao primeiro do grupo com séries em aberto.
    let first = (a..=b).find(|k| open.get(*k).copied().unwrap_or(0) > 0);
    AfterSet { go_to: first, rest: true }
}

impl Db {
    fn day_exercise_tags(&self, schedule_id: Id) -> AppResult<Vec<(Id, String)>> {
        let mut st = self.conn.prepare("SELECT id, group_tag FROM plan_exercise WHERE schedule_id = ?1 ORDER BY position, id")?;
        let v = st.query_map(params![schedule_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    fn write_tags(&self, rows: &[(Id, String)], tags: &[String]) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        for ((id, _), t) in rows.iter().zip(tags) {
            tx.execute("UPDATE plan_exercise SET group_tag = ?2 WHERE id = ?1", params![id, t])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Liga (ou desliga) o exercício da ficha ao próximo, formando/desfazendo um grupo. Transacional.
    pub fn toggle_group_with_next(&mut self, plan_exercise_id: Id) -> AppResult<()> {
        let pe = self.get_plan_exercise(plan_exercise_id)?;
        let rows = self.day_exercise_tags(pe.schedule_id)?;
        let i = rows.iter().position(|(id, _)| *id == plan_exercise_id).unwrap_or(0);
        if i + 1 >= rows.len() {
            return validation(tr("Não há exercício seguinte para agrupar."));
        }
        let tags: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
        let new = toggle_link(&tags, i);
        self.write_tags(&rows, &new)?;
        self.touch_plan(pe.plan_id)
    }

    /// Recalcula os grupos de um dia (após mover/remover exercícios: só vizinhos ficam juntos).
    pub fn normalize_groups(&self, schedule_id: Id) -> AppResult<()> {
        let rows = self.day_exercise_tags(schedule_id)?;
        let tags: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
        let new = normalize(&tags);
        if new != tags {
            self.write_tags(&rows, &new)?;
        }
        Ok(())
    }

    /// Define o tipo planejado de uma série do exercício da ficha.
    pub fn set_plan_set_kind(&self, plan_exercise_id: Id, set_number: i64, kind: SetKind) -> AppResult<()> {
        let pe = self.get_plan_exercise(plan_exercise_id)?;
        if set_number < 1 || set_number > pe.target.sets {
            return validation(tr("Série inválida."));
        }
        let mut kinds = pe.set_kinds.clone();
        kinds.resize(pe.target.sets as usize, SetKind::Normal);
        kinds[(set_number - 1) as usize] = kind;
        self.conn.execute(
            "UPDATE plan_exercise SET set_kinds = ?2 WHERE id = ?1",
            params![plan_exercise_id, format_set_kinds(&kinds)],
        )?;
        self.touch_plan(pe.plan_id)
    }

    /// Altera o tipo de uma série da sessão em andamento (sessões concluídas não mudam).
    pub fn set_session_set_kind(&self, set_id: Id, kind: SetKind) -> AppResult<()> {
        let (status,): (String,) = self.conn.query_row(
            "SELECT s.status FROM session_set ss JOIN session_exercise e ON e.id = ss.session_exercise_id
             JOIN workout_session s ON s.id = e.session_id WHERE ss.id = ?1",
            params![set_id],
            |r| Ok((r.get(0)?,)),
        )?;
        if status != "in_progress" {
            return Err(AppError::Conflict(tr("Sessões concluídas fazem parte do histórico e não podem ser alteradas.").into()));
        }
        self.conn.execute("UPDATE session_set SET set_kind = ?2 WHERE id = ?1", params![set_id, kind.as_str()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn link_unlink_and_normalize() {
        let tags = t(&["", "", "", ""]);
        let g = toggle_link(&tags, 0);
        assert_eq!(g, t(&["G1", "G1", "", ""]));
        let g = toggle_link(&g, 1);
        assert_eq!(g, t(&["G1", "G1", "G1", ""]));
        assert_eq!(GroupKind::of_size(3), Some(GroupKind::Triset));
        let g = toggle_link(&g, 0);
        assert_eq!(g, t(&["", "G1", "G1", ""]));
        // Após mover, etiquetas iguais não vizinhas viram grupos separados ou soltos.
        assert_eq!(normalize(&t(&["G1", "", "G1", "G1"])), t(&["", "", "G1", "G1"]));
        assert_eq!(members(&t(&["", "G1", "G1", ""]), 2), (1, 2));
    }

    #[test]
    fn superset_navigation() {
        let tags = t(&["G1", "G1", ""]);
        // Registrou no A (A e B com séries): vai ao B sem descanso.
        assert_eq!(after_set(&tags, &[2, 3, 3], 0), AfterSet { go_to: Some(1), rest: false });
        // Registrou no B: descanso e volta ao A.
        assert_eq!(after_set(&tags, &[2, 2, 3], 1), AfterSet { go_to: Some(0), rest: true });
        // A terminou: depois do B, fica no B.
        assert_eq!(after_set(&tags, &[0, 2, 3], 1), AfterSet { go_to: Some(1), rest: true });
        // Exercício solto: comportamento normal.
        assert_eq!(after_set(&tags, &[2, 2, 3], 2), AfterSet { go_to: None, rest: true });
    }

    #[test]
    fn set_kinds_persist_and_copy_to_session() {
        let mut db = Db::open_in_memory().unwrap();
        let p = db.create_plan("P", "", ScheduleType::Sequence).unwrap();
        let day = db.add_day(p, "A", 0).unwrap();
        let ex1 = db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap().id;
        let ex2 = db.find_exercise_by_name("Remada curvada com barra").unwrap().unwrap().id;
        let a = db.add_plan_exercise(day, ex1, Target { sets: 3, reps_min: 8, reps_max: 10, load_g: 0 }, 90, "").unwrap();
        db.add_plan_exercise(day, ex2, Target { sets: 3, reps_min: 8, reps_max: 10, load_g: 0 }, 90, "").unwrap();
        db.set_plan_set_kind(a, 1, SetKind::Warmup).unwrap();
        db.set_plan_set_kind(a, 3, SetKind::Drop).unwrap();
        assert!(db.set_plan_set_kind(a, 4, SetKind::Drop).is_err());
        db.toggle_group_with_next(a).unwrap();
        let pe = db.get_plan_exercise(a).unwrap();
        assert_eq!(pe.set_kinds, vec![SetKind::Warmup, SetKind::Normal, SetKind::Drop]);
        assert_eq!(pe.group_tag, "G1");
        let sid = db.start_session(day).unwrap();
        let f = db.get_session_full(sid).unwrap();
        assert_eq!(f.exercises[0].group_tag, "G1");
        assert_eq!(f.exercises[1].group_tag, "G1");
        let kinds: Vec<SetKind> = f.exercises[0].sets.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, vec![SetKind::Warmup, SetKind::Normal, SetKind::Drop]);
        db.set_session_set_kind(f.exercises[1].sets[0].id, SetKind::Failure).unwrap();
        for s in f.exercises.iter().flat_map(|e| e.sets.iter()) {
            db.record_set(s.id, 8, 40_000).unwrap();
        }
        db.finish_session(sid, "").unwrap();
        assert!(db.set_session_set_kind(f.exercises[1].sets[0].id, SetKind::Normal).is_err(), "histórico imutável");
        let f = db.get_session_full(sid).unwrap();
        assert_eq!(f.exercises[1].sets[0].kind, SetKind::Failure);
    }
}
