//! Casos de teste prioritários da documentação (seção 4.2), no nível de domínio/persistência.
//! CT-06/CT-07 (cronômetro) estão em `src/domain/timer.rs`; CT-11 (offline) é garantido pela
//! ausência da permissão INTERNET no manifesto e verificado em `android_manifest_is_minimal`.

use workout::calendar::{planned_on, today};
use workout::domain::*;
use workout::import_export::backup::{create_backup, validate_backup};
use workout::import_export::xlsx_export::{export_template, export_xlsx, ExportKind};
use workout::import_export::xlsx_import::*;
use workout::persistence::Db;
use workout::training::ExerciseFilter;

fn ex(db: &Db, name: &str) -> Id {
    db.find_exercise_by_name(name).unwrap().unwrap().id
}

fn t(sets: i64, reps: i64, kg: i64) -> Target {
    Target { sets, reps_min: reps, reps_max: reps, load_g: kg * 1000 }
}

/// Ficha com dois dias (CT-01).
fn sample_plan(db: &Db) -> (Id, Id, Id) {
    let plan = db.create_plan("ABC Hipertrofia", "teste", ScheduleType::Weekday).unwrap();
    let a = db.add_day(plan, "Treino A", weekday_bit(0) | weekday_bit(3)).unwrap();
    let b = db.add_day(plan, "Treino B", weekday_bit(1)).unwrap();
    db.add_plan_exercise(a, ex(db, "Supino reto com barra"), t(3, 10, 40), 90, "").unwrap();
    db.add_plan_exercise(a, ex(db, "Crucifixo com halteres"), t(3, 12, 12), 60, "").unwrap();
    db.add_plan_exercise(b, ex(db, "Agachamento livre com barra"), t(4, 8, 60), 120, "").unwrap();
    (plan, a, b)
}

fn complete_all_sets(db: &Db, sid: Id, load_g: i64) {
    let full = db.get_session_full(sid).unwrap();
    for e in &full.exercises {
        for (i, s) in e.sets.iter().enumerate() {
            db.record_set(s.id, 10 - i as i64, load_g + i as i64 * 2500).unwrap();
        }
    }
}

#[test]
fn ct01_create_plan_with_two_days() {
    let db = Db::open_in_memory().unwrap();
    let (plan, _, _) = sample_plan(&db);
    let plans = db.list_plans().unwrap();
    assert!(plans.iter().any(|p| p.id == plan && p.name == "ABC Hipertrofia"));
    let full = db.get_plan_full(plan).unwrap();
    assert_eq!(full.days.len(), 2);
    assert_eq!(full.days[0].exercises.len(), 2);
    assert_eq!(full.days[1].exercises[0].target.sets, 4);
}

#[test]
fn ct02_editing_plan_keeps_history_identical() {
    let mut db = Db::open_in_memory().unwrap();
    let (plan, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    complete_all_sets(&db, sid, 40_000);
    db.finish_session(sid, "boa").unwrap();
    let before = db.get_session_full(sid).unwrap();

    // Edita a ficha: renomeia, troca exercício, altera alvos, apaga dia.
    db.update_plan(plan, "Outra", "", ScheduleType::Sequence).unwrap();
    let pe = db.list_plan_exercises(a).unwrap()[0].clone();
    db.update_plan_exercise(pe.id, ex(&db, "Supino na máquina"), t(5, 5, 80), 180, "x").unwrap();
    db.update_exercise(
        ex(&db, "Crucifixo com halteres"),
        &ExerciseInput { name: "Crucifixo renomeado".into(), ..Default::default() },
    )
    .unwrap();
    db.delete_day(a).unwrap();

    let after = db.get_session_full(sid).unwrap();
    assert_eq!(before.session.plan_name, after.session.plan_name);
    assert_eq!(before.session.plan_snapshot_json, after.session.plan_snapshot_json);
    let names = |f: &SessionFull| f.exercises.iter().map(|e| e.performed_name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&before), names(&after));
    let sets = |f: &SessionFull| {
        f.exercises.iter().flat_map(|e| e.sets.iter().map(|s| (s.actual_reps, s.actual_load_g))).collect::<Vec<_>>()
    };
    assert_eq!(sets(&before), sets(&after));
    let snap: PlanSnapshot = serde_json::from_str(&after.session.plan_snapshot_json).unwrap();
    assert_eq!(snap.plan_name, "ABC Hipertrofia");
    assert_eq!(snap.exercises[0].name, "Supino reto com barra");
}

