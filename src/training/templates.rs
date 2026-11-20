//! Modelos de fichas (documento EMPETO, 4.6): catálogo local e versionado.
//! Criar a partir de um modelo gera uma ficha nova e independente; alterar a ficha não altera o modelo.

use crate::i18n::tr;
use crate::domain::*;
use crate::persistence::Db;
use rusqlite::{params, OptionalExtension};

pub const TEMPLATES_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct TemplateExercise {
    pub name: &'static str,
    pub sets: i64,
    pub reps: (i64, i64),
    pub rest: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct TemplateDay {
    pub name: &'static str,
    /// Bitmask de dias da semana (seg = 1 … dom = 64); 0 = sequência.
    pub weekdays: u8,
    pub exercises: &'static [TemplateExercise],
}

#[derive(Debug, Clone, Copy)]
pub struct Template {
    pub code: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub schedule: ScheduleType,
    pub days: &'static [TemplateDay],
}

const fn ex(name: &'static str, sets: i64, a: i64, b: i64, rest: i64) -> TemplateExercise {
    TemplateExercise { name, sets, reps: (a, b), rest }
}

pub const TEMPLATES: &[Template] = &[
    Template {
        code: "abc",
        name: "ABC",
        description: "Divisão em três dias: A peito/ombros/tríceps, B costas/bíceps, C pernas.",
        schedule: ScheduleType::Sequence,
        days: &[
            TemplateDay {
                name: "A — Peito, ombros e tríceps",
                weekdays: 0,
                exercises: &[
                    ex("Supino reto com barra", 4, 8, 12, 90),
                    ex("Supino inclinado com halteres", 3, 10, 12, 75),
                    ex("Desenvolvimento com halteres", 3, 8, 12, 75),
                    ex("Elevação lateral com halteres", 3, 12, 15, 60),
                    ex("Tríceps pulley", 3, 10, 12, 60),
                ],
            },
            TemplateDay {
                name: "B — Costas e bíceps",
                weekdays: 0,
                exercises: &[
                    ex("Puxada frontal no pulley", 4, 8, 12, 90),
                    ex("Remada baixa no cabo", 3, 10, 12, 75),
                    ex("Remada unilateral com halter", 3, 10, 12, 60),
                    ex("Rosca direta com barra", 3, 8, 12, 60),
                    ex("Rosca martelo", 3, 10, 12, 60),
                ],
            },
            TemplateDay {
                name: "C — Pernas",
                weekdays: 0,
                exercises: &[
                    ex("Agachamento livre com barra", 4, 6, 10, 120),
                    ex("Leg press 45°", 3, 10, 12, 90),
                    ex("Cadeira extensora", 3, 12, 15, 60),
                    ex("Mesa flexora", 3, 10, 12, 60),
                    ex("Panturrilha em pé", 4, 12, 15, 45),
                ],
            },
        ],
    },
    Template {
        code: "fullbody",
        name: "Full Body",
        description: "Corpo inteiro em cada treino, alternando dois dias (A/B).",
        schedule: ScheduleType::Sequence,
        days: &[
            TemplateDay {
                name: "Full Body A",
                weekdays: 0,
                exercises: &[
                    ex("Agachamento livre com barra", 3, 8, 10, 120),
                    ex("Supino reto com barra", 3, 8, 10, 90),
                    ex("Remada curvada com barra", 3, 8, 10, 90),
                    ex("Desenvolvimento com halteres", 2, 10, 12, 75),
                    ex("Prancha", 3, 1, 1, 45),
                ],
            },
            TemplateDay {
                name: "Full Body B",
                weekdays: 0,
                exercises: &[
                    ex("Levantamento terra romeno", 3, 8, 10, 120),
                    ex("Supino inclinado com halteres", 3, 10, 12, 90),
                    ex("Puxada frontal no pulley", 3, 10, 12, 90),
                    ex("Afundo com halteres", 2, 10, 12, 75),
                    ex("Abdominal supra", 3, 15, 20, 45),
                ],
            },
        ],
    },
    Template {
        code: "ppl",
        name: "Push/Pull/Legs",
        description: "Empurrar, puxar e pernas em sequência.",
        schedule: ScheduleType::Sequence,
        days: &[
            TemplateDay {
                name: "Push — Empurrar",
                weekdays: 0,
                exercises: &[
                    ex("Supino reto com barra", 4, 6, 10, 120),
                    ex("Desenvolvimento com barra", 3, 8, 10, 90),
                    ex("Supino inclinado com halteres", 3, 10, 12, 75),
                    ex("Elevação lateral com halteres", 3, 12, 15, 60),
                    ex("Tríceps francês com halter", 3, 10, 12, 60),
                ],
            },
            TemplateDay {
                name: "Pull — Puxar",
                weekdays: 0,
                exercises: &[
                    ex("Barra fixa", 4, 6, 10, 120),
                    ex("Remada curvada com barra", 3, 8, 10, 90),
                    ex("Face pull", 3, 12, 15, 60),
                    ex("Rosca alternada com halteres", 3, 10, 12, 60),
                ],
            },
            TemplateDay {
                name: "Legs — Pernas",
                weekdays: 0,
                exercises: &[
                    ex("Agachamento livre com barra", 4, 6, 10, 120),
                    ex("Levantamento terra romeno", 3, 8, 10, 120),
                    ex("Leg press 45°", 3, 10, 12, 90),
                    ex("Cadeira flexora", 3, 10, 12, 60),
                    ex("Panturrilha sentado", 4, 12, 15, 45),
                ],
            },
        ],
    },
];

pub fn find(code: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.code == code)
}

/// Prévia: estrutura do modelo e exercícios que ainda não existem na biblioteca.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplatePreview {
    pub text: String,
    pub missing_exercises: Vec<String>,
}

