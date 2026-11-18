//! Cria dados de demonstração para testar a interface no desktop:
//!   cargo run --example seed_demo -- workout-data/workout.db

use workout::domain::*;
use workout::persistence::Db;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "workout-data/workout.db".into());
    let mut db = Db::open(std::path::Path::new(&path)).expect("abrir banco");
    let ex = |db: &Db, n: &str| db.find_exercise_by_name(n).unwrap().unwrap().id;
    let plan = db.create_plan("Hipertrofia ABC", "Divisão em três dias", ScheduleType::Weekday).unwrap();
    let a = db.add_day(plan, "Treino A — Peito e tríceps", weekday_bit(0) | weekday_bit(3)).unwrap();
    let b = db.add_day(plan, "Treino B — Costas e bíceps", weekday_bit(1) | weekday_bit(4)).unwrap();
    let c = db.add_day(plan, "Treino C — Pernas", weekday_bit(2)).unwrap();
    let t = |s, a, b, kg: f64| Target { sets: s, reps_min: a, reps_max: b, load_g: (kg * 1000.0) as i64 };
    for (d, n, tg, r) in [
        (a, "Supino reto com barra", t(4, 8, 12, 40.0), 90),
        (a, "Supino inclinado com halteres", t(3, 10, 12, 16.0), 75),
        (a, "Crucifixo na máquina (peck deck)", t(3, 12, 15, 35.0), 60),
        (a, "Tríceps pulley", t(3, 10, 12, 25.0), 60),
        (b, "Puxada frontal no pulley", t(4, 8, 12, 50.0), 90),
        (b, "Remada baixa no cabo", t(3, 10, 12, 45.0), 75),
        (b, "Rosca direta com barra", t(3, 8, 10, 20.0), 60),
        (c, "Agachamento livre com barra", t(4, 6, 8, 60.0), 120),
        (c, "Leg press 45°", t(3, 10, 12, 140.0), 90),
        (c, "Mesa flexora", t(3, 10, 12, 30.0), 60),
    ] {
        db.add_plan_exercise(d, ex(&db, n), tg, r, "").unwrap();
    }
    let seq = db.create_plan("Full body (sequência)", "", ScheduleType::Sequence).unwrap();
    let s1 = db.add_day(seq, "A", 0).unwrap();
    let s2 = db.add_day(seq, "B", 0).unwrap();
    db.add_plan_exercise(s1, ex(&db, "Agachamento goblet"), t(3, 10, 10, 20.0), 60, "").unwrap();
    db.add_plan_exercise(s2, ex(&db, "Levantamento terra romeno"), t(3, 8, 8, 50.0), 90, "").unwrap();
    // Histórico com evolução
    for (i, extra) in [0.0, 2.5, 5.0].iter().enumerate() {
        let sid = db.start_session(a).unwrap();
        let f = db.get_session_full(sid).unwrap();
        for e in &f.exercises {
            for s in &e.sets {
                db.record_set(s.id, s.planned_reps_max - (i as i64 % 2), s.planned_load_g + (*extra * 1000.0) as i64).unwrap();
            }
        }
        db.finish_session(sid, if i == 2 { "Boa sessão, carga subiu." } else { "" }).unwrap();
    }
    println!("dados de demonstração criados em {path}");
}