#[test]
fn completed_session_is_immutable_at_db_level() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    complete_all_sets(&db, sid, 40_000);
    db.finish_session(sid, "").unwrap();
    let set_id = db.get_session_full(sid).unwrap().exercises[0].sets[0].id;
    assert!(db.record_set(set_id, 1, 1).is_err());
    // Mesmo contornando a camada de aplicação, o banco recusa.
    assert!(db.conn.execute("UPDATE session_set SET actual_reps = 99 WHERE id = ?1", [set_id]).is_err());
    assert!(db.conn.execute("UPDATE workout_session SET notes = 'x' WHERE id = ?1", [sid]).is_err());
    assert!(db.conn.execute("DELETE FROM workout_session WHERE id = ?1", [sid]).is_err());
}

#[test]
fn ct03_record_sets_with_different_loads() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    let full = db.get_session_full(sid).unwrap();
    let sets = &full.exercises[0].sets;
    db.record_set(sets[0].id, 10, 40_000).unwrap();
    db.record_set(sets[1].id, 8, 42_500).unwrap();
    db.record_set(sets[2].id, 6, 45_000).unwrap();
    let full = db.get_session_full(sid).unwrap();
    let got: Vec<_> = full.exercises[0].sets.iter().map(|s| (s.actual_reps, s.actual_load_g)).collect();
    assert_eq!(got, vec![(Some(10), Some(40_000)), (Some(8), Some(42_500)), (Some(6), Some(45_000))]);
    // RN-05: planejado separado do realizado
    assert!(full.exercises[0].sets.iter().all(|s| s.planned_load_g == 40_000 && s.planned_reps_min == 10));
}

#[test]
fn ct04_temporary_substitution_keeps_plan() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let original = db.list_plan_exercises(a).unwrap()[0].exercise.id;
    let sid = db.start_session(a).unwrap();
    let se = db.get_session_full(sid).unwrap().exercises[0].id;
    let alt = ex(&db, "Supino reto com halteres");
    db.substitute_exercise(se, alt, false).unwrap();
    assert_eq!(db.list_plan_exercises(a).unwrap()[0].exercise.id, original);
    let e = db.get_session_full(sid).unwrap().exercises[0].clone();
    assert_eq!(e.substitution, SubstitutionType::Temporary);
    assert_eq!(e.original_name, "Supino reto com barra"); // RN-04
    assert_eq!(e.performed_name, "Supino reto com halteres");
    db.undo_substitution(se).unwrap();
    let e = db.get_session_full(sid).unwrap().exercises[0].clone();
    assert_eq!(e.performed_name, "Supino reto com barra");
}

#[test]
fn ct05_permanent_substitution_updates_future_sessions_only() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    // sessão antiga
    let old = db.start_session(a).unwrap();
    complete_all_sets(&db, old, 40_000);
    db.finish_session(old, "").unwrap();
    // sessão nova com substituição permanente
    let sid = db.start_session(a).unwrap();
    let se = db.get_session_full(sid).unwrap().exercises[0].id;
    let alt = ex(&db, "Supino na máquina");
    db.substitute_exercise(se, alt, true).unwrap();
    complete_all_sets(&db, sid, 50_000);
    db.finish_session(sid, "").unwrap();
    // ficha atualizada
    assert_eq!(db.list_plan_exercises(a).unwrap()[0].exercise.id, alt);
    // sessão antiga intacta
    let o = db.get_session_full(old).unwrap();
    assert_eq!(o.exercises[0].performed_name, "Supino reto com barra");
    assert_eq!(o.exercises[0].substitution, SubstitutionType::None);
    // próxima sessão já usa o substituto
    let next = db.start_session(a).unwrap();
    let n = db.get_session_full(next).unwrap();
    assert_eq!(n.exercises[0].original_name, "Supino na máquina");
    assert_eq!(n.exercises[0].substitution, SubstitutionType::None);
}