/// Valida a estrutura do modelo (nomes, séries, faixas, descanso).
pub fn validate(t: &Template) -> AppResult<()> {
    if t.days.is_empty() {
        return validation(tr("Modelo sem dias."));
    }
    for d in t.days {
        if d.exercises.is_empty() {
            return validation(trf!("O dia \"{}\" do modelo não tem exercícios.", d.name));
        }
        for e in d.exercises {
            Target { sets: e.sets, reps_min: e.reps.0, reps_max: e.reps.1, load_g: 0 }.validated()?;
            validate_rest(e.rest)?;
        }
    }
    Ok(())
}

impl Db {
    pub fn template_preview(&self, code: &str) -> AppResult<TemplatePreview> {
        let t = find(code).ok_or_else(|| AppError::NotFound(tr("Modelo não encontrado.").into()))?;
        validate(t)?;
        let mut missing = Vec::new();
        let mut text = trf!("{}\n", tr(t.description));
        for d in t.days {
            text.push_str(&trf!("\n{}\n", tr(d.name)));
            for e in d.exercises {
                let exists = self.find_exercise_by_name(e.name)?.map(|x| !x.archived).unwrap_or(false);
                if !exists && !missing.contains(&e.name.to_string()) {
                    missing.push(e.name.to_string());
                }
                text.push_str(&trf!(
                    "  • {} — {} × {}{}\n",
                    e.name,
                    e.sets,
                    format_reps(e.reps.0, e.reps.1),
                    if exists { "" } else { tr(" (será adicionado à biblioteca)") }
                ));
            }
        }
        Ok(TemplatePreview { text, missing_exercises: missing })
    }

    /// Cria uma ficha nova a partir do modelo, em uma transação. Exercícios ausentes (renomeados
    /// ou excluídos da biblioteca) são criados como personalizados; arquivados são reativados.
    pub fn create_from_template(&mut self, code: &str) -> AppResult<Id> {
        let t = find(code).ok_or_else(|| AppError::NotFound(tr("Modelo não encontrado.").into()))?;
        validate(t)?;
        let mut name = tr(t.name).to_string();
        let mut n = 2;
        while self.find_plan_by_name(&name)?.is_some() {
            name = trf!("{} ({})", tr(t.name), n);
            n += 1;
        }
        let now = now_ms();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO workout_plan(name, description, schedule_type, active, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            params![name, trf!("Criada a partir do modelo {} (v{}). {}", tr(t.name), TEMPLATES_VERSION, tr(t.description)), t.schedule.as_str(), now],
        )?;
        let plan = tx.last_insert_rowid();
        for (di, d) in t.days.iter().enumerate() {
            tx.execute(
                "INSERT INTO plan_schedule(plan_id, name, weekdays, sequence_order) VALUES (?1, ?2, ?3, ?4)",
                params![plan, tr(d.name), d.weekdays as i64, di as i64],
            )?;
            let day = tx.last_insert_rowid();
            for (pos, e) in d.exercises.iter().enumerate() {
                let found: Option<(Id, i64)> = tx
                    .query_row("SELECT id, archived FROM exercise WHERE name = ?1 COLLATE NOCASE", params![e.name], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                    .optional()?;
                let ex_id = match found {
                    Some((id, archived)) => {
                        if archived != 0 {
                            tx.execute("UPDATE exercise SET archived = 0 WHERE id = ?1", params![id])?;
                        }
                        id
                    }
                    None => {
                        tx.execute(
                            "INSERT INTO exercise(name, is_custom, created_at, updated_at) VALUES (?1, 1, ?2, ?2)",
                            params![e.name, now],
                        )?;
                        tx.last_insert_rowid()
                    }
                };
                tx.execute(
                    "INSERT INTO plan_exercise(plan_id, schedule_id, exercise_id, position, target_sets, target_reps_min,
                       target_reps_max, target_load_g, rest_seconds, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, '')",
                    params![plan, day, ex_id, pos as i64, e.sets, e.reps.0, e.reps.1, e.rest],
                )?;
            }
        }
        tx.commit()?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_templates_valid_and_use_library() {
        let db = Db::open_in_memory().unwrap();
        for t in TEMPLATES {
            validate(t).unwrap();
            let p = db.template_preview(t.code).unwrap();
            assert!(p.missing_exercises.is_empty(), "{}: {:?}", t.code, p.missing_exercises);
        }
    }

    #[test]
    fn creates_independent_plan() {
        let mut db = Db::open_in_memory().unwrap();
        let a = db.create_from_template("abc").unwrap();
        let b = db.create_from_template("abc").unwrap();
        assert_ne!(a, b);
        assert_eq!(db.get_plan(b).unwrap().name, "ABC (2)");
        let full = db.get_plan_full(a).unwrap();
        assert_eq!(full.days.len(), 3);
        // editar a ficha não afeta o modelo nem a outra ficha
        let first = full.days[0].exercises[0].id;
        db.delete_plan_exercise(first).unwrap();
        assert_eq!(db.get_plan_full(b).unwrap().days[0].exercises.len(), 5);
        assert_eq!(find("abc").unwrap().days[0].exercises.len(), 5);
    }

    #[test]
    fn missing_exercise_is_detected_and_created() {
        let mut db = Db::open_in_memory().unwrap();
        let id = db.find_exercise_by_name("Face pull").unwrap().unwrap().id;
        db.update_exercise(id, &ExerciseInput { name: "Face pull na polia".into(), ..Default::default() }).unwrap();
        let p = db.template_preview("ppl").unwrap();
        assert_eq!(p.missing_exercises, vec!["Face pull".to_string()]);
        db.create_from_template("ppl").unwrap();
        assert!(db.find_exercise_by_name("Face pull").unwrap().unwrap().custom);
    }
}
