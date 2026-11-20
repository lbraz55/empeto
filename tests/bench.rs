//! Benchmark do documento de evolução v1.1 (12.5): 100, 1.000 e 10.000 sessões.
//! Não roda no `cargo test` normal. Para executar:
//!   cargo test --release --test bench -- --ignored --nocapture
//! Mede abertura/migração do banco, listas do histórico, evolução, relatório mensal, recordes,
//! comparação, registro de série e backup. Os resultados ficam em docs/DESEMPENHO.md.

use std::time::Instant;
use workout::domain::*;
use workout::import_export::backup::create_backup;
use workout::persistence::Db;

fn populate(db: &mut Db, n: usize) {
    let p = db.create_plan("Bench", "", ScheduleType::Sequence).unwrap();
    let names = ["Supino reto com barra", "Agachamento livre com barra", "Remada curvada com barra", "Desenvolvimento com halteres", "Rosca direta com barra"];
    let day = db.add_day(p, "A", 0).unwrap();
    for nm in names {
        let ex = db.find_exercise_by_name(nm).unwrap().unwrap().id;
        db.add_plan_exercise(day, ex, Target { sets: 4, reps_min: 8, reps_max: 12, load_g: 40_000 }, 90, "").unwrap();
    }
    // Sessões distribuídas nos últimos ~n/3 dias (até 3 por dia), datas ajustadas direto no banco.
    let now = now_ms();
    // Só para gerar os dados rápido: sem fsync. As medições reabrem o banco com as configurações do app.
    db.conn.execute_batch("PRAGMA synchronous = OFF;").unwrap();
    for i in 0..n {
        let sid = db.start_session(day).unwrap();
        let f = db.get_session_full(sid).unwrap();
        for (k, e) in f.exercises.iter().enumerate() {
            for s in &e.sets {
                db.record_set(s.id, 8 + ((i + k) % 5) as i64, 40_000 + ((i % 20) as i64) * 1_250).unwrap();
            }
        }
        db.finish_session(sid, "").unwrap();
        let t = now - ((n - i) as i64) * 8 * 3_600_000;
        db.conn
            .execute_batch(&format!(
                "DROP TRIGGER IF EXISTS trg_session_immutable; UPDATE workout_session SET started_at = {t}, ended_at = {} WHERE id = {sid};",
                t + 3_600_000
            ))
            .unwrap();
    }
}

fn ms<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    println!("  {:<44} {:>8.1} ms", label, t.elapsed().as_secs_f64() * 1000.0);
    r
}

#[test]
#[ignore]
fn bench_history_sizes() {
    for n in [100usize, 1_000, 10_000] {
        let dir = std::env::temp_dir().join(format!("empeto-bench-{n}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("workout.db");
        let t = Instant::now();
        {
            let mut db = Db::open(&path).unwrap();
            populate(&mut db, n);
        }
        println!("\n{n} sessões (gerar: {:.1} s; arquivo: {:.1} MB)", t.elapsed().as_secs_f64(),
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0);
        let mut db = ms("abrir banco (inicialização)", || Db::open(&path).unwrap());
        ms("histórico: primeira página (100 sessões)", || db.list_session_summaries_limit(None, None, Some(100)).unwrap());
        let list = ms("histórico: todas as sessões (referência)", || db.list_session_summaries(None, None).unwrap());
        assert_eq!(list.len(), n);
        let ex = ms("histórico: exercícios realizados", || db.history_exercises().unwrap());
        let key = ex[0].key.clone();
        ms("evolução de um exercício (tudo)", || db.exercise_evolution(&key, None).unwrap());
        let today = workout::calendar::today();
        use chrono::Datelike;
        ms("relatório mensal", || db.month_report(today.year(), today.month()).unwrap());
        ms("recordes dos últimos 30 dias", || db.recent_records(now_ms() - 30 * 86_400_000, None).unwrap());
        ms("estatísticas de 2 sessões (comparação)", || (db.session_stats(list[0].id).unwrap(), db.session_stats(list[1].id).unwrap()));
        ms("busca global", || workout::application::search::search(&db, "supino").unwrap());
        let plan = db.list_plans_full().unwrap()[0].clone();
        ms("abrir fichas", || db.list_plans_full().unwrap());
        let day = plan.days[0].day.id;
        let sid = ms("iniciar sessão", || db.start_session(day).unwrap());
        let set = db.get_session_full(sid).unwrap().exercises[0].sets[0].id;
        ms("registrar série", || db.record_set(set, 10, 40_000).unwrap());
        ms("recorde da série registrada", || db.records_for_set(set).unwrap());
        let out = dir.join("b.zip");
        ms("criar backup", || create_backup(&db, &out, &dir, "bench", None).unwrap());
        println!("  backup: {:.1} MB", std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