fn write_sample_xlsx(path: &std::path::Path) {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Planilha").unwrap();
    let rows: &[&[&str]] = &[
        &["Ficha", "Dia", "Exercício", "Grupo muscular", "Séries", "Repetições", "Carga (kg)", "Descanso", "Coluna estranha"],
        &["Força", "Segunda", "Levantamento terra", "Posterior", "5", "5", "100", "3:00", "?"],
        &["", "", "Barra fixa", "Costas", "4", "6-8", "", "120", ""],
        &["", "Quinta", "Desenvolvimento com barra", "Ombros", "5", "5", "45,5", "", ""],
        &["", "", "Exercício inventado XYZ", "Core", "3", "abc", "10", "60", ""],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            ws.write_string(r as u32, c as u16, *v).unwrap();
        }
    }
    wb.save(path).unwrap();
}

#[test]
fn ct08_import_xlsx_with_recognized_headers_shows_preview_first() {
    let mut db = Db::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("treino.xlsx");
    write_sample_xlsx(&path);
    let wb = read_workbook(&path).unwrap();
    let sheet = &wb.sheets[0];
    let mapping = detect_mapping(sheet);
    let plans_before = db.list_plans().unwrap().len();
    let preview = build_preview(&db, sheet, &mapping, ImportDefaults::default());
    // Prévia não grava nada (RN-09).
    assert_eq!(db.list_plans().unwrap().len(), plans_before);
    assert!(preview.can_import(), "{:?}", preview.errors);
    assert_eq!(preview.unrecognized_columns, vec!["Coluna estranha".to_string()]);
    assert!(preview.warnings.iter().any(|w| w.contains("repetições inválidas"))); // RN-10
    let plan = &preview.plans[0];
    assert_eq!(plan.name, "Força");
    assert_eq!(plan.schedule, ScheduleType::Weekday);
    assert_eq!(plan.days.len(), 2);
    assert_eq!(plan.days[0].exercises[0].rest_seconds, 180);
    assert_eq!(plan.days[1].exercises[0].target.load_g, 45_500);

    let res = apply_import(&mut db, &preview, ConflictPolicy::CreateNew).unwrap();
    assert_eq!(res.plans, 1);
    assert_eq!(res.exercises, 4);
    assert_eq!(res.new_catalog_exercises, 1);
    // Reimportar não sobrescreve sem escolha explícita.
    let preview2 = build_preview(&db, sheet, &mapping, ImportDefaults::default());
    assert!(preview2.has_conflicts());
    apply_import(&mut db, &preview2, ConflictPolicy::CreateNew).unwrap();
    assert!(db.find_plan_by_name("Força (importada)").unwrap().is_some());
    assert!(db.find_plan_by_name("Força").unwrap().is_some());
    // Substituição explícita
    let preview3 = build_preview(&db, sheet, &mapping, ImportDefaults::default());
    let r = apply_import(&mut db, &preview3, ConflictPolicy::Replace).unwrap();
    assert_eq!(r.replaced, 1);
}

#[test]
fn ct09_invalid_file_gives_readable_error_and_no_partial_data() {
    let mut db = Db::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.xlsx");
    std::fs::write(&bad, b"isto nao e uma planilha").unwrap();
    let err = read_workbook(&bad).unwrap_err();
    assert!(err.to_string().contains("Formato não reconhecido"));
    let corrupt = dir.path().join("corrupt.xlsx");
    std::fs::write(&corrupt, b"PK\x03\x04garbage-garbage-garbage").unwrap();
    assert!(read_workbook(&corrupt).is_err());
    let empty = dir.path().join("empty.xlsx");
    std::fs::write(&empty, b"").unwrap();
    assert!(read_workbook(&empty).is_err());

    // Falha no meio da transação não deixa dados parciais.
    let plans_before = db.list_plans().unwrap().len();
    let mut preview = ImportPreview {
        sheet_name: "x".into(),
        valid_rows: 2,
        data_rows: 2,
        ..Default::default()
    };
    let good = ImportExercise {
        row: 2,
        name: "Supino reto com barra".into(),
        muscle: String::new(),
        exercise_type: String::new(),
        equipment: String::new(),
        pattern: String::new(),
        target: t(3, 10, 40),
        rest_seconds: 90,
        notes: String::new(),
    };
    let mut broken = good.clone();
    broken.target.sets = 999; // viola CHECK do banco
    preview.plans.push(ImportPlan {
        name: "Parcial".into(),
        schedule: ScheduleType::Sequence,
        days: vec![ImportDay { name: "A".into(), weekdays: 0, exercises: vec![good, broken] }],
        exists: false,
    });
    assert!(apply_import(&mut db, &preview, ConflictPolicy::CreateNew).is_err());
    assert_eq!(db.list_plans().unwrap().len(), plans_before);
    assert!(db.find_plan_by_name("Parcial").unwrap().is_none());
}

