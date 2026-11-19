//! Busca global (documento EMPETO, 8.2): serviço central que consulta os módulos responsáveis
//! (biblioteca, fichas, histórico). A interface nunca consulta o banco diretamente.

use crate::calendar::local_date_of;
use crate::domain::*;
use crate::persistence::Db;
use crate::training::ExerciseFilter;
use chrono::Datelike;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Exercise,
    Plan,
    Session,
}

impl HitKind {
    pub fn label(self) -> &'static str {
        match self {
            HitKind::Exercise => "Exercício",
            HitKind::Plan => "Ficha",
            HitKind::Session => "Sessão",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub kind: HitKind,
    pub id: Id,
    pub title: String,
    pub subtitle: String,
}

const PER_KIND: usize = 20;

fn matches(hay: &str, query: &str) -> bool {
    let hay = normalize(hay);
    normalize(query).split_whitespace().all(|w| hay.contains(w))
}

pub fn search(db: &Db, query: &str) -> AppResult<Vec<SearchHit>> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    // Fichas (nome, objetivo, dias)
    for p in db.list_plans_full()?.into_iter() {
        let days = p.days.iter().map(|d| d.day.name.clone()).collect::<Vec<_>>().join(" ");
        if matches(&format!("{} {} {}", p.plan.name, p.plan.description, days), q) {
            out.push(SearchHit {
                kind: HitKind::Plan,
                id: p.plan.id,
                title: p.plan.name.clone(),
                subtitle: format!("{} dia(s){}", p.days.len(), if p.plan.favorite { " · favorita" } else { "" }),
            });
        }
        if out.iter().filter(|h| h.kind == HitKind::Plan).count() >= PER_KIND {
            break;
        }
    }
    // Exercícios (inclui arquivados para que o histórico continue encontrável)
    let f = ExerciseFilter { text: q.to_string(), include_archived: true, ..Default::default() };
    for e in db.list_exercises_page(&f, PER_KIND)?.items {
        out.push(SearchHit {
            kind: HitKind::Exercise,
            id: e.id,
            title: e.name.clone(),
            subtitle: [e.muscle_focus.as_str(), e.equipment.as_str(), if e.archived { "arquivado" } else { "" }]
                .iter()
                .filter(|x| !x.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" · "),
        });
    }
    // Sessões concluídas (ficha, dia, observações)
    let mut n = 0;
    for s in db.list_session_summaries(None, None)? {
        let notes = db.get_session(s.id).map(|x| x.notes).unwrap_or_default();
        if matches(&format!("{} {} {}", s.plan_name, s.schedule_name, notes), q) {
            let d = local_date_of(s.started_at);
            out.push(SearchHit {
                kind: HitKind::Session,
                id: s.id,
                title: format!("{} — {}", s.plan_name, s.schedule_name),
                subtitle: format!("{:02}/{:02}/{} · {} série(s)", d.day(), d.month(), d.year(), s.sets),
            });
            n += 1;
            if n >= PER_KIND {
                break;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_each_kind_without_accents() {
        let mut db = Db::open_in_memory().unwrap();
        let p = db.create_plan("Hipertrofia Pernas", "foco em quadríceps", ScheduleType::Sequence).unwrap();
        let day = db.add_day(p, "Dia único", 0).unwrap();
        let ex = db.find_exercise_by_name("Agachamento livre com barra").unwrap().unwrap().id;
        db.add_plan_exercise(day, ex, Target { sets: 1, reps_min: 5, reps_max: 5, load_g: 0 }, 60, "").unwrap();
        let sid = db.start_session(day).unwrap();
        let set = db.get_session_full(sid).unwrap().exercises[0].sets[0].id;
        db.record_set(set, 5, 60_000).unwrap();
        db.finish_session(sid, "sessão pesada").unwrap();
        let hits = search(&db, "pernas").unwrap();
        assert!(hits.iter().any(|h| h.kind == HitKind::Plan));
        assert!(hits.iter().any(|h| h.kind == HitKind::Session));
        let hits = search(&db, "agachamento").unwrap();
        assert!(hits.iter().any(|h| h.kind == HitKind::Exercise));
        assert!(search(&db, "sessao pesada").unwrap().iter().any(|h| h.kind == HitKind::Session));
        assert!(search(&db, "a").unwrap().is_empty());
    }
}