#[test]
fn ct10_backup_and_restore_on_clean_install() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("tmp");
    let zip = dir.path().join("backup.zip");
    let (plan_count, session_id, prefs) = {
        let mut db = Db::open(&dir.path().join("a/workout.db")).unwrap();
        let (_, a, _) = sample_plan(&db);
        let sid = db.start_session(a).unwrap();
        complete_all_sets(&db, sid, 40_000);
        db.finish_session(sid, "ótima sessão").unwrap();
        db.set_pref("training.unit", "lb").unwrap();
        create_backup(&db, &zip, &work, "0.1.0", None).unwrap();
        (db.list_plans().unwrap().len(), sid, db.all_prefs().unwrap())
    };
    // "Instalação limpa"
    let mut fresh = Db::open(&dir.path().join("b/workout.db")).unwrap();
    assert_eq!(fresh.list_plans().unwrap().len(), 0);
    let v = validate_backup(&zip, &work, None).unwrap();
    assert_eq!(v.manifest.counts.sessions, 1);
    fresh.restore_from(v).unwrap();
    assert_eq!(fresh.list_plans().unwrap().len(), plan_count);
    let s = fresh.get_session_full(session_id).unwrap();
    assert_eq!(s.session.notes, "ótima sessão");
    assert_eq!(fresh.all_prefs().unwrap(), prefs);
    assert!(fresh.integrity_ok().unwrap());
}

#[test]
fn restore_rejects_tampered_backup_and_keeps_current_data() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("tmp");
    let db = Db::open(&dir.path().join("w.db")).unwrap();
    sample_plan(&db);
    let zip = dir.path().join("b.zip");
    create_backup(&db, &zip, &work, "0.1.0", None).unwrap();
    // adultera: troca o banco por outro conteúdo mantendo o manifesto
    let tampered = dir.path().join("t.zip");
    {
        let mut src = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
        let mut man = String::new();
        std::io::Read::read_to_string(&mut src.by_name("manifest.json").unwrap(), &mut man).unwrap();
        let mut w = zip::ZipWriter::new(std::fs::File::create(&tampered).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("manifest.json", o).unwrap();
        std::io::Write::write_all(&mut w, man.as_bytes()).unwrap();
        w.start_file("workout.db", o).unwrap();
        std::io::Write::write_all(&mut w, b"not a database").unwrap();
        w.finish().unwrap();
    }
    assert!(validate_backup(&tampered, &work, None).is_err());
    let garbage = dir.path().join("g.zip");
    std::fs::write(&garbage, b"nope").unwrap();
    assert!(validate_backup(&garbage, &work, None).is_err());
    assert_eq!(db.list_plans().unwrap().len(), 1);
}

#[test]
fn ct12_deleting_used_plan_keeps_history() {
    let mut db = Db::open_in_memory().unwrap();
    let (plan, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    complete_all_sets(&db, sid, 40_000);
    db.finish_session(sid, "").unwrap();
    db.delete_plan(plan).unwrap();
    assert!(db.get_plan(plan).is_err());
    let s = db.get_session_full(sid).unwrap();
    assert_eq!(s.session.plan_id, None);
    assert_eq!(s.session.plan_name, "ABC Hipertrofia");
    assert_eq!(s.completed_sets(), 6);
    assert_eq!(db.list_session_summaries(None, None).unwrap().len(), 1);
}

#[test]
fn cannot_delete_plan_with_session_in_progress() {
    let mut db = Db::open_in_memory().unwrap();
    let (plan, a, _) = sample_plan(&db);
    db.start_session(a).unwrap();
    assert!(db.delete_plan(plan).is_err());
}

#[test]
fn rn02_planned_is_not_realized_until_recorded() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    assert!(db.list_session_summaries(None, None).unwrap().is_empty());
    assert!(db.finish_session(sid, "").is_err()); // nada registrado
    db.discard_session(sid).unwrap();
    assert!(db.active_session_id().unwrap().is_none());
}

#[test]
fn only_one_session_in_progress() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, b) = sample_plan(&db);
    db.start_session(a).unwrap();
    assert!(db.start_session(b).is_err());
}

#[test]
fn finish_keeps_only_performed_sets_and_extra_sets_work() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    let full = db.get_session_full(sid).unwrap();
    let se = full.exercises[0].id;
    let extra = db.add_session_set(se).unwrap();
    db.record_set(extra, 5, 50_000).unwrap();
    db.record_set(full.exercises[0].sets[0].id, 10, 40_000).unwrap();
    db.finish_session(sid, "").unwrap();
    let f = db.get_session_full(sid).unwrap();
    assert_eq!(f.exercises[0].sets.len(), 2);
    assert_eq!(f.exercises[1].sets.len(), 0);
    assert!(f.exercises[0].completed);
}

#[test]
fn rf15_evolution_by_exercise() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    for load in [40_000, 42_500, 45_000] {
        let sid = db.start_session(a).unwrap();
        complete_all_sets(&db, sid, load);
        db.finish_session(sid, "").unwrap();
    }
    let hx = db.history_exercises().unwrap();
    let supino = hx.iter().find(|h| h.name == "Supino reto com barra").unwrap();
    assert_eq!(supino.sessions, 3);
    let evo = db.exercise_evolution(&supino.key, None).unwrap();
    assert_eq!(evo.len(), 3);
    assert_eq!(evo[0].max_load_g, 45_000);
    assert_eq!(evo[2].max_load_g, 50_000);
    assert!(evo[2].volume_g > evo[0].volume_g);
}

#[test]
fn sequence_plans_rotate() {
    let mut db = Db::open_in_memory().unwrap();
    let plan = db.create_plan("ABC", "", ScheduleType::Sequence).unwrap();
    let a = db.add_day(plan, "A", 0).unwrap();
    let b = db.add_day(plan, "B", 0).unwrap();
    db.add_plan_exercise(a, ex(&db, "Prancha"), t(3, 1, 0), 30, "").unwrap();
    db.add_plan_exercise(b, ex(&db, "Esteira"), t(1, 1, 0), 0, "").unwrap();
    assert_eq!(db.next_sequence_day(plan).unwrap().unwrap().id, a);
    let sid = db.start_session(a).unwrap();
    complete_all_sets(&db, sid, 0);
    db.finish_session(sid, "").unwrap();
    assert_eq!(db.next_sequence_day(plan).unwrap().unwrap().id, b);
    db.move_day(b, -1).unwrap();
    assert_eq!(db.list_days(plan).unwrap()[0].id, b);
}

#[test]
fn calendar_planned_by_weekday() {
    let db = Db::open_in_memory().unwrap();
    sample_plan(&db);
    let plans = db.list_plans_full().unwrap();
    let t0 = today();
    let mut found = 0;
    for i in 0..7 {
        let d = t0 + chrono::Duration::days(i);
        found += planned_on(d, t0, &plans, &Default::default(), &[]).len();
    }
    assert_eq!(found, 3); // A: seg+qui, B: ter
}

#[test]
fn duplicate_plan_copies_everything() {
    let mut db = Db::open_in_memory().unwrap();
    let (plan, _, _) = sample_plan(&db);
    let copy = db.duplicate_plan(plan).unwrap();
    let f = db.get_plan_full(copy).unwrap();
    assert_eq!(f.plan.name, "ABC Hipertrofia (cópia)");
    assert_eq!(f.days.len(), 2);
    assert_eq!(f.days.iter().map(|d| d.exercises.len()).sum::<usize>(), 3);
}

#[test]
fn rf17_export_and_round_trip_import() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    complete_all_sets(&db, sid, 40_000);
    db.finish_session(sid, "").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("export.xlsx");
    let rows = export_xlsx(&db, ExportKind::All, &out).unwrap();
    assert!(rows >= 3 + 5);
    let wb = read_workbook(&out).unwrap();
    assert_eq!(wb.sheets[0].name, "Fichas");
    let m = detect_mapping(&wb.sheets[0]);
    let p = build_preview(&db, &wb.sheets[0], &m, ImportDefaults::default());
    assert!(p.can_import(), "{:?}", p.errors);
    assert_eq!(p.exercise_count(), 3);
    assert!(p.unrecognized_columns.is_empty(), "{:?}", p.unrecognized_columns);
    assert_eq!(p.plans[0].days[0].weekdays, weekday_bit(0) | weekday_bit(3));
    let tpl = dir.path().join("modelo.xlsx");
    export_template(&tpl).unwrap();
    let wb = read_workbook(&tpl).unwrap();
    let p = build_preview(&db, &wb.sheets[0], &detect_mapping(&wb.sheets[0]), ImportDefaults::default());
    assert!(p.can_import());
    assert_eq!(p.exercise_count(), 4);
    let ics = workout::import_export::ics::build_ics(&db, 2).unwrap();
    assert!(ics.contains("BEGIN:VEVENT") && ics.ends_with("END:VCALENDAR\r\n"));
}

#[test]
fn exercise_library_crud() {
    let db = Db::open_in_memory().unwrap();
    let id = db
        .create_exercise(&ExerciseInput {
            name: "  Remada   cavalinho ".into(),
            muscle_focus: "Costas".into(),
            equipment: "Barra".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(db.get_exercise(id).unwrap().name, "Remada cavalinho");
    assert!(db.create_exercise(&ExerciseInput { name: "remada cavalinho".into(), ..Default::default() }).is_err());
    db.set_exercise_archived(id, true).unwrap();
    assert!(db.list_exercises(&ExerciseFilter::default()).unwrap().iter().all(|e| e.id != id));
    let f = ExerciseFilter { muscle: "costas".into(), equipment: "barra".into(), ..Default::default() };
    assert!(db.list_exercises(&f).unwrap().iter().all(|e| e.muscle_focus == "Costas"));
}

#[test]
fn android_manifest_is_minimal() {
    // Seção 10 + evolução v1.1 (13.2): notificações e vibração; INTERNET e REQUEST_INSTALL_PACKAGES só para a
    // verificação/instalação de atualizações escolhida pelo usuário. Sem calendário/armazenamento amplo.
    let m = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/android/AndroidManifest.xml")).unwrap();
    let perms: Vec<&str> = m
        .lines()
        .filter(|l| l.contains("uses-permission"))
        .filter_map(|l| l.split("android:name=\"").nth(1))
        .filter_map(|s| s.split('"').next())
        .collect();
    assert_eq!(
        perms,
        vec![
            "android.permission.POST_NOTIFICATIONS",
            "android.permission.VIBRATE",
            "android.permission.INTERNET",
            "android.permission.REQUEST_INSTALL_PACKAGES"
        ]
    );
    assert!(m.contains("android:usesCleartextTraffic=\"false\""), "só HTTPS");
    for forbidden in ["CALENDAR", "STORAGE", "SCHEDULE_EXACT_ALARM", "LOCATION", "CAMERA", "CONTACTS", "ACCOUNTS"] {
        assert!(!perms.iter().any(|p| p.contains(forbidden)), "{forbidden}");
    }
}

#[test]
fn backup_with_password() {
    use workout::import_export::backup::read_manifest;
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("tmp");
    let db = Db::open(&dir.path().join("w.db")).unwrap();
    sample_plan(&db);
    let zip = dir.path().join("b.zip");
    assert!(create_backup(&db, &zip, &work, "0.1.4", Some("abc")).is_err()); // senha curta
    create_backup(&db, &zip, &work, "0.1.4", Some("segredo123")).unwrap();
    assert!(read_manifest(&zip).unwrap().encrypted);
    assert!(validate_backup(&zip, &work, None).is_err());
    let e = validate_backup(&zip, &work, Some("errada")).unwrap_err();
    assert!(e.to_string().contains("incorreta"), "{e}");
    let v = validate_backup(&zip, &work, Some("segredo123")).unwrap();
    assert_eq!(v.manifest.counts.plans, 1);
    // o banco não aparece em texto puro dentro do zip
    let raw = std::fs::read(&zip).unwrap();
    assert!(!raw.windows(15).any(|w| w == b"SQLite format 3"));
}

#[test]
fn rest_taken_is_recorded_and_frozen() {
    let mut db = Db::open_in_memory().unwrap();
    let (_, a, _) = sample_plan(&db);
    let sid = db.start_session(a).unwrap();
    let set = db.get_session_full(sid).unwrap().exercises[0].sets[0].id;
    assert!(db.set_rest_taken(set, 80).is_ok()); // série ainda não registrada: nada muda
    assert_eq!(db.get_session_full(sid).unwrap().exercises[0].sets[0].rest_taken_s, None);
    db.record_set(set, 10, 40_000).unwrap();
    db.set_rest_taken(set, 95).unwrap();
    db.finish_session(sid, "").unwrap();
    assert_eq!(db.get_session_full(sid).unwrap().exercises[0].sets[0].rest_taken_s, Some(95));
    assert!(db.set_rest_taken(set, 10).is_err());
}

/// 8.12 (documento de evolução v1.1): sessão interrompida (app fechado, bateria acabou) é
/// recuperada ao reabrir o banco, com as séries já registradas, sem duplicar e sem concluir sozinha.
#[test]
fn interrupted_session_is_recovered_after_reopen() {
    let dir = std::env::temp_dir().join(format!("empeto-recovery-{}", now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workout.db");
    let (sid, recorded) = {
        let mut db = Db::open(&path).unwrap();
        let (_plan, a, _b) = sample_plan(&db);
        let sid = db.start_session(a).unwrap();
        let sets = db.get_session_full(sid).unwrap().exercises[0].sets.clone();
        db.record_set(sets[0].id, 10, 40_000).unwrap();
        db.record_set(sets[1].id, 9, 42_500).unwrap();
        (sid, vec![(sets[0].id, 10, 40_000), (sets[1].id, 9, 42_500)])
        // `db` sai de escopo sem finalizar a sessão: simula o fechamento inesperado.
    };
    let mut db = Db::open(&path).unwrap();
    assert_eq!(db.active_session_id().unwrap(), Some(sid), "sessão aberta é encontrada");
    let f = db.get_session_full(sid).unwrap();
    assert_eq!(f.session.status, SessionStatus::InProgress, "não é concluída automaticamente");
    for (id, reps, load) in recorded {
        let s = f.exercises[0].sets.iter().find(|s| s.id == id).unwrap();
        assert_eq!((s.actual_reps, s.actual_load_g), (Some(reps), Some(load)));
    }
    // Não duplica: não é possível abrir outra sessão enquanto esta existir.
    let day = f.session.schedule_id.unwrap();
    assert!(db.start_session(day).is_err());
    assert_eq!(db.list_session_summaries(None, None).unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Evolução v1.1, 5.1/5.2: mídia dos exercícios persiste e a biblioteca inicial já vem ilustrada.
#[test]
fn exercise_media_is_persisted_and_seeded() {
    let db = Db::open_in_memory().unwrap();
    let seed = db.find_exercise_by_name("Supino reto com barra").unwrap().unwrap();
    assert_eq!(seed.illustration, "bench-press");
    assert_eq!(workout::exercise_media::resolve(&seed), "bench-press");
    let id = db
        .create_exercise(&ExerciseInput {
            name: "Remada cavalinho".into(),
            movement_pattern: "Puxar horizontal".into(),
            video_url: " https://example.com/remada ".into(),
            ..Default::default()
        })
        .unwrap();
    let e = db.get_exercise(id).unwrap();
    assert_eq!(e.video_url, "https://example.com/remada");
    assert_eq!(e.illustration, "");
    assert_eq!(workout::exercise_media::resolve(&e), "row"); // automática pelo padrão
    let mut input = ExerciseInput { name: e.name.clone(), illustration: "deadlift".into(), ..Default::default() };
    db.update_exercise(id, &input).unwrap();
    assert_eq!(db.get_exercise(id).unwrap().illustration, "deadlift");
    input.video_url = "ftp://x.y".into();
    assert!(db.update_exercise(id, &input).is_err(), "link inválido é recusado");
    input.illustration = "nao-existe".into();
    input.video_url.clear();
    db.update_exercise(id, &input).unwrap();
    assert_eq!(db.get_exercise(id).unwrap().illustration, "", "id desconhecido vira automática");
}
