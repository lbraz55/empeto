//! Controlador da interface: liga a UI Slint (Bridge) aos casos de uso em Rust.

mod format;

use crate::application::prefs::{Prefs, RestEndAction, TONE_NAMES};
use crate::calendar::{self as cal, date_key, local_date_of, local_time_of, parse_date_key};
use crate::domain::timer::{Clock, TimerEvent, TimerPhase, TimerState, TimerStatus};
use crate::domain::*;
use crate::history::EvolutionMetric;
use crate::import_export::backup::{create_backup, validate_backup, ValidatedBackup};
use crate::import_export::xlsx_export::{export_template, export_xlsx, ExportKind};
use crate::import_export::xlsx_import::{
    apply_import, build_preview, detect_mapping, read_workbook, ConflictPolicy, Field, ImportDefaults, ImportPreview,
    Mapping, Workbook,
};
use crate::persistence::{schema_version, Db};
use crate::platform::{FileResult, Platform, ALARM_END, ALARM_PRE};
use crate::training::ExerciseFilter;
use crate::ui::*;
use chrono::{Datelike, Duration, NaiveDate};
use format::*;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Páginas
const P_HOME: i32 = 0;
const P_PLANS: i32 = 1;
const P_CAL: i32 = 2;
const P_HIST: i32 = 3;
const P_EDITOR: i32 = 10;
const P_LIBRARY: i32 = 11;
const P_SESSION: i32 = 12;
const P_TIMER: i32 = 13;
const P_DETAIL: i32 = 14;
const P_EVO: i32 = 15;
const P_SETTINGS: i32 = 20;
const P_IMPORT: i32 = 21;

const XLSX_MIMES: &[&str] = &[
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-excel",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/octet-stream",
];
const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    DeletePlan(Id),
    DeleteDay(Id),
    DeletePlanEx(Id),
    FinishSession,
    DiscardSession,
    Restore,
    ArchiveExercise(Id, bool),
}

#[derive(Debug, Clone)]
enum ChoiceCtx {
    Tone(&'static str),
    Unit,
    LoadStep,
    LibFilter(&'static str, Vec<String>),
    ImportSheet,
    ImportMap(usize),
    ExportKind,
}

#[derive(Debug, Clone)]
enum TextCtx {
    SessionNotes,
    ExerciseNotes(Id),
    TimerCustom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InfoCtx {
    NotifPermission { then_enable: bool },
    ActiveSession,
    Plain,
}

#[derive(Debug, Clone, PartialEq)]
enum FileOp {
    ImportXlsx,
    Restore,
    Save(String),
}

#[derive(Debug, Clone, Default)]
struct PeCtx {
    day_id: Id,
    plan_ex_id: Option<Id>,
    exercise_id: Option<Id>,
}

struct ImportState {
    file_name: String,
    workbook: Workbook,
    sheet: usize,
    mapping: Mapping,
    preview: ImportPreview,
    replace: bool,
}

pub struct Controller {
    ui: slint::Weak<AppWindow>,
    db: Db,
    platform: Box<dyn Platform>,
    prefs: Prefs,
    stack: Vec<i32>,
    plan_id: Option<Id>,
    pe: Option<PeCtx>,
    day_ctx: Option<(Option<Id>, u8)>,
    exf_id: Option<Id>,
    lib_pick: bool,
    confirm: Option<ConfirmAction>,
    choice: Option<ChoiceCtx>,
    text: Option<TextCtx>,
    info: Option<InfoCtx>,
    session_id: Option<Id>,
    sub_se: Option<Id>,
    timer: TimerState,
    cal_month: NaiveDate,
    cal_selected: NaiveDate,
    cal_cache: Vec<(NaiveDate, Vec<cal::PlannedWorkout>, Vec<crate::history::SessionSummary>)>,
    detail_id: Option<Id>,
    evo_key: String,
    evo_period: i32,
    evo_metric: i32,
    import: Option<ImportState>,
    file_op: Option<FileOp>,
    pending_restore: Option<ValidatedBackup>,
    awaiting_permission: bool,
    toast_ticks: i32,
    in_background: bool,
}

thread_local! {
    static CONTROLLER: RefCell<Option<Rc<RefCell<Controller>>>> = const { RefCell::new(None) };
}

/// Ciclo de vida (Android): pausa/retomada do app.
pub fn lifecycle(foreground: bool) {
    slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
        CONTROLLER.with(|c| {
            if let Some(ctrl) = c.borrow().as_ref() {
                if let Ok(mut s) = ctrl.try_borrow_mut() {
                    if foreground {
                        s.on_resume();
                    } else {
                        s.on_pause();
                    }
                }
            }
        });
    });
}

impl Controller {
    fn ui(&self) -> AppWindow {
        self.ui.upgrade().expect("janela ativa")
    }

    fn with_bridge<R>(&self, f: impl FnOnce(&Bridge) -> R) -> R {
        let ui = self.ui();
        let b = ui.global::<Bridge>();
        f(&b)
    }

    fn toast(&mut self, msg: impl AsRef<str>) {
        let m = msg.as_ref().to_string();
        self.with_bridge(|b| b.set_toast(ss(&m)));
        self.toast_ticks = 14; // ~3.5 s
    }

    fn report(&mut self, r: AppResult<()>) {
        if let Err(e) = r {
            log::warn!("{e}");
            self.toast(e.to_string());
        }
    }

    fn show_info(&mut self, ctx: InfoCtx, title: &str, text: &str, ok: &str, alt: &str) {
        self.info = Some(ctx);
        self.with_bridge(|b| {
            b.set_info_title(ss(title));
            b.set_info_text(ss(text));
            b.set_info_ok(ss(ok));
            b.set_info_alt(ss(alt));
            b.set_dialog(ss("info"));
        });
    }

    fn ask_confirm(&mut self, action: ConfirmAction, title: &str, text: &str, ok: &str) {
        self.confirm = Some(action);
        self.with_bridge(|b| {
            b.set_confirm_title(ss(title));
            b.set_confirm_text(ss(text));
            b.set_confirm_ok(ss(ok));
            b.set_dialog(ss("confirm"));
        });
    }

    fn ask_choice(&mut self, ctx: ChoiceCtx, title: &str, text: &str, options: Vec<String>, selected: i32) {
        self.choice = Some(ctx);
        self.with_bridge(|b| {
            b.set_choice_title(ss(title));
            b.set_choice_text(ss(text));
            b.set_choice_options(model(options.iter().map(ss).collect()));
            b.set_choice_selected(selected);
            b.set_dialog(ss("choice"));
        });
    }

    fn ask_text(&mut self, ctx: TextCtx, title: &str, hint: &str, value: &str, multiline: bool, numeric: bool) {
        self.text = Some(ctx);
        self.with_bridge(|b| {
            b.set_text_title(ss(title));
            b.set_text_hint(ss(hint));
            b.set_text_value(ss(value));
            b.set_text_multiline(multiline);
            b.set_text_numeric(numeric);
            b.set_dialog(ss("text"));
        });
    }

    fn close_dialog(&mut self) {
        self.with_bridge(|b| b.set_dialog(ss("")));
        self.confirm = None;
        self.choice = None;
        self.text = None;
        self.info = None;
    }

    fn page(&self) -> i32 {
        self.with_bridge(|b| b.get_page())
    }

    // ------------------------------------------------------------------
    // Navegação
    // ------------------------------------------------------------------

    fn navigate(&mut self, page: i32) {
        if page < 10 {
            self.stack.clear();
        } else if self.stack.last() == Some(&page) {
            self.stack.pop();
        }
        self.stack.push(page);
        self.show_page(page);
    }

    fn show_page(&mut self, page: i32) {
        if page == P_SETTINGS && self.page() != P_SETTINGS {
            self.with_bridge(|b| b.set_settings_section(0));
        }
        self.with_bridge(|b| b.set_page(page));
        self.refresh_page(page);
    }

    fn refresh_page(&mut self, page: i32) {
        let r = match page {
            P_HOME => self.refresh_home(),
            P_PLANS => self.refresh_plans(),
            P_CAL => self.refresh_calendar(),
            P_HIST => self.refresh_history(),
            P_EDITOR => self.refresh_editor(),
            P_LIBRARY => self.refresh_library(),
            P_SESSION => self.refresh_session(),
            P_TIMER => {
                self.refresh_timer();
                Ok(())
            }
            P_DETAIL => self.refresh_detail(),
            P_EVO => self.refresh_evolution(),
            P_SETTINGS => self.refresh_settings(),
            P_IMPORT => {
                self.refresh_import();
                Ok(())
            }
            _ => Ok(()),
        };
        self.report(r);
    }

    fn back(&mut self) {
        if self.with_bridge(|b| b.get_dialog()) != "" {
            self.close_dialog();
            return;
        }
        let page = self.page();
        if page == P_SETTINGS && self.with_bridge(|b| b.get_settings_section()) > 0 {
            self.with_bridge(|b| b.set_settings_section(0));
            return;
        }
        if page == P_EDITOR {
            self.autosave_plan_header();
        }
        if page == P_LIBRARY && self.lib_pick {
            self.lib_pick = false;
            self.with_bridge(|b| b.set_lib_mode(ss("browse")));
            if self.pe.as_ref().map(|p| p.plan_ex_id.is_some() || p.exercise_id.is_some()).unwrap_or(false) {
                self.stack.pop();
                let prev = *self.stack.last().unwrap_or(&P_EDITOR);
                self.show_page(prev);
                self.with_bridge(|b| b.set_dialog(ss("plan-ex")));
                return;
            }
            self.pe = None;
        }
        if page == P_IMPORT {
            self.import = None;
        }
        self.stack.pop();
        match self.stack.last().copied() {
            Some(p) => self.show_page(p),
            None => {
                if page != P_HOME {
                    self.navigate(P_HOME);
                } else {
                    self.platform.move_to_back();
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Home
    // ------------------------------------------------------------------

    fn next_seq_map(&self, plans: &[PlanFull]) -> HashMap<Id, Id> {
        let mut m = HashMap::new();
        for p in plans.iter().filter(|p| p.plan.schedule_type == ScheduleType::Sequence) {
            if let Ok(Some(d)) = self.db.next_sequence_day(p.plan.id) {
                m.insert(p.plan.id, d.id);
            }
        }
        m
    }

    fn sessions_on(&self, d: NaiveDate) -> AppResult<Vec<crate::history::SessionSummary>> {
        let (a, b) = cal::day_bounds_ms(d);
        self.db.list_session_summaries(Some(a), Some(b))
    }

    fn refresh_home(&mut self) -> AppResult<()> {
        let today = cal::today();
        let plans = self.db.list_plans_full()?;
        let next = self.next_seq_map(&plans);
        let done_today = self.sessions_on(today)?;
        let done_plan_ids: Vec<Id> = done_today
            .iter()
            .filter_map(|s| self.db.get_session(s.id).ok().and_then(|x| x.plan_id))
            .collect();
        let mut cards: Vec<CardRow> = Vec::new();
        let card = |kind: &str, title: &str, l1: String, l2: String, action: &str, id: Id| CardRow {
            kind: ss(kind),
            title: ss(title),
            line1: ss(l1),
            line2: ss(l2),
            action: ss(action),
            ref_id: id as i32,
        };

        if let Some(sid) = self.db.active_session_id()? {
            let f = self.db.get_session_full(sid)?;
            cards.push(card(
                "session",
                "SESSÃO EM ANDAMENTO",
                format!("{} — {}", f.session.plan_name, f.session.schedule_name),
                format!(
                    "{} de {} séries · iniciada às {}",
                    f.completed_sets(),
                    f.total_sets(),
                    local_time_of(f.session.started_at)
                ),
                "Continuar",
                sid,
            ));
        }

        let planned = cal::planned_on(today, today, &plans, &next, &done_plan_ids);
        if planned.is_empty() {
            let l2 = if plans.is_empty() {
                "Crie sua primeira ficha em Treinos.".to_string()
            } else {
                "Nenhum treino planejado para hoje. Toque para escolher uma ficha.".to_string()
            };
            let l1 = if done_today.is_empty() { "Dia livre".to_string() } else { "Treino do dia concluído".to_string() };
            cards.push(card("plans", "TREINO DE HOJE", l1, l2, "Treinos", 0));
        } else {
            for p in &planned {
                let done = done_today.iter().any(|s| s.schedule_name == p.day_name && s.plan_name == p.plan_name);
                cards.push(card(
                    "today",
                    if p.from_sequence { "TREINO DE HOJE · PRÓXIMO DA SEQUÊNCIA" } else { "TREINO DE HOJE" },
                    format!("{} — {}", p.plan_name, p.day_name),
                    format!("{} exercício(s){}", p.exercises, if done { " · já realizado hoje" } else { "" }),
                    "Iniciar",
                    p.day_id,
                ));
            }
        }

        let mut upcoming = Vec::new();
        for i in 1..=7 {
            let d = today + Duration::days(i);
            for p in cal::planned_on(d, today, &plans, &next, &[]) {
                upcoming.push(format!(
                    "{} {:02}/{:02} — {} ({})",
                    WEEKDAY_SHORT[cal::weekday_index(d) as usize],
                    d.day(),
                    d.month(),
                    p.day_name,
                    p.plan_name
                ));
            }
        }
        let seq_names: Vec<String> = plans
            .iter()
            .filter(|p| p.plan.active && p.plan.schedule_type == ScheduleType::Sequence)
            .filter(|p| !planned.iter().any(|x| x.plan_id == p.plan.id))
            .filter_map(|p| {
                let did = next.get(&p.plan.id)?;
                let d = p.days.iter().find(|d| d.day.id == *did)?;
                Some(format!("Próximo da sequência: {} ({})", d.day.name, p.plan.name))
            })
            .collect();
        if !upcoming.is_empty() || !seq_names.is_empty() {
            let mut lines: Vec<String> = upcoming.into_iter().take(3).collect();
            lines.extend(seq_names.into_iter().take(2));
            let first = lines.remove(0);
            cards.push(card("calendar", "PRÓXIMOS TREINOS", first, lines.join("\n"), "Calendário", 0));
        }

        let all = self.db.list_session_summaries(None, None)?;
        if let Some(last) = all.first() {
            let dur = last.ended_at.map(|e| format!(" · {}", fmt_minutes(e - last.started_at))).unwrap_or_default();
            cards.push(card(
                "last",
                "ÚLTIMA SESSÃO",
                format!("{} — {}", last.plan_name, last.schedule_name),
                format!(
                    "{} · {} exercício(s) · {} série(s){}",
                    fmt_date_time(last.started_at),
                    last.exercises,
                    last.sets,
                    dur
                ),
                "Detalhes",
                last.id,
            ));
        }

        let ws = cal::week_start(today, self.prefs.week_starts_monday);
        let (a, _) = cal::day_bounds_ms(ws);
        let (_, b) = cal::day_bounds_ms(ws + Duration::days(6));
        let week = self.db.list_session_summaries(Some(a), Some(b))?;
        let mut planned_week = 0;
        for i in 0..7 {
            let d = ws + Duration::days(i);
            planned_week += cal::planned_on(d, today, &plans, &HashMap::new(), &[]).len();
        }
        let sets: i64 = week.iter().map(|s| s.sets).sum();
        cards.push(card(
            "week",
            "SESSÕES NA SEMANA",
            format!("{} sessão(ões) concluída(s)", week.len()),
            if planned_week > 0 {
                format!("{} planejada(s) por dia da semana · {} séries registradas", planned_week, sets)
            } else {
                format!("{} séries registradas", sets)
            },
            "Histórico",
            0,
        ));

        let date = cal::date_long(today);
        self.with_bridge(|b| {
            b.set_home_date(ss(capitalize(&date)));
            b.set_home_cards(model(cards));
        });
        Ok(())
    }

    fn home_card(&mut self, kind: &str, id: Id) {
        match kind {
            "session" => self.open_session(),
            "today" => self.start_day(id),
            "plans" => self.navigate(P_PLANS),
            "calendar" => self.navigate(P_CAL),
            "last" => self.open_detail(id),
            "week" => self.navigate(P_HIST),
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Fichas
    // ------------------------------------------------------------------

    fn refresh_plans(&mut self) -> AppResult<()> {
        let rows: Vec<PlanRow> = self
            .db
            .list_plans_full()?
            .into_iter()
            .map(|p| {
                let n: usize = p.days.iter().map(|d| d.exercises.len()).sum();
                let org = match p.plan.schedule_type {
                    ScheduleType::Weekday => {
                        let mask = p.days.iter().fold(0u8, |m, d| m | d.day.weekdays);
                        if mask == 0 { "Dias da semana".to_string() } else { weekdays_text(mask) }
                    }
                    ScheduleType::Sequence => format!(
                        "Sequência {}",
                        p.days.iter().map(|d| d.day.name.clone()).collect::<Vec<_>>().join(" → ")
                    ),
                };
                PlanRow {
                    id: p.plan.id as i32,
                    name: ss(&p.plan.name),
                    subtitle: ss(format!("{} dia(s) · {} exercício(s) · {}", p.days.len(), n, org)),
                    active: p.plan.active,
                }
            })
            .collect();
        self.with_bridge(|b| b.set_plans(model(rows)));
        Ok(())
    }

    fn new_plan(&mut self) {
        let mut name = "Nova ficha".to_string();
        let mut i = 2;
        while self.db.find_plan_by_name(&name).ok().flatten().is_some() {
            name = format!("Nova ficha {i}");
            i += 1;
        }
        match self.db.create_plan(&name, "", ScheduleType::Weekday) {
            Ok(id) => {
                self.plan_id = Some(id);
                self.navigate(P_EDITOR);
                self.toast("Ficha criada. Dê um nome e adicione os dias de treino.");
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn open_plan(&mut self, id: Id) {
        self.plan_id = Some(id);
        self.navigate(P_EDITOR);
    }

    fn autosave_plan_header(&mut self) {
        let Some(id) = self.plan_id else { return };
        let (name, desc, weekday) =
            self.with_bridge(|b| (b.get_edit_plan_name().to_string(), b.get_edit_plan_desc().to_string(), b.get_edit_plan_weekday()));
        if let Ok(p) = self.db.get_plan(id) {
            if p.name != clean_text(&name, 80) || p.description != clean_notes(&desc) {
                let st = if weekday { ScheduleType::Weekday } else { ScheduleType::Sequence };
                let r = self.db.update_plan(id, &name, &desc, st);
                self.report(r);
            }
        }
    }

    fn refresh_editor(&mut self) -> AppResult<()> {
        let Some(id) = self.plan_id else { return Ok(()) };
        let p = self.db.get_plan_full(id)?;
        let unit = self.prefs.unit;
        let days: Vec<DayRow> = p
            .days
            .iter()
            .map(|d| {
                let subtitle = match p.plan.schedule_type {
                    ScheduleType::Weekday => {
                        if d.day.weekdays == 0 {
                            "Sem dia da semana definido — toque em editar".to_string()
                        } else {
                            weekdays_text(d.day.weekdays)
                        }
                    }
                    ScheduleType::Sequence => format!("Posição {} na sequência", d.day.sequence_order + 1),
                };
                DayRow {
                    id: d.day.id as i32,
                    name: ss(&d.day.name),
                    subtitle: ss(subtitle),
                    exercises: model(
                        d.exercises
                            .iter()
                            .map(|e| PlanExRow {
                                id: e.id as i32,
                                name: ss(&e.exercise.name),
                                detail: ss(target_text(&e.target, e.rest_seconds, unit)),
                                notes: ss(&e.notes),
                            })
                            .collect(),
                    ),
                }
            })
            .collect();
        self.with_bridge(|b| {
            b.set_edit_plan_id(id as i32);
            b.set_edit_plan_name(ss(&p.plan.name));
            b.set_edit_plan_desc(ss(&p.plan.description));
            b.set_edit_plan_weekday(p.plan.schedule_type == ScheduleType::Weekday);
            b.set_edit_plan_active(p.plan.active);
            b.set_edit_days(model(days));
        });
        Ok(())
    }

    fn save_plan_header(&mut self, name: &str, desc: &str) {
        let Some(id) = self.plan_id else { return };
        let weekday = self.with_bridge(|b| b.get_edit_plan_weekday());
        let st = if weekday { ScheduleType::Weekday } else { ScheduleType::Sequence };
        match self.db.update_plan(id, name, desc, st) {
            Ok(()) => {
                self.toast("Ficha salva.");
                self.report_refresh_editor();
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn report_refresh_editor(&mut self) {
        let r = self.refresh_editor();
        self.report(r);
    }

    fn set_plan_schedule(&mut self, weekday: bool) {
        let Some(id) = self.plan_id else { return };
        self.autosave_plan_header();
        let r = self.db.get_plan(id).and_then(|p| {
            self.db.update_plan(id, &p.name, &p.description, if weekday { ScheduleType::Weekday } else { ScheduleType::Sequence })
        });
        self.report(r);
        self.report_refresh_editor();
    }

    fn start_day(&mut self, day_id: Id) {
        if let Ok(Some(_)) = self.db.active_session_id() {
            self.show_info(
                InfoCtx::ActiveSession,
                "Sessão em andamento",
                "Já existe uma sessão em andamento. Continue-a ou finalize/descarte antes de iniciar outra.",
                "Abrir sessão",
                "Fechar",
            );
            return;
        }
        if self.page() == P_EDITOR {
            self.autosave_plan_header();
        }
        match self.db.start_session(day_id) {
            Ok(sid) => {
                self.session_id = Some(sid);
                self.with_bridge(|b| b.set_session_index(0));
                self.navigate(P_SESSION);
                if self.prefs.keep_screen_on {
                    self.platform.keep_screen_on(true);
                }
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    // Dias
    fn open_day_dialog(&mut self, day: Option<Id>) {
        let Some(plan_id) = self.plan_id else { return };
        let weekday_plan = self.db.get_plan(plan_id).map(|p| p.schedule_type == ScheduleType::Weekday).unwrap_or(true);
        let (name, mask) = match day.and_then(|d| self.db.get_day(d).ok()) {
            Some(d) => (d.name, d.weekdays),
            None => {
                let n = self.db.list_days(plan_id).map(|v| v.len()).unwrap_or(0);
                let letter = (b'A' + (n as u8 % 26)) as char;
                (format!("Treino {letter}"), 0)
            }
        };
        self.day_ctx = Some((day, mask));
        self.with_bridge(|b| {
            b.set_day_title(ss(if day.is_some() { "Editar dia" } else { "Novo dia de treino" }));
            b.set_day_name(ss(&name));
            b.set_day_show_weekdays(weekday_plan);
            b.set_day_weekdays(model((0..7).map(|i| mask & (1 << i) != 0).collect()));
            b.set_dialog(ss("day"));
        });
    }

    fn day_toggle(&mut self, i: i32) {
        if let Some((_, mask)) = self.day_ctx.as_mut() {
            *mask ^= 1 << (i as u8);
            let m = *mask;
            self.with_bridge(|b| b.set_day_weekdays(model((0..7).map(|i| m & (1 << i) != 0).collect())));
        }
    }

    fn day_save(&mut self, name: &str) {
        let Some((day, mask)) = self.day_ctx.clone() else { return };
        let Some(plan_id) = self.plan_id else { return };
        let r = match day {
            Some(id) => self.db.update_day(id, name, mask),
            None => self.db.add_day(plan_id, name, mask).map(|_| ()),
        };
        match r {
            Ok(()) => {
                self.close_dialog();
                self.day_ctx = None;
                self.report_refresh_editor();
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    // Exercício da ficha
    fn fill_pe_form(&mut self, title: &str, ex_name: &str, t: Target, rest: i64, notes: &str, can_delete: bool) {
        let unit = self.prefs.unit;
        self.with_bridge(|b| {
            b.set_pe_title(ss(title));
            b.set_pe_exercise(ss(ex_name));
            b.set_pe_sets(ss(t.sets.to_string()));
            b.set_pe_reps(ss(format_reps(t.reps_min, t.reps_max).replace('–', "-")));
            b.set_pe_load(ss(if t.load_g > 0 { format_load(t.load_g, unit) } else { String::new() }));
            b.set_pe_rest(ss(rest.to_string()));
            b.set_pe_notes(ss(notes));
            b.set_pe_unit(ss(unit.as_str()));
            b.set_pe_can_delete(can_delete);
        });
    }

    fn add_plan_ex(&mut self, day_id: Id) {
        self.autosave_plan_header();
        self.pe = Some(PeCtx { day_id, plan_ex_id: None, exercise_id: None });
        let defaults = Target {
            sets: self.prefs.default_sets,
            reps_min: self.prefs.default_reps,
            reps_max: self.prefs.default_reps,
            load_g: 0,
        };
        self.fill_pe_form("Adicionar exercício", "", defaults, self.prefs.timer_default_seconds, "", false);
        self.open_library(true);
    }

    fn edit_plan_ex(&mut self, id: Id) {
        match self.db.get_plan_exercise(id) {
            Ok(pe) => {
                self.pe = Some(PeCtx { day_id: pe.schedule_id, plan_ex_id: Some(id), exercise_id: Some(pe.exercise.id) });
                self.fill_pe_form("Editar exercício", &pe.exercise.name, pe.target, pe.rest_seconds, &pe.notes, true);
                self.with_bridge(|b| b.set_dialog(ss("plan-ex")));
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn pe_save(&mut self, sets: &str, reps: &str, load: &str, rest: &str, notes: &str) {
        let Some(ctx) = self.pe.clone() else { return };
        let r = (|| -> AppResult<()> {
            let Some(ex_id) = ctx.exercise_id else { return validation("Escolha o exercício.") };
            let sets: i64 = sets.trim().parse().map_err(|_| AppError::Validation("Séries: informe um número.".into()))?;
            let (rmin, rmax) = parse_reps(reps)?;
            let load_g = parse_load(load, self.prefs.unit)?;
            let rest = if rest.trim().is_empty() { 0 } else { parse_rest(rest)? };
            let t = Target { sets, reps_min: rmin, reps_max: rmax, load_g };
            match ctx.plan_ex_id {
                Some(id) => self.db.update_plan_exercise(id, ex_id, t, rest, notes),
                None => self.db.add_plan_exercise(ctx.day_id, ex_id, t, rest, notes).map(|_| ()),
            }
        })();
        match r {
            Ok(()) => {
                self.close_dialog();
                self.pe = None;
                self.report_refresh_editor();
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    // ------------------------------------------------------------------
    // Biblioteca
    // ------------------------------------------------------------------

    fn open_library(&mut self, pick: bool) {
        self.lib_pick = pick;
        self.with_bridge(|b| {
            b.set_lib_mode(ss(if pick { "pick" } else { "browse" }));
            b.set_dialog(ss(""));
        });
        self.navigate(P_LIBRARY);
    }

    fn lib_filter_struct(&self) -> ExerciseFilter {
        self.with_bridge(|b| ExerciseFilter {
            text: b.get_lib_search().to_string(),
            muscle: b.get_lib_muscle().to_string(),
            equipment: b.get_lib_equipment().to_string(),
            pattern: b.get_lib_pattern().to_string(),
            include_archived: b.get_lib_show_archived() && !self.lib_pick,
        })
    }

    fn refresh_library(&mut self) -> AppResult<()> {
        let f = self.lib_filter_struct();
        let rows: Vec<ExRow> = self.db.list_exercises(&f)?.into_iter().map(|e| ex_row(&e, "")).collect();
        self.with_bridge(|b| b.set_library(model(rows)));
        Ok(())
    }

    fn lib_item(&mut self, id: Id) {
        if self.lib_pick {
            let Ok(ex) = self.db.get_exercise(id) else { return };
            if let Some(pe) = self.pe.as_mut() {
                pe.exercise_id = Some(id);
            }
            self.lib_pick = false;
            self.with_bridge(|b| {
                b.set_pe_exercise(ss(&ex.name));
                b.set_lib_mode(ss("browse"));
            });
            self.stack.pop();
            let prev = *self.stack.last().unwrap_or(&P_EDITOR);
            self.show_page(prev);
            self.with_bridge(|b| b.set_dialog(ss("plan-ex")));
        } else {
            self.open_exercise_form(Some(id));
        }
    }

    fn open_exercise_form(&mut self, id: Option<Id>) {
        let e = id.and_then(|i| self.db.get_exercise(i).ok()).unwrap_or_default();
        self.exf_id = id;
        self.with_bridge(|b| {
            b.set_exf_title(ss(if id.is_some() { "Editar exercício" } else { "Novo exercício" }));
            b.set_exf_name(ss(&e.name));
            b.set_exf_muscle(ss(&e.muscle_focus));
            b.set_exf_type(ss(&e.exercise_type));
            b.set_exf_equipment(ss(&e.equipment));
            b.set_exf_pattern(ss(&e.movement_pattern));
            b.set_exf_editing(id.is_some());
            b.set_exf_archived(e.archived);
            b.set_dialog(ss("exercise"));
        });
    }

    fn exf_save(&mut self, input: ExerciseInput) {
        let r = match self.exf_id {
            Some(id) => self.db.update_exercise(id, &input).map(|_| id),
            None => self.db.create_exercise(&input),
        };
        match r {
            Ok(id) => {
                self.close_dialog();
                let r = self.refresh_library();
                self.report(r);
                if self.lib_pick && self.exf_id.is_none() {
                    self.lib_item(id);
                } else {
                    self.toast("Exercício salvo.");
                }
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn lib_choose_filter(&mut self, field: &'static str) {
        let values = self.db.exercise_field_values(field).unwrap_or_default();
        let current = self.with_bridge(|b| match field {
            "muscle" => b.get_lib_muscle(),
            "equipment" => b.get_lib_equipment(),
            _ => b.get_lib_pattern(),
        });
        let mut opts = vec!["Todos".to_string()];
        opts.extend(values.iter().cloned());
        let sel = values.iter().position(|v| v.as_str() == current.as_str()).map(|i| i as i32 + 1).unwrap_or(0);
        let title = match field {
            "muscle" => "Grupo muscular",
            "equipment" => "Equipamento",
            _ => "Padrão de movimento",
        };
        self.ask_choice(ChoiceCtx::LibFilter(field, values), title, "", opts, sel);
    }

    // ------------------------------------------------------------------
    // Sessão
    // ------------------------------------------------------------------

    fn open_session(&mut self) {
        match self.db.active_session_id() {
            Ok(Some(sid)) => {
                self.session_id = Some(sid);
                self.navigate(P_SESSION);
                if self.prefs.keep_screen_on {
                    self.platform.keep_screen_on(true);
                }
            }
            _ => self.toast("Nenhuma sessão em andamento."),
        }
    }

    fn refresh_session(&mut self) -> AppResult<()> {
        let sid = match self.session_id.or(self.db.active_session_id()?) {
            Some(s) => s,
            None => {
                self.with_bridge(|b| b.set_session_active(false));
                return Ok(());
            }
        };
        self.session_id = Some(sid);
        let f = self.db.get_session_full(sid)?;
        let unit = self.prefs.unit;
        let mut rows = Vec::new();
        for e in &f.exercises {
            let last = if self.prefs.prefill_last {
                e.performed_exercise_id.and_then(|id| self.db.last_performance(id, sid).ok().flatten())
            } else {
                None
            };
            let done_sets = e.sets.iter().filter(|s| s.completed_at.is_some()).count();
            let sets: Vec<SetRow> = e
                .sets
                .iter()
                .map(|s| {
                    let prev = last.as_ref().and_then(|l| l.sets.iter().find(|x| x.0 == s.set_number));
                    let (load, reps) = match (s.actual_load_g, s.actual_reps) {
                        (Some(l), Some(r)) if s.completed_at.is_some() => (format_load(l, unit), r.to_string()),
                        _ => {
                            let l = prev.and_then(|p| p.2).unwrap_or(s.planned_load_g);
                            (if l > 0 { format_load(l, unit) } else { String::new() }, s.planned_reps_max.to_string())
                        }
                    };
                    let hint = match prev {
                        Some((_, Some(r), Some(l))) => format!(
                            "Última vez: {} × {} ({})",
                            format_load_unit(*l, unit),
                            r,
                            fmt_date_short(last.as_ref().unwrap().session_started_at)
                        ),
                        _ => String::new(),
                    };
                    SetRow {
                        id: s.id as i32,
                        number: s.set_number as i32,
                        planned: ss(format!(
                            "plan. {}{}",
                            if s.planned_load_g > 0 { format!("{} × ", format_load_unit(s.planned_load_g, unit)) } else { String::new() },
                            format_reps(s.planned_reps_min, s.planned_reps_max)
                        )),
                        load: ss(load),
                        reps: ss(reps),
                        done: s.completed_at.is_some(),
                        hint: ss(hint),
                    }
                })
                .collect();
            let planned = match e.sets.first() {
                Some(s) => format!(
                    "{} série(s) · {} reps{}{}",
                    e.sets.len(),
                    format_reps(s.planned_reps_min, s.planned_reps_max),
                    if s.planned_load_g > 0 { format!(" · {}", format_load_unit(s.planned_load_g, unit)) } else { String::new() },
                    if e.rest_seconds > 0 { format!(" · descanso {}", format_duration_s(e.rest_seconds)) } else { String::new() }
                ),
                None => "Sem séries".to_string(),
            };
            let substitution = match e.substitution {
                SubstitutionType::None => String::new(),
                SubstitutionType::Temporary => format!("Substitui {} · só nesta sessão", e.original_name),
                SubstitutionType::Permanent => format!("Substitui {} · ficha atualizada", e.original_name),
            };
            rows.push(SessionExRow {
                id: e.id as i32,
                name: ss(&e.performed_name),
                original: ss(&e.original_name),
                muscle: ss(&e.muscle_focus),
                kind: ss(&e.exercise_type),
                equipment: ss(&e.equipment),
                substitution: ss(substitution),
                planned: ss(planned),
                rest: e.rest_seconds as i32,
                notes: ss(&e.notes),
                completed: e.completed,
                done_sets: done_sets as i32,
                sets: model(sets),
            });
        }
        let n = rows.len() as i32;
        let total = f.total_sets();
        let done = f.completed_sets();
        self.with_bridge(|b| {
            b.set_session_active(true);
            b.set_session_title(ss(&f.session.schedule_name));
            b.set_session_subtitle(ss(&f.session.plan_name));
            b.set_session_progress(ss(format!("{} · {}/{} séries", f.session.plan_name, done, total)));
            b.set_session_notes(ss(&f.session.notes));
            b.set_session_unit(ss(unit.as_str()));
            if b.get_session_index() >= n {
                b.set_session_index((n - 1).max(0));
            }
            b.set_session_exercises(model(rows));
        });
        self.update_elapsed();
        Ok(())
    }

    fn update_elapsed(&mut self) {
        let Some(sid) = self.session_id else { return };
        if let Ok(s) = self.db.get_session(sid) {
            let secs = (now_ms() - s.started_at) / 1000;
            self.with_bridge(|b| b.set_session_elapsed(ss(format_duration_s(secs))));
        }
    }

    fn session_record(&mut self, set_id: Id, load: &str, reps: &str) {
        let unit = self.prefs.unit;
        let r = (|| -> AppResult<(String, i64)> {
            let load_g = parse_load(load, unit)?;
            let reps_n: i64 = if reps.trim().is_empty() {
                return validation("Informe as repetições realizadas.");
            } else {
                reps.trim().parse().map_err(|_| AppError::Validation("Repetições inválidas.".into()))?
            };
            self.db.record_set(set_id, reps_n, load_g)?;
            let sid = self.session_id.unwrap_or(0);
            let f = self.db.get_session_full(sid)?;
            let (ex, set) = f
                .exercises
                .iter()
                .find_map(|e| e.sets.iter().find(|s| s.id == set_id).map(|s| (e.clone(), s.clone())))
                .ok_or_else(|| AppError::NotFound("Série não encontrada.".into()))?;
            Ok((format!("{} — após a série {}", ex.performed_name, set.set_number), ex.rest_seconds))
        })();
        match r {
            Ok((context, rest)) => {
                let r = self.refresh_session();
                self.report(r);
                if self.prefs.auto_start_rest {
                    let secs = if rest > 0 { rest } else { self.prefs.timer_default_seconds };
                    self.start_rest(secs, &context);
                    self.toast(format!("Série registrada. Descanso de {} iniciado.", format_duration_s(secs)));
                } else {
                    self.timer.context = context;
                    self.timer.set_duration(if rest > 0 { rest * 1000 } else { self.timer.duration_ms });
                    self.save_timer();
                    self.refresh_timer();
                }
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn adjust_load(&self, text: &str, dir: i32) -> String {
        let unit = self.prefs.unit;
        let cur = parse_load(text, unit).unwrap_or(0);
        let step = self.prefs.load_step_g;
        let v = (cur + dir as i64 * step).clamp(0, MAX_LOAD_G);
        format_load(v, unit)
    }

    fn open_substitute(&mut self, se_id: Id) {
        let Some(sid) = self.session_id else { return };
        let Ok(f) = self.db.get_session_full(sid) else { return };
        let Some(e) = f.exercises.iter().find(|e| e.id == se_id).cloned() else { return };
        self.sub_se = Some(se_id);
        let base = e.performed_exercise_id.or(e.original_exercise_id);
        let suggestions: Vec<ExRow> = base
            .and_then(|id| self.db.suggested_alternatives(id).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(x, reason)| ex_row(&x, &reason))
            .collect();
        let original = if e.substitution == SubstitutionType::None {
            e.performed_name.clone()
        } else {
            format!("{} (original: {})", e.performed_name, e.original_name)
        };
        self.with_bridge(|b| {
            b.set_sub_original(ss(original));
            b.set_sub_search(ss(""));
            b.set_sub_suggestions(model(suggestions));
            b.set_sub_results(model(vec![]));
            b.set_sub_selected_id(-1);
            b.set_sub_selected_name(ss(""));
            b.set_sub_permanent(false);
            b.set_sub_can_permanent(e.plan_exercise_id.is_some());
            b.set_dialog(ss("substitute"));
        });
    }

    fn sub_search(&mut self, text: &str) {
        let f = ExerciseFilter { text: text.to_string(), ..Default::default() };
        let rows: Vec<ExRow> =
            self.db.list_exercises(&f).unwrap_or_default().into_iter().take(60).map(|e| ex_row(&e, "")).collect();
        self.with_bridge(|b| b.set_sub_results(model(rows)));
    }

    fn sub_confirm(&mut self, permanent: bool) {
        let Some(se) = self.sub_se else { return };
        let id = self.with_bridge(|b| b.get_sub_selected_id()) as Id;
        if id <= 0 {
            self.toast("Escolha o exercício substituto.");
            return;
        }
        match self.db.substitute_exercise(se, id, permanent) {
            Ok(()) => {
                self.close_dialog();
                let r = self.refresh_session();
                self.report(r);
                self.toast(if permanent {
                    "Substituído. A ficha foi atualizada para as próximas sessões."
                } else {
                    "Substituído somente nesta sessão. A ficha não mudou."
                });
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn finish_session_now(&mut self) {
        let Some(sid) = self.session_id else { return };
        let notes = self.db.get_session(sid).map(|s| s.notes).unwrap_or_default();
        match self.db.finish_session(sid, &notes) {
            Ok(()) => {
                self.session_id = None;
                self.platform.keep_screen_on(false);
                self.with_bridge(|b| b.set_session_active(false));
                self.stack.clear();
                self.stack.push(P_HIST);
                self.open_detail(sid);
                self.toast("Sessão finalizada e salva no histórico.");
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    // ------------------------------------------------------------------
    // Cronômetro
    // ------------------------------------------------------------------

    fn save_timer(&self) {
        if let Ok(j) = serde_json::to_string(&self.timer) {
            let _ = self.db.set_pref("timer.state", &j);
        }
    }

    fn load_timer(&mut self) {
        let now = Clock::now();
        let mut t = self
            .db
            .get_pref("timer.state")
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str::<TimerState>(&j).ok())
            .unwrap_or_else(|| TimerState::new(self.prefs.timer_default_seconds * 1000));
        t.restore(now);
        // Se terminou enquanto o app estava fechado, apenas marca como encerrado (o alerta já foi
        // emitido pela notificação agendada, se permitida).
        let _ = t.tick(now);
        self.timer = t;
        self.apply_timer_prefs();
    }

    fn apply_timer_prefs(&mut self) {
        self.timer.pre_alert_ms = if self.prefs.pre_alert { self.prefs.pre_alert_seconds * 1000 } else { 0 };
    }

    fn start_rest(&mut self, secs: i64, context: &str) {
        self.timer.phase = TimerPhase::Rest;
        self.timer.context = context.to_string();
        self.timer.set_duration(secs * 1000);
        self.apply_timer_prefs();
        self.timer.restart(Clock::now());
        self.after_timer_start();
    }

    fn after_timer_start(&mut self) {
        self.save_timer();
        self.refresh_timer();
        self.maybe_ask_notification_permission();
    }

    /// 10.6: explica e pede a permissão no contexto em que ela é necessária, uma vez.
    fn maybe_ask_notification_permission(&mut self) {
        if self.platform.is_android()
            && self.prefs.background_alert
            && !self.prefs.notif_permission_asked
            && !self.platform.notifications_allowed()
        {
            self.prefs.notif_permission_asked = true;
            let _ = self.prefs.save(&self.db);
            self.show_info(
                InfoCtx::NotifPermission { then_enable: true },
                "Alerta com a tela bloqueada",
                "Para avisar o fim do descanso quando o app estiver em segundo plano ou com a tela bloqueada, o Workout precisa enviar notificações. Sem essa permissão, o cronômetro continua funcionando normalmente na tela (e a vibração, se ativada).",
                "Permitir",
                "Agora não",
            );
        }
    }

    fn timer_tick(&mut self) {
        let now = Clock::now();
        let events = self.timer.tick(now);
        let mut changed = false;
        for ev in events {
            changed = true;
            match ev {
                TimerEvent::PreAlert => {
                    if self.prefs.sound_enabled {
                        self.platform.play_tone(self.prefs.tone_pre_alert, 150);
                    }
                    if self.prefs.vibrate {
                        self.platform.vibrate(120);
                    }
                }
                TimerEvent::Finished(TimerPhase::Rest) => {
                    if self.prefs.sound_enabled {
                        self.platform.play_tone(self.prefs.tone_rest_end, 700);
                    }
                    if self.prefs.vibrate {
                        self.platform.vibrate(500);
                    }
                    if self.prefs.rest_end_action == RestEndAction::PrepareCountdown {
                        let ctx = self.timer.context.clone();
                        self.timer.phase = TimerPhase::Prepare;
                        self.timer.context = ctx;
                        self.timer.set_duration(self.prefs.prepare_seconds * 1000);
                        self.timer.pre_alert_ms = 0;
                        self.timer.restart(now);
                    } else if self.page() != P_TIMER {
                        self.toast("Descanso encerrado.");
                    }
                }
                TimerEvent::Finished(TimerPhase::Prepare) => {
                    if self.prefs.sound_enabled {
                        self.platform.play_tone(self.prefs.tone_set_start, 500);
                    }
                    if self.prefs.vibrate {
                        self.platform.vibrate(300);
                    }
                    if self.page() != P_TIMER {
                        self.toast("Hora de iniciar a série.");
                    }
                    // Volta à fase de descanso para o próximo uso, sem iniciar nada.
                    self.timer.phase = TimerPhase::Rest;
                    self.timer.set_duration(self.prefs.timer_default_seconds * 1000);
                    self.timer.status = TimerStatus::Finished;
                    self.apply_timer_prefs();
                }
            }
        }
        if changed {
            self.save_timer();
        }
        self.refresh_timer();
    }

    fn refresh_timer(&mut self) {
        let now = Clock::now();
        let t = &self.timer;
        let status = match t.status {
            TimerStatus::Idle => "idle",
            TimerStatus::Running => "running",
            TimerStatus::Paused => "paused",
            TimerStatus::Finished => "finished",
        };
        let phase = match t.phase {
            TimerPhase::Rest => "Descanso",
            TimerPhase::Prepare => "Preparação — próxima série",
        };
        let notice = if self.platform.is_android() && self.prefs.background_alert && !self.platform.notifications_allowed() {
            "Notificações desativadas: com o app em segundo plano o alerta pode não aparecer. O cronômetro continua correto ao voltar."
        } else if self.platform.is_android() && !self.prefs.background_alert {
            "Alerta em segundo plano desativado nas preferências."
        } else {
            ""
        };
        let display = t.display(now);
        let progress = t.progress(now);
        let ctx = t.context.clone();
        let duration = format_duration_s(t.duration_ms / 1000);
        let step = format!("{} s", self.prefs.timer_adjust_step);
        self.with_bridge(|b| {
            b.set_timer_display(ss(&display));
            b.set_timer_status(ss(status));
            b.set_timer_progress(progress);
            b.set_timer_context(ss(&ctx));
            b.set_timer_phase(ss(phase));
            b.set_timer_duration(ss(&duration));
            b.set_timer_step(ss(&step));
            b.set_timer_notice(ss(notice));
        });
    }

    fn on_pause(&mut self) {
        self.in_background = true;
        self.save_timer();
        if self.timer.is_running() && self.prefs.background_alert {
            let now = Clock::now();
            let rem = self.timer.remaining(now);
            let end_boot = now.boot_ms + rem;
            let title = match self.timer.phase {
                TimerPhase::Rest => "Descanso encerrado",
                TimerPhase::Prepare => "Hora de iniciar a série",
            };
            let text = if self.timer.context.is_empty() { "Toque para abrir o Workout".to_string() } else { self.timer.context.clone() };
            self.platform.schedule_alarm(ALARM_END, end_boot, title, &text, self.prefs.sound_enabled, self.prefs.vibrate);
            if self.timer.pre_alert_ms > 0 && rem > self.timer.pre_alert_ms + 1000 {
                self.platform.schedule_alarm(
                    ALARM_PRE,
                    end_boot - self.timer.pre_alert_ms,
                    &format!("Faltam {} s de descanso", self.timer.pre_alert_ms / 1000),
                    &text,
                    self.prefs.sound_enabled,
                    self.prefs.vibrate,
                );
            }
            self.platform.show_countdown(now.wall_ms + rem, "Descanso em andamento", &text);
        }
    }

    fn on_resume(&mut self) {
        let was_bg = self.in_background;
        self.in_background = false;
        self.platform.cancel_alarm(ALARM_END);
        self.platform.cancel_alarm(ALARM_PRE);
        self.platform.cancel_countdown();
        if was_bg {
            // Recalcula pelo instante de término; alertas já emitidos em segundo plano não se repetem.
            let now = Clock::now();
            let finished_bg = self.timer.is_running() && self.timer.remaining(now) <= 0;
            if finished_bg {
                let _ = self.timer.tick(now);
                if self.timer.phase == TimerPhase::Prepare {
                    self.timer.phase = TimerPhase::Rest;
                    self.timer.set_duration(self.prefs.timer_default_seconds * 1000);
                    self.timer.status = TimerStatus::Finished;
                }
                self.save_timer();
                self.toast("O descanso terminou enquanto o app estava em segundo plano.");
            }
            self.refresh_timer();
            let p = self.page();
            self.refresh_page(p);
        }
    }

    // ------------------------------------------------------------------
    // Calendário
    // ------------------------------------------------------------------

    fn refresh_calendar(&mut self) -> AppResult<()> {
        let today = cal::today();
        let monday = self.prefs.week_starts_monday;
        let week_view = self.with_bridge(|b| b.get_cal_week_view());
        let dates: Vec<NaiveDate> = if week_view {
            cal::week_dates(self.cal_selected, monday)
        } else {
            cal::month_grid(self.cal_month.year(), self.cal_month.month(), monday)
        };
        let plans = self.db.list_plans_full()?;
        let next = self.next_seq_map(&plans);
        let (a, _) = cal::day_bounds_ms(dates[0]);
        let (_, b) = cal::day_bounds_ms(*dates.last().unwrap());
        let sessions = self.db.list_session_summaries(Some(a), Some(b))?;
        let mut by_date: HashMap<NaiveDate, Vec<crate::history::SessionSummary>> = HashMap::new();
        for s in sessions {
            by_date.entry(local_date_of(s.started_at)).or_default().push(s);
        }
        let done_today_plans: Vec<Id> = by_date
            .get(&today)
            .map(|v| v.iter().filter_map(|s| self.db.get_session(s.id).ok().and_then(|x| x.plan_id)).collect())
            .unwrap_or_default();
        self.cal_cache.clear();
        let mut cells = Vec::new();
        for d in &dates {
            let planned = cal::planned_on(*d, today, &plans, &next, &done_today_plans);
            let done = by_date.remove(d).unwrap_or_default();
            cells.push(CalCell {
                key: ss(date_key(*d)),
                day: d.day() as i32,
                in_month: week_view || d.month() == self.cal_month.month(),
                today: *d == today,
                selected: *d == self.cal_selected,
                planned: !planned.is_empty(),
                done: !done.is_empty(),
            });
            self.cal_cache.push((*d, planned, done));
        }
        let title = if week_view {
            format!("{} – {}", fmt_dm(dates[0]), fmt_dm(dates[6]))
        } else {
            cal::month_title(self.cal_month.year(), self.cal_month.month())
        };
        let labels: Vec<SharedString> = dates.iter().take(7).map(|d| ss(WEEKDAY_SHORT[cal::weekday_index(*d) as usize])).collect();
        self.with_bridge(|b| {
            b.set_cal_title(ss(title));
            b.set_cal_weekday_labels(model(labels));
            b.set_cal_cells(model(cells));
        });
        self.refresh_cal_entries();
        Ok(())
    }

    fn refresh_cal_entries(&mut self) {
        let today = cal::today();
        let sel = self.cal_selected;
        let entries: Vec<CalEntry> = match self.cal_cache.iter().find(|(d, _, _)| *d == sel) {
            Some((_, planned, done)) => {
                let mut v: Vec<CalEntry> = done
                    .iter()
                    .map(|s| CalEntry {
                        kind: ss("done"),
                        title: ss(format!("{} — {}", s.plan_name, s.schedule_name)),
                        subtitle: ss(format!(
                            "{} · {} exercício(s) · {} série(s)",
                            local_time_of(s.started_at),
                            s.exercises,
                            s.sets
                        )),
                        session_id: s.id as i32,
                        day_id: 0,
                        can_start: false,
                    })
                    .collect();
                v.extend(planned.iter().map(|p| CalEntry {
                    kind: ss("planned"),
                    title: ss(format!("{} — {}", p.plan_name, p.day_name)),
                    subtitle: ss(format!(
                        "{} exercício(s){}",
                        p.exercises,
                        if p.from_sequence { " · próximo da sequência" } else { "" }
                    )),
                    session_id: 0,
                    day_id: p.day_id as i32,
                    can_start: sel == today,
                }));
                v
            }
            None => vec![],
        };
        let title = capitalize(&cal::date_long(sel));
        self.with_bridge(|b| {
            b.set_cal_selected_title(ss(title));
            b.set_cal_entries(model(entries));
        });
    }

    fn cal_shift(&mut self, dir: i32) {
        if self.with_bridge(|b| b.get_cal_week_view()) {
            self.cal_selected += Duration::days(7 * dir as i64);
            self.cal_month = cal::add_months(self.cal_selected, 0);
        } else {
            self.cal_month = cal::add_months(self.cal_month, dir);
            self.cal_selected = self.cal_month;
        }
        let r = self.refresh_calendar();
        self.report(r);
    }

    fn cal_entry(&mut self, i: usize) {
        let entries = self.with_bridge(|b| b.get_cal_entries());
        let Some(e) = entries.row_data(i) else { return };
        if e.kind == "done" {
            self.open_detail(e.session_id as Id);
        } else if e.can_start {
            self.start_day(e.day_id as Id);
        } else if let Ok(d) = self.db.get_day(e.day_id as Id) {
            self.open_plan(d.plan_id);
        }
    }

    // ------------------------------------------------------------------
    // Histórico
    // ------------------------------------------------------------------

    fn refresh_history(&mut self) -> AppResult<()> {
        let unit = self.prefs.unit;
        let list = self.db.list_session_summaries(None, None)?;
        let rows: Vec<HistRow> = list
            .iter()
            .map(|s| HistRow {
                id: s.id as i32,
                date: ss(capitalize(&fmt_date_time(s.started_at))),
                title: ss(format!("{} — {}", s.plan_name, s.schedule_name)),
                subtitle: ss(format!(
                    "{} exercício(s) · {} série(s) · volume {}{}",
                    s.exercises,
                    s.sets,
                    fmt_volume(s.volume_g, unit),
                    s.ended_at.map(|e| format!(" · {}", fmt_minutes(e - s.started_at))).unwrap_or_default()
                )),
            })
            .collect();
        let evo: Vec<HistExRow> = self
            .db
            .history_exercises()?
            .into_iter()
            .map(|h| HistExRow {
                key: ss(&h.key),
                name: ss(&h.name),
                detail: ss(format!("{} sessão(ões) · última em {}", h.sessions, fmt_date_short(h.last_at))),
            })
            .collect();
        let summary = if list.is_empty() { String::new() } else { format!("{} sessão(ões) concluída(s)", list.len()) };
        self.with_bridge(|b| {
            b.set_hist_rows(model(rows));
            b.set_evo_exercises(model(evo));
            b.set_hist_summary(ss(summary));
        });
        Ok(())
    }

    fn open_detail(&mut self, id: Id) {
        self.detail_id = Some(id);
        self.navigate(P_DETAIL);
    }

    fn refresh_detail(&mut self) -> AppResult<()> {
        let Some(id) = self.detail_id else { return Ok(()) };
        let f = self.db.get_session_full(id)?;
        let unit = self.prefs.unit;
        // intervalos entre séries (inclui execução) a partir dos horários de conclusão
        let mut times: Vec<(Id, i64)> = f
            .exercises
            .iter()
            .flat_map(|e| e.sets.iter().filter_map(|s| s.completed_at.map(|t| (s.id, t))))
            .collect();
        times.sort_by_key(|x| x.1);
        let mut interval: HashMap<Id, i64> = HashMap::new();
        for w in times.windows(2) {
            interval.insert(w[1].0, (w[1].1 - w[0].1) / 1000);
        }
        let exercises: Vec<DetailEx> = f
            .exercises
            .iter()
            .map(|e| {
                let badge = match e.substitution {
                    SubstitutionType::None => String::new(),
                    SubstitutionType::Temporary => format!("Substituiu {} (só nesta sessão)", e.original_name),
                    SubstitutionType::Permanent => format!("Substituiu {} (permanente)", e.original_name),
                };
                let mut lines: Vec<SharedString> = e
                    .sets
                    .iter()
                    .map(|s| {
                        let actual = match (s.actual_load_g, s.actual_reps) {
                            (Some(l), Some(r)) => format!("{} × {}", format_load_unit(l, unit), r),
                            _ => "não registrada".to_string(),
                        };
                        let rest = interval
                            .get(&s.id)
                            .map(|secs| format!(" · intervalo {}", format_duration_s(*secs)))
                            .unwrap_or_default();
                        ss(format!(
                            "Série {}: {}  (plan. {}{}){}",
                            s.set_number,
                            actual,
                            if s.planned_load_g > 0 { format!("{} × ", format_load_unit(s.planned_load_g, unit)) } else { String::new() },
                            format_reps(s.planned_reps_min, s.planned_reps_max),
                            rest
                        ))
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push(ss("Não realizado nesta sessão."));
                }
                DetailEx {
                    title: ss(&e.performed_name),
                    subtitle: ss(format!(
                        "{}{}{}",
                        e.muscle_focus,
                        if e.exercise_type.is_empty() { String::new() } else { format!(" · {}", e.exercise_type) },
                        if e.rest_seconds > 0 { format!(" · descanso planejado {}", format_duration_s(e.rest_seconds)) } else { String::new() }
                    )),
                    badge: ss(badge),
                    lines: model(lines),
                    notes: ss(&e.notes),
                }
            })
            .collect();
        let when = match f.session.ended_at {
            Some(end) => format!(
                "{} · {}–{} ({})",
                capitalize(&cal::date_long(local_date_of(f.session.started_at))),
                local_time_of(f.session.started_at),
                local_time_of(end),
                fmt_minutes(end - f.session.started_at)
            ),
            None => format!("{} · em andamento", capitalize(&cal::date_long(local_date_of(f.session.started_at)))),
        };
        let summary = format!(
            "{} série(s) · volume {}",
            f.completed_sets(),
            fmt_volume(f.volume_g(), unit)
        );
        self.with_bridge(|b| {
            b.set_detail_title(ss(format!("{} — {}", f.session.plan_name, f.session.schedule_name)));
            b.set_detail_subtitle(ss(when));
            b.set_detail_summary(ss(summary));
            b.set_detail_notes(ss(&f.session.notes));
            b.set_detail_exercises(model(exercises));
        });
        Ok(())
    }

    fn refresh_evolution(&mut self) -> AppResult<()> {
        let unit = self.prefs.unit;
        let from = match self.evo_period {
            0 => Some(now_ms() - 30 * 86_400_000),
            1 => Some(now_ms() - 90 * 86_400_000),
            2 => Some(now_ms() - 365 * 86_400_000),
            _ => None,
        };
        let metric = match self.evo_metric {
            1 => EvolutionMetric::Volume,
            2 => EvolutionMetric::Estimated1Rm,
            3 => EvolutionMetric::BestReps,
            _ => EvolutionMetric::MaxLoad,
        };
        let pts = self.db.exercise_evolution(&self.evo_key, from)?;
        let name = self
            .db
            .history_exercises()?
            .into_iter()
            .find(|h| h.key == self.evo_key)
            .map(|h| h.name)
            .unwrap_or_default();
        let fmt_v = |v: i64| -> String {
            match metric {
                EvolutionMetric::BestReps => format!("{v} reps"),
                EvolutionMetric::Volume => fmt_volume(v, unit),
                _ => format_load_unit(v, unit),
            }
        };
        let values: Vec<i64> = pts.iter().map(|p| p.value(metric)).collect();
        let (path, dots) = chart_paths(&values);
        let (mn, mx) = (values.iter().copied().min().unwrap_or(0), values.iter().copied().max().unwrap_or(0));
        let summary = if values.len() >= 2 {
            let first = values[0];
            let last = *values.last().unwrap();
            let diff = last - first;
            let pct = if first > 0 { format!(" ({:+.0}%)", diff as f64 * 100.0 / first as f64) } else { String::new() };
            format!(
                "{} sessão(ões) no período. Variação: {}{}{}",
                values.len(),
                if diff >= 0 { "+" } else { "−" },
                fmt_v(diff.abs()),
                pct
            )
        } else if values.len() == 1 {
            "1 sessão no período.".to_string()
        } else {
            String::new()
        };
        let rows: Vec<EvoRow> = pts
            .iter()
            .rev()
            .map(|p| EvoRow {
                date: ss(fmt_date_short(p.started_at)),
                value: ss(fmt_v(p.value(metric))),
                detail: ss(format!("{} série(s) · melhor {} reps", p.sets, p.best_reps)),
            })
            .collect();
        self.with_bridge(|b| {
            b.set_evo_name(ss(&name));
            b.set_evo_period(self.evo_period);
            b.set_evo_metric(self.evo_metric);
            b.set_evo_has_data(!values.is_empty());
            b.set_evo_path(ss(path));
            b.set_evo_dots(ss(dots));
            b.set_evo_min(ss(format!("mín. {}", fmt_v(mn))));
            b.set_evo_max(ss(format!("máx. {}", fmt_v(mx))));
            b.set_evo_first_date(ss(pts.first().map(|p| fmt_date_short(p.started_at)).unwrap_or_default()));
            b.set_evo_last_date(ss(pts.last().map(|p| fmt_date_short(p.started_at)).unwrap_or_default()));
            b.set_evo_summary(ss(summary));
            b.set_evo_rows(model(rows));
        });
        Ok(())
    }

    // ------------------------------------------------------------------
    // Configurações
    // ------------------------------------------------------------------

    fn refresh_settings(&mut self) -> AppResult<()> {
        let p = self.prefs.clone();
        let counts = self.db.counts()?;
        let db_size = self
            .db
            .path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        let notif = if !self.platform.is_android() {
            "Em segundo plano, o alerta usa uma notificação do sistema.".to_string()
        } else if self.platform.notifications_allowed() {
            "Notificações permitidas. O Android pode atrasar o alerta alguns segundos em modo de economia de energia.".to_string()
        } else {
            "Notificações não permitidas: em segundo plano, apenas a vibração (se ativada) pode ocorrer. O cronômetro continua correto na tela.".to_string()
        };
        let data = format!(
            "{} ficha(s) · {} exercício(s) na biblioteca · {} sessão(ões) · {} série(s) registradas\nBanco de dados: {:.1} MB, no armazenamento privado do app (sem internet, sem nuvem).",
            counts.plans,
            counts.exercises,
            counts.sessions,
            counts.sets,
            db_size as f64 / 1_048_576.0
        );
        let version = self.platform.app_version();
        let schema = schema_version(&self.db.conn).unwrap_or(0);
        let about = format!(
            "Versão do app: {version}\nVersão do banco de dados (schema): {schema}\n\n\
             Aplicativo pessoal e offline: não usa internet, não tem conta, anúncios nem sincronização. \
             Os dados ficam somente neste aparelho — faça backups periódicos.\n\n\
             O Workout é um organizador de treinos. Não é ferramenta de diagnóstico, prescrição ou recomendação médica \
             e não substitui orientação de profissional de educação física ou saúde.\n\n\
             Alternativas de exercícios são sugestões organizacionais e não indicam equivalência biomecânica.\n\n\
             A precisão dos alertas em segundo plano depende das políticas e permissões do Android.\n\n\
             Permissões usadas: notificações (alerta do cronômetro, opcional) e vibração.\n\n\
             Feito com Rust, Slint e SQLite. Licenças de terceiros: veja o repositório do projeto."
        );
        let step = format_load_unit(p.load_step_g, p.unit);
        self.with_bridge(|b| {
            b.set_set_sound(p.sound_enabled);
            b.set_set_tone_rest(ss(TONE_NAMES[p.tone_rest_end as usize]));
            b.set_set_tone_start(ss(TONE_NAMES[p.tone_set_start as usize]));
            b.set_set_tone_pre(ss(TONE_NAMES[p.tone_pre_alert as usize]));
            b.set_set_vibrate(p.vibrate);
            b.set_set_pre_alert(p.pre_alert);
            b.set_set_pre_alert_s(ss(format!("{} s", p.pre_alert_seconds)));
            b.set_set_background(p.background_alert);
            b.set_set_timer_default(ss(format_duration_s(p.timer_default_seconds)));
            b.set_set_adjust_step(ss(format!("{} s", p.timer_adjust_step)));
            b.set_set_auto_rest(p.auto_start_rest);
            b.set_set_rest_end_prepare(p.rest_end_action == RestEndAction::PrepareCountdown);
            b.set_set_prepare_s(ss(format!("{} s", p.prepare_seconds)));
            b.set_set_unit(ss(if p.unit == LoadUnit::Kg { "Quilogramas (kg)" } else { "Libras (lb)" }));
            b.set_set_load_step(ss(step));
            b.set_set_prefill_last(p.prefill_last);
            b.set_set_default_sets(ss(p.default_sets.to_string()));
            b.set_set_default_reps(ss(p.default_reps.to_string()));
            b.set_set_week_monday(p.week_starts_monday);
            b.set_set_week_view(p.calendar_week_view);
            b.set_set_large_text(p.large_text);
            b.set_set_keep_screen(p.keep_screen_on);
            b.set_set_confirm_finish(p.confirm_finish);
            b.set_notif_status(ss(notif));
            b.set_data_info(ss(data));
            b.set_app_version(ss(&version));
            b.set_about_text(ss(about));
        });
        Ok(())
    }

    fn save_prefs(&mut self) {
        let r = self.prefs.save(&self.db);
        self.report(r);
        self.apply_timer_prefs();
        self.ui().global::<Theme>().set_large(self.prefs.large_text);
        let r = self.refresh_settings();
        self.report(r);
    }

    fn pref_toggle(&mut self, key: &str) {
        let p = &mut self.prefs;
        match key {
            "sound" => p.sound_enabled = !p.sound_enabled,
            "vibrate" => p.vibrate = !p.vibrate,
            "pre-alert" => p.pre_alert = !p.pre_alert,
            "background" => {
                if !p.background_alert && self.platform.is_android() && !self.platform.notifications_allowed() {
                    self.show_info(
                        InfoCtx::NotifPermission { then_enable: true },
                        "Permitir notificações",
                        "O alerta com o app em segundo plano é exibido como notificação. Ao continuar, o Android vai pedir sua autorização. Você pode negar: o restante do app continua funcionando.",
                        "Continuar",
                        "Cancelar",
                    );
                    return;
                }
                p.background_alert = !p.background_alert;
            }
            "auto-rest" => p.auto_start_rest = !p.auto_start_rest,
            "rest-end-prepare" => {
                p.rest_end_action = if p.rest_end_action == RestEndAction::None {
                    RestEndAction::PrepareCountdown
                } else {
                    RestEndAction::None
                }
            }
            "prefill-last" => p.prefill_last = !p.prefill_last,
            "week-monday" => p.week_starts_monday = !p.week_starts_monday,
            "week-view" => p.calendar_week_view = !p.calendar_week_view,
            "large-text" => p.large_text = !p.large_text,
            "keep-screen" => p.keep_screen_on = !p.keep_screen_on,
            "confirm-finish" => p.confirm_finish = !p.confirm_finish,
            _ => return,
        }
        self.save_prefs();
    }

    fn pref_step(&mut self, key: &str, d: i32) {
        let p = &mut self.prefs;
        let d = d as i64;
        match key {
            "pre-alert-s" => p.pre_alert_seconds = (p.pre_alert_seconds + d).clamp(3, 60),
            "timer-default" => p.timer_default_seconds = (p.timer_default_seconds + d * 15).clamp(15, 3600),
            "adjust-step" => p.timer_adjust_step = (p.timer_adjust_step + d * 5).clamp(5, 60),
            "prepare-s" => p.prepare_seconds = (p.prepare_seconds + d).clamp(3, 30),
            "default-sets" => p.default_sets = (p.default_sets + d).clamp(1, 20),
            "default-reps" => p.default_reps = (p.default_reps + d).clamp(1, 100),
            _ => return,
        }
        self.save_prefs();
    }

    fn pref_choose(&mut self, key: &str) {
        let tones: Vec<String> = TONE_NAMES.iter().map(|s| s.to_string()).collect();
        match key {
            "tone-rest" => self.ask_choice(ChoiceCtx::Tone("rest"), "Som do fim do descanso", "", tones, self.prefs.tone_rest_end),
            "tone-start" => self.ask_choice(ChoiceCtx::Tone("start"), "Som do início de série", "", tones, self.prefs.tone_set_start),
            "tone-pre" => self.ask_choice(ChoiceCtx::Tone("pre"), "Som do pré-alerta", "", tones, self.prefs.tone_pre_alert),
            "unit" => self.ask_choice(
                ChoiceCtx::Unit,
                "Unidade de carga",
                "Os valores são guardados com precisão em gramas; trocar a unidade só muda a exibição.",
                vec!["Quilogramas (kg)".into(), "Libras (lb)".into()],
                if self.prefs.unit == LoadUnit::Kg { 0 } else { 1 },
            ),
            "load-step" => {
                let opts: Vec<String> = LOAD_STEPS.iter().map(|g| format_load_unit(*g, self.prefs.unit)).collect();
                let sel = LOAD_STEPS.iter().position(|g| *g == self.prefs.load_step_g).map(|i| i as i32).unwrap_or(-1);
                self.ask_choice(ChoiceCtx::LoadStep, "Incremento de carga", "", opts, sel);
            }
            _ => {}
        }
    }

    fn choice_made(&mut self, i: i32) {
        let Some(ctx) = self.choice.clone() else { return };
        self.close_dialog();
        let i = i.max(0);
        match ctx {
            ChoiceCtx::Tone(which) => {
                match which {
                    "rest" => self.prefs.tone_rest_end = i,
                    "start" => self.prefs.tone_set_start = i,
                    _ => self.prefs.tone_pre_alert = i,
                }
                self.platform.play_tone(i, 300);
                self.save_prefs();
            }
            ChoiceCtx::Unit => {
                self.prefs.unit = if i == 1 { LoadUnit::Lb } else { LoadUnit::Kg };
                self.save_prefs();
            }
            ChoiceCtx::LoadStep => {
                self.prefs.load_step_g = LOAD_STEPS[(i as usize).min(LOAD_STEPS.len() - 1)];
                self.save_prefs();
            }
            ChoiceCtx::LibFilter(field, values) => {
                let v = if i == 0 { String::new() } else { values.get(i as usize - 1).cloned().unwrap_or_default() };
                self.with_bridge(|b| match field {
                    "muscle" => b.set_lib_muscle(ss(&v)),
                    "equipment" => b.set_lib_equipment(ss(&v)),
                    _ => b.set_lib_pattern(ss(&v)),
                });
                let r = self.refresh_library();
                self.report(r);
            }
            ChoiceCtx::ImportSheet => {
                if let Some(st) = self.import.as_mut() {
                    st.sheet = (i as usize).min(st.workbook.sheets.len() - 1);
                    st.mapping = detect_mapping(&st.workbook.sheets[st.sheet]);
                }
                self.rebuild_import_preview();
            }
            ChoiceCtx::ImportMap(col) => {
                if let Some(st) = self.import.as_mut() {
                    let f = if i == 0 { None } else { Field::from_index(i as i64 - 1) };
                    st.mapping.set(col, f);
                }
                self.rebuild_import_preview();
            }
            ChoiceCtx::ExportKind => {
                let kind = match i {
                    0 => ExportKind::Plans,
                    1 => ExportKind::History,
                    _ => ExportKind::All,
                };
                self.do_export(kind);
            }
        }
    }

    // ------------------------------------------------------------------
    // Arquivos: importação, exportação, backup
    // ------------------------------------------------------------------

    fn tmp_file(&self, name: &str) -> std::path::PathBuf {
        let dir = self.platform.tmp_dir();
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    fn today_tag() -> String {
        cal::today().format("%Y-%m-%d").to_string()
    }

    fn save_via_picker(&mut self, path: std::path::PathBuf, name: &str, mime: &str, what: &str) {
        self.file_op = Some(FileOp::Save(what.to_string()));
        self.platform.save_document(name, mime, &path);
    }

    fn do_export(&mut self, kind: ExportKind) {
        let name = match kind {
            ExportKind::Plans => format!("workout-fichas-{}.xlsx", Self::today_tag()),
            ExportKind::History => format!("workout-historico-{}.xlsx", Self::today_tag()),
            ExportKind::All => format!("workout-dados-{}.xlsx", Self::today_tag()),
        };
        let path = self.tmp_file(&format!("export-{}.xlsx", now_ms()));
        match export_xlsx(&self.db, kind, &path) {
            Ok(_) => self.save_via_picker(path, &name, XLSX_MIME, "Planilha exportada"),
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn data_template(&mut self) {
        let path = self.tmp_file(&format!("modelo-{}.xlsx", now_ms()));
        match export_template(&path) {
            Ok(()) => self.save_via_picker(path, "modelo-importacao-workout.xlsx", XLSX_MIME, "Modelo salvo"),
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn data_backup(&mut self) {
        let path = self.tmp_file(&format!("backup-{}.zip", now_ms()));
        let work = self.platform.tmp_dir();
        let version = self.platform.app_version();
        match create_backup(&self.db, &path, &work, &version) {
            Ok(_) => {
                let name = format!("workout-backup-{}.zip", Self::today_tag());
                self.save_via_picker(path, &name, "application/zip", "Backup criado");
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn data_ics(&mut self) {
        match crate::import_export::ics::build_ics(&self.db, 8) {
            Ok(text) => {
                let path = self.tmp_file(&format!("cal-{}.ics", now_ms()));
                if let Err(e) = std::fs::write(&path, text) {
                    self.toast(e.to_string());
                    return;
                }
                self.save_via_picker(path, "workout-calendario.ics", "text/calendar", "Calendário exportado");
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn poll_files(&mut self) {
        if self.file_op.is_none() {
            return;
        }
        let Some(res) = self.platform.poll_file_result() else { return };
        let op = self.file_op.take().unwrap();
        match (op, res) {
            (_, FileResult::Cancelled) => {}
            (_, FileResult::Error(e)) => self.toast(e),
            (FileOp::Save(what), FileResult::Saved { name }) => self.toast(format!("{what}: {name}")),
            (FileOp::ImportXlsx, FileResult::Opened { path, name }) => {
                let r = read_workbook(&path);
                let _ = std::fs::remove_file(&path);
                match r {
                    Ok(wb) => {
                        let mapping = detect_mapping(&wb.sheets[0]);
                        self.import = Some(ImportState {
                            file_name: name,
                            workbook: wb,
                            sheet: 0,
                            mapping,
                            preview: ImportPreview::default(),
                            replace: false,
                        });
                        self.with_bridge(|b| b.set_imp_reviewed(false));
                        self.rebuild_import_preview();
                        self.navigate(P_IMPORT);
                    }
                    Err(e) => self.show_info(InfoCtx::Plain, "Não foi possível importar", &e.to_string(), "OK", ""),
                }
            }
            (FileOp::Restore, FileResult::Opened { path, name: _ }) => {
                let work = self.platform.tmp_dir();
                let r = validate_backup(&path, &work);
                let _ = std::fs::remove_file(&path);
                match r {
                    Ok(v) => {
                        let m = &v.manifest;
                        let text = format!(
                            "Backup de {} (app {}): {} ficha(s), {} sessão(ões), {} série(s).\n\nOs dados atuais serão substituídos por este backup. Uma cópia do estado atual é guardada para desfazer em caso de falha. Deseja continuar?",
                            fmt_date_time(m.created_at),
                            m.app_version,
                            m.counts.plans,
                            m.counts.sessions,
                            m.counts.sets
                        );
                        self.pending_restore = Some(v);
                        self.ask_confirm(ConfirmAction::Restore, "Restaurar backup", &text, "Restaurar");
                    }
                    Err(e) => self.show_info(InfoCtx::Plain, "Backup inválido", &e.to_string(), "OK", ""),
                }
            }
            (op, res) => log::warn!("resultado de arquivo inesperado: {op:?} {res:?}"),
        }
    }

    fn rebuild_import_preview(&mut self) {
        let defaults = ImportDefaults {
            sets: self.prefs.default_sets,
            reps: (self.prefs.default_reps, self.prefs.default_reps),
            rest_seconds: self.prefs.timer_default_seconds,
            unit: self.prefs.unit,
        };
        if let Some(st) = self.import.as_mut() {
            let sheet = &st.workbook.sheets[st.sheet];
            st.preview = build_preview(&self.db, sheet, &st.mapping, defaults);
        }
        self.with_bridge(|b| b.set_imp_reviewed(false));
        self.refresh_import();
    }

    fn refresh_import(&mut self) {
        let Some(st) = self.import.as_ref() else { return };
        let sheet = &st.workbook.sheets[st.sheet];
        let header = sheet.rows.get(st.mapping.header_row).cloned().unwrap_or_default();
        let sample_row = sheet.rows.get(st.mapping.header_row + 1).cloned().unwrap_or_default();
        let cols: Vec<MapCol> = (0..st.mapping.columns.len())
            .filter(|c| header.get(*c).map(|h| !h.trim().is_empty()).unwrap_or(false) || st.mapping.columns[*c].is_some())
            .map(|c| MapCol {
                index: c as i32,
                header: ss(header.get(c).filter(|h| !h.trim().is_empty()).cloned().unwrap_or_else(|| format!("Coluna {}", c + 1))),
                sample: ss(sample_row.get(c).map(|s| format!("Ex.: {s}")).unwrap_or_default()),
                field: ss(st.mapping.columns[c].map(|f| f.label()).unwrap_or("Ignorar")),
            })
            .collect();
        let unit = self.prefs.unit;
        let plans: Vec<PlanPreview> = st
            .preview
            .plans
            .iter()
            .map(|p| PlanPreview {
                name: ss(&p.name),
                detail: ss(format!(
                    "{} · {}",
                    p.schedule.label(),
                    p.days
                        .iter()
                        .map(|d| format!(
                            "{}{}: {}",
                            d.name,
                            if d.weekdays != 0 { format!(" ({})", weekdays_text(d.weekdays)) } else { String::new() },
                            d.exercises
                                .iter()
                                .map(|e| format!(
                                    "{} {}×{}{}",
                                    e.name,
                                    e.target.sets,
                                    format_reps(e.target.reps_min, e.target.reps_max),
                                    if e.target.load_g > 0 { format!(" {}", format_load_unit(e.target.load_g, unit)) } else { String::new() }
                                ))
                                .collect::<Vec<_>>()
                                .join("; ")
                        ))
                        .collect::<Vec<_>>()
                        .join("\n")
                )),
                conflict: p.exists,
            })
            .collect();
        let summary = format!(
            "{} linha(s) com dados, {} válida(s) → {} ficha(s), {} exercício(s). Nada é gravado até você confirmar.",
            st.preview.data_rows,
            st.preview.valid_rows,
            st.preview.plans.len(),
            st.preview.exercise_count()
        );
        let p = &st.preview;
        let (file, sheet_name, multi, can, conflicts, replace) = (
            st.file_name.clone(),
            sheet.name.clone(),
            st.workbook.sheets.len() > 1,
            p.can_import(),
            p.has_conflicts(),
            st.replace,
        );
        let warnings: Vec<SharedString> = p.warnings.iter().map(ss).collect();
        let errors: Vec<SharedString> = p.errors.iter().map(ss).collect();
        self.with_bridge(|b| {
            b.set_imp_file(ss(file));
            b.set_imp_sheet(ss(sheet_name));
            b.set_imp_multi_sheet(multi);
            b.set_imp_columns(model(cols));
            b.set_imp_plans(model(plans));
            b.set_imp_warnings(model(warnings));
            b.set_imp_errors(model(errors));
            b.set_imp_summary(ss(summary));
            b.set_imp_can_import(can);
            b.set_imp_has_conflicts(conflicts);
            b.set_imp_replace(replace);
        });
    }

    fn imp_confirm(&mut self) {
        let Some(st) = self.import.as_ref() else { return };
        if !st.preview.warnings.is_empty() && !self.with_bridge(|b| b.get_imp_reviewed()) {
            self.toast("Confira os avisos antes de importar.");
            return;
        }
        let policy = if st.replace { ConflictPolicy::Replace } else { ConflictPolicy::CreateNew };
        let preview = st.preview.clone();
        match apply_import(&mut self.db, &preview, policy) {
            Ok(r) => {
                self.import = None;
                self.navigate(P_PLANS);
                self.show_info(
                    InfoCtx::Plain,
                    "Importação concluída",
                    &format!(
                        "{} ficha(s), {} dia(s) e {} exercício(s) importados.{}{}",
                        r.plans,
                        r.days,
                        r.exercises,
                        if r.new_catalog_exercises > 0 {
                            format!("\n{} exercício(s) novo(s) adicionados à biblioteca.", r.new_catalog_exercises)
                        } else {
                            String::new()
                        },
                        if r.replaced > 0 { format!("\n{} ficha(s) substituída(s).", r.replaced) } else { String::new() }
                    ),
                    "OK",
                    "",
                );
            }
            Err(e) => self.show_info(InfoCtx::Plain, "Importação não realizada", &format!("{e}\n\nNenhum dado foi gravado."), "OK", ""),
        }
    }

    fn do_restore(&mut self) {
        let Some(v) = self.pending_restore.take() else { return };
        match self.db.restore_from(v) {
            Ok(()) => {
                self.prefs = Prefs::load(&self.db).unwrap_or_default();
                self.session_id = self.db.active_session_id().ok().flatten();
                self.load_timer();
                self.ui().global::<Theme>().set_large(self.prefs.large_text);
                self.navigate(P_HOME);
                self.show_info(InfoCtx::Plain, "Backup restaurado", "Fichas, sessões, exercícios e preferências foram restaurados.", "OK", "");
            }
            Err(e) => self.show_info(
                InfoCtx::Plain,
                "Falha na restauração",
                &format!("{e}\n\nOs dados anteriores foram mantidos."),
                "OK",
                "",
            ),
        }
    }

    // ------------------------------------------------------------------
    // Diálogos: ações
    // ------------------------------------------------------------------

    fn confirm_accepted(&mut self) {
        let Some(a) = self.confirm.clone() else { return };
        self.close_dialog();
        match a {
            ConfirmAction::DeletePlan(id) => {
                let r = self.db.delete_plan(id);
                if r.is_ok() {
                    self.toast("Ficha excluída. Sessões já realizadas continuam no histórico.");
                    if self.plan_id == Some(id) {
                        self.plan_id = None;
                    }
                    if self.page() == P_EDITOR {
                        self.stack.pop();
                        self.navigate(P_PLANS);
                    }
                }
                self.report(r);
                let r = self.refresh_plans();
                self.report(r);
            }
            ConfirmAction::DeleteDay(id) => {
                let r = self.db.delete_day(id);
                self.report(r);
                self.report_refresh_editor();
            }
            ConfirmAction::DeletePlanEx(id) => {
                let r = self.db.delete_plan_exercise(id);
                self.report(r);
                self.pe = None;
                self.report_refresh_editor();
            }
            ConfirmAction::FinishSession => self.finish_session_now(),
            ConfirmAction::DiscardSession => {
                if let Some(sid) = self.session_id {
                    match self.db.discard_session(sid) {
                        Ok(()) => {
                            self.session_id = None;
                            self.platform.keep_screen_on(false);
                            self.with_bridge(|b| b.set_session_active(false));
                            self.navigate(P_HOME);
                            self.toast("Sessão descartada. Nada foi gravado no histórico.");
                        }
                        Err(e) => self.toast(e.to_string()),
                    }
                }
            }
            ConfirmAction::Restore => self.do_restore(),
            ConfirmAction::ArchiveExercise(id, archive) => {
                let r = self.db.set_exercise_archived(id, archive);
                match r {
                    Ok(()) => self.toast(if archive { "Exercício arquivado." } else { "Exercício reativado." }),
                    Err(e) => self.toast(e.to_string()),
                }
                let r = self.refresh_library();
                self.report(r);
            }
        }
    }

    fn text_saved(&mut self, value: &str) {
        let Some(ctx) = self.text.clone() else { return };
        let r = match ctx {
            TextCtx::SessionNotes => self.session_id.map(|sid| self.db.set_session_notes(sid, value)).unwrap_or(Ok(())),
            TextCtx::ExerciseNotes(se) => self.db.set_session_exercise_notes(se, value),
            TextCtx::TimerCustom => {
                let secs = parse_rest(value).and_then(|s| if s < 1 { validation("Informe pelo menos 1 segundo.") } else { Ok(s) });
                match secs {
                    Ok(s) => {
                        self.timer.set_duration(s * 1000);
                        self.save_timer();
                        self.refresh_timer();
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        };
        match r {
            Ok(()) => {
                self.close_dialog();
                if matches!(ctx, TextCtx::SessionNotes | TextCtx::ExerciseNotes(_)) {
                    let r = self.refresh_session();
                    self.report(r);
                }
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn info_ok(&mut self) {
        let ctx = self.info;
        self.close_dialog();
        match ctx {
            Some(InfoCtx::NotifPermission { then_enable }) => {
                self.prefs.notif_permission_asked = true;
                if then_enable {
                    self.prefs.background_alert = true;
                }
                let _ = self.prefs.save(&self.db);
                self.awaiting_permission = true;
                self.platform.request_notification_permission();
            }
            Some(InfoCtx::ActiveSession) => self.open_session(),
            _ => {}
        }
    }

    fn info_alt(&mut self) {
        let ctx = self.info;
        self.close_dialog();
        if let Some(InfoCtx::NotifPermission { .. }) = ctx {
            self.prefs.notif_permission_asked = true;
            let _ = self.prefs.save(&self.db);
            self.refresh_timer();
        }
    }

    fn poll_permission(&mut self) {
        if !self.awaiting_permission {
            return;
        }
        if let Some(granted) = self.platform.poll_permission_result() {
            self.awaiting_permission = false;
            if granted {
                self.toast("Notificações permitidas. Você receberá o alerta com a tela bloqueada.");
            } else {
                self.toast("Permissão negada. O cronômetro continua funcionando na tela; em segundo plano o alerta pode não aparecer.");
            }
            let r = self.refresh_settings();
            self.report(r);
            self.refresh_timer();
        }
    }

    /// Chamado a cada 250 ms.
    fn tick(&mut self) {
        if self.timer.is_running() || self.timer.status == TimerStatus::Finished {
            self.timer_tick();
        }
        if self.page() == P_SESSION {
            self.update_elapsed();
        }
        self.poll_files();
        self.poll_permission();
        if self.toast_ticks > 0 {
            self.toast_ticks -= 1;
            if self.toast_ticks == 0 {
                self.with_bridge(|b| b.set_toast(ss("")));
            }
        }
    }
}

const LOAD_STEPS: [i64; 6] = [500, 1_000, 1_250, 2_000, 2_500, 5_000];

fn ex_row(e: &Exercise, note: &str) -> ExRow {
    let detail = [e.muscle_focus.as_str(), e.exercise_type.as_str(), e.equipment.as_str(), e.movement_pattern.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    ExRow { id: e.id as i32, name: ss(&e.name), detail: ss(detail), note: ss(note), archived: e.archived }
}

fn target_text(t: &Target, rest: i64, unit: LoadUnit) -> String {
    let mut s = format!("{} × {}", t.sets, format_reps(t.reps_min, t.reps_max));
    if t.load_g > 0 {
        s.push_str(&format!(" · {}", format_load_unit(t.load_g, unit)));
    }
    if rest > 0 {
        s.push_str(&format!(" · descanso {}", format_duration_s(rest)));
    }
    s
}

/// Constrói a aplicação: abre o banco, cria a janela, registra callbacks e roda o loop de eventos.
pub fn run(platform: Box<dyn Platform>) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let data_dir = platform.data_dir();
    let (db, open_error) = match Db::open(&data_dir.join("workout.db")) {
        Ok(db) => (db, None),
        Err(e) => {
            log::error!("falha ao abrir banco: {e}");
            (Db::open_in_memory().expect("banco em memória"), Some(e.to_string()))
        }
    };
    // Limpa temporários antigos (5.3).
    if let Ok(rd) = std::fs::read_dir(platform.tmp_dir()) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let prefs = Prefs::load(&db).unwrap_or_default();
    let today = cal::today();
    let ctrl = Rc::new(RefCell::new(Controller {
        ui: ui.as_weak(),
        db,
        platform,
        prefs,
        stack: vec![P_HOME],
        plan_id: None,
        pe: None,
        day_ctx: None,
        exf_id: None,
        lib_pick: false,
        confirm: None,
        choice: None,
        text: None,
        info: None,
        session_id: None,
        sub_se: None,
        timer: TimerState::default(),
        cal_month: cal::add_months(today, 0),
        cal_selected: today,
        cal_cache: Vec::new(),
        detail_id: None,
        evo_key: String::new(),
        evo_period: 3,
        evo_metric: 0,
        import: None,
        file_op: None,
        pending_restore: None,
        awaiting_permission: false,
        toast_ticks: 0,
        in_background: false,
    }));

    {
        let mut c = ctrl.borrow_mut();
        c.load_timer();
        c.session_id = c.db.active_session_id().ok().flatten();
        let large = c.prefs.large_text;
        let week_view = c.prefs.calendar_week_view;
        ui.global::<Theme>().set_large(large);
        let b = ui.global::<Bridge>();
        b.set_cal_week_view(week_view);
        b.set_lib_mode(ss("browse"));
        b.set_timer_presets(model(vec![30, 45, 60, 90, 120, 180]));
        c.refresh_timer();
        c.navigate(P_HOME);
        if let Some(err) = open_error {
            c.show_info(
                InfoCtx::Plain,
                "Erro ao abrir os dados",
                &format!("{err}\n\nO app abriu em modo temporário (nada será salvo). Restaure um backup ou reinstale o app."),
                "OK",
                "",
            );
        }
    }
    register_callbacks(&ui, &ctrl);
    CONTROLLER.with(|c| *c.borrow_mut() = Some(ctrl.clone()));

    let tick_ctrl = ctrl.clone();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(250), move || {
        if let Ok(mut c) = tick_ctrl.try_borrow_mut() {
            c.tick();
        }
    });

    // A interface abriu: remove o marcador de inicialização usado pela tela de diagnóstico.
    let marker = data_dir.join("startup.marker");
    slint::Timer::single_shot(std::time::Duration::from_millis(1500), move || {
        let _ = std::fs::remove_file(&marker);
        log::info!("interface pronta");
    });

    let r = ui.run();
    if let Ok(mut c) = ctrl.try_borrow_mut() {
        c.on_pause();
    }
    CONTROLLER.with(|c| *c.borrow_mut() = None);
    r
}

macro_rules! cb {
    ($b:expr, $ctrl:expr, $setter:ident, |$c:ident $(, $a:ident : $t:ty)*| $body:block) => {{
        let ctrl = $ctrl.clone();
        $b.$setter(move |$($a : $t),*| {
            let mut guard = match ctrl.try_borrow_mut() {
                Ok(g) => g,
                Err(_) => return,
            };
            let $c: &mut Controller = &mut guard;
            $body
        });
    }};
}

fn register_callbacks(ui: &AppWindow, ctrl: &Rc<RefCell<Controller>>) {
    let b = ui.global::<Bridge>();

    cb!(b, ctrl, on_navigate, |c, p: i32| { c.navigate(p); });
    cb!(b, ctrl, on_back, |c| { c.back(); });
    cb!(b, ctrl, on_home_card_clicked, |c, kind: SharedString, id: i32| { c.home_card(&kind, id as Id); });

    // Fichas
    cb!(b, ctrl, on_new_plan, |c| { c.new_plan(); });
    cb!(b, ctrl, on_open_plan, |c, id: i32| { c.open_plan(id as Id); });
    cb!(b, ctrl, on_duplicate_plan, |c, id: i32| {
        match c.db.duplicate_plan(id as Id) {
            Ok(_) => c.toast("Ficha duplicada."),
            Err(e) => c.toast(e.to_string()),
        }
        let r = c.refresh_plans();
        c.report(r);
    });
    cb!(b, ctrl, on_delete_plan, |c, id: i32| {
        let id = if id < 0 { c.plan_id.unwrap_or(0) } else { id as Id };
        let name = c.db.get_plan(id).map(|p| p.name).unwrap_or_default();
        c.ask_confirm(
            ConfirmAction::DeletePlan(id),
            "Excluir ficha",
            &format!("Excluir \"{name}\"? As sessões já realizadas com esta ficha continuam no histórico."),
            "Excluir",
        );
    });
    cb!(b, ctrl, on_toggle_plan_active, |c, id: i32| {
        let r = c.db.get_plan(id as Id).and_then(|p| c.db.set_plan_active(p.id, !p.active));
        c.report(r);
        let p = c.page();
        c.refresh_page(p);
    });
    cb!(b, ctrl, on_open_library, |c| { c.open_library(false); });
    cb!(b, ctrl, on_save_plan_header, |c, name: SharedString, desc: SharedString| { c.save_plan_header(&name, &desc); });
    cb!(b, ctrl, on_set_plan_schedule, |c, weekday: bool| { c.set_plan_schedule(weekday); });
    cb!(b, ctrl, on_add_day, |c| {
        c.autosave_plan_header();
        c.open_day_dialog(None);
    });
    cb!(b, ctrl, on_edit_day, |c, id: i32| { c.open_day_dialog(Some(id as Id)); });
    cb!(b, ctrl, on_delete_day, |c, id: i32| {
        let name = c.db.get_day(id as Id).map(|d| d.name).unwrap_or_default();
        c.ask_confirm(
            ConfirmAction::DeleteDay(id as Id),
            "Excluir dia",
            &format!("Excluir \"{name}\" e seus exercícios desta ficha? O histórico não é afetado."),
            "Excluir",
        );
    });
    cb!(b, ctrl, on_move_day, |c, id: i32, d: i32| {
        let r = c.db.move_day(id as Id, d as i64);
        c.report(r);
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_add_plan_ex, |c, day: i32| { c.add_plan_ex(day as Id); });
    cb!(b, ctrl, on_edit_plan_ex, |c, id: i32| { c.edit_plan_ex(id as Id); });
    cb!(b, ctrl, on_move_plan_ex, |c, id: i32, d: i32| {
        let r = c.db.move_plan_exercise(id as Id, d as i64);
        c.report(r);
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_start_day, |c, id: i32| { c.start_day(id as Id); });
    cb!(b, ctrl, on_day_toggle, |c, i: i32| { c.day_toggle(i); });
    cb!(b, ctrl, on_day_save, |c, name: SharedString| { c.day_save(&name); });
    cb!(b, ctrl, on_pe_pick, |c| {
        c.with_bridge(|b| b.set_dialog(ss("")));
        c.open_library(true);
    });
    cb!(b, ctrl, on_pe_save, |c, s: SharedString, r: SharedString, l: SharedString, rest: SharedString, n: SharedString| {
        c.pe_save(&s, &r, &l, &rest, &n);
    });
    cb!(b, ctrl, on_pe_delete, |c| {
        if let Some(id) = c.pe.as_ref().and_then(|p| p.plan_ex_id) {
            c.ask_confirm(ConfirmAction::DeletePlanEx(id), "Remover exercício", "Remover este exercício da ficha? O histórico não é afetado.", "Remover");
        }
    });

    // Biblioteca
    cb!(b, ctrl, on_lib_search_changed, |c, _t: SharedString| {
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_filter, |c, f: SharedString| {
        let field: &'static str = match f.as_str() {
            "muscle" => "muscle",
            "equipment" => "equipment",
            _ => "pattern",
        };
        c.lib_choose_filter(field);
    });
    cb!(b, ctrl, on_lib_clear_filters, |c| {
        c.with_bridge(|b| {
            b.set_lib_search(ss(""));
            b.set_lib_muscle(ss(""));
            b.set_lib_equipment(ss(""));
            b.set_lib_pattern(ss(""));
        });
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_toggle_archived, |c| {
        c.with_bridge(|b| b.set_lib_show_archived(!b.get_lib_show_archived()));
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_item, |c, id: i32| { c.lib_item(id as Id); });
    cb!(b, ctrl, on_lib_new, |c| { c.open_exercise_form(None); });
    cb!(b, ctrl, on_lib_edit, |c, id: i32| { c.open_exercise_form(Some(id as Id)); });
    cb!(b, ctrl, on_lib_archive, |c, _id: i32| {
        if let Some(id) = c.exf_id {
            let archived = c.db.get_exercise(id).map(|e| e.archived).unwrap_or(false);
            c.close_dialog();
            c.ask_confirm(
                ConfirmAction::ArchiveExercise(id, !archived),
                if archived { "Reativar exercício" } else { "Arquivar exercício" },
                if archived {
                    "O exercício volta a aparecer na biblioteca."
                } else {
                    "O exercício deixa de aparecer na biblioteca. O histórico continua com os registros."
                },
                if archived { "Reativar" } else { "Arquivar" },
            );
        }
    });
    cb!(b, ctrl, on_exf_save, |c, n: SharedString, m: SharedString, t: SharedString, e: SharedString, p: SharedString| {
        c.exf_save(ExerciseInput {
            name: n.to_string(),
            muscle_focus: m.to_string(),
            exercise_type: t.to_string(),
            equipment: e.to_string(),
            movement_pattern: p.to_string(),
        });
    });

    // Diálogos
    cb!(b, ctrl, on_confirm_accepted, |c| { c.confirm_accepted(); });
    cb!(b, ctrl, on_dialog_closed, |c| {
        let was_pe = c.with_bridge(|b| b.get_dialog()) == "plan-ex";
        c.close_dialog();
        if was_pe {
            c.pe = None;
        }
    });
    cb!(b, ctrl, on_choice_made, |c, i: i32| { c.choice_made(i); });
    cb!(b, ctrl, on_text_saved, |c, v: SharedString| { c.text_saved(&v); });
    cb!(b, ctrl, on_info_ok_clicked, |c| { c.info_ok(); });
    cb!(b, ctrl, on_info_alt_clicked, |c| { c.info_alt(); });

    // Sessão
    cb!(b, ctrl, on_session_select, |c, i: i32| {
        let n = c.with_bridge(|b| b.get_session_exercises().row_count()) as i32;
        c.with_bridge(|b| b.set_session_index(i.clamp(0, (n - 1).max(0))));
    });
    cb!(b, ctrl, on_session_record, |c, id: i32, load: SharedString, reps: SharedString| {
        c.session_record(id as Id, &load, &reps);
    });
    cb!(b, ctrl, on_session_undo, |c, id: i32| {
        let r = c.db.clear_set(id as Id);
        c.report(r);
        let r = c.refresh_session();
        c.report(r);
    });
    cb!(b, ctrl, on_session_add_set, |c, id: i32| {
        let r = c.db.add_session_set(id as Id).map(|_| ());
        c.report(r);
        let r = c.refresh_session();
        c.report(r);
    });
    cb!(b, ctrl, on_session_remove_set, |c, id: i32| {
        let r = c.db.remove_last_session_set(id as Id);
        c.report(r);
        let r = c.refresh_session();
        c.report(r);
    });
    cb!(b, ctrl, on_session_substitute, |c, id: i32| { c.open_substitute(id as Id); });
    cb!(b, ctrl, on_session_undo_sub, |c, _id: i32| {
        if let Some(se) = c.sub_se {
            match c.db.undo_substitution(se) {
                Ok(()) => {
                    c.close_dialog();
                    c.toast("Exercício original restaurado nesta sessão.");
                }
                Err(e) => c.toast(e.to_string()),
            }
            let r = c.refresh_session();
            c.report(r);
        }
    });
    cb!(b, ctrl, on_session_ex_notes, |c, id: i32| {
        let notes = c
            .session_id
            .and_then(|sid| c.db.get_session_full(sid).ok())
            .and_then(|f| f.exercises.into_iter().find(|e| e.id == id as Id).map(|e| e.notes))
            .unwrap_or_default();
        c.ask_text(TextCtx::ExerciseNotes(id as Id), "Observação do exercício", "", &notes, true, false);
    });
    cb!(b, ctrl, on_session_edit_notes, |c| {
        let notes = c.session_id.and_then(|sid| c.db.get_session(sid).ok()).map(|s| s.notes).unwrap_or_default();
        c.ask_text(TextCtx::SessionNotes, "Observações da sessão", "", &notes, true, false);
    });
    cb!(b, ctrl, on_session_finish, |c| {
        let Some(sid) = c.session_id else { return };
        let f = match c.db.get_session_full(sid) {
            Ok(f) => f,
            Err(e) => {
                c.toast(e.to_string());
                return;
            }
        };
        if f.completed_sets() == 0 {
            c.toast("Registre pelo menos uma série antes de finalizar, ou descarte a sessão.");
            return;
        }
        if c.prefs.confirm_finish {
            let pending = f.total_sets() - f.completed_sets();
            let text = if pending > 0 {
                format!("{} série(s) não registrada(s) serão descartadas. Após finalizar, a sessão vira registro histórico e não pode ser alterada.", pending)
            } else {
                "Após finalizar, a sessão vira registro histórico e não pode ser alterada.".to_string()
            };
            c.ask_confirm(ConfirmAction::FinishSession, "Finalizar sessão", &text, "Finalizar");
        } else {
            c.finish_session_now();
        }
    });
    cb!(b, ctrl, on_session_discard, |c| {
        c.ask_confirm(
            ConfirmAction::DiscardSession,
            "Descartar sessão",
            "Descartar esta sessão? Nada dela será salvo no histórico.",
            "Descartar",
        );
    });
    {
        let ctrl = ctrl.clone();
        b.on_adjust_load(move |text, dir| match ctrl.try_borrow() {
            Ok(c) => ss(c.adjust_load(&text, dir)),
            Err(_) => text,
        });
    }
    b.on_adjust_reps(|text, dir| {
        let v: i64 = text.trim().parse().unwrap_or(0);
        ss((v + dir as i64).clamp(0, 1000).to_string())
    });
    cb!(b, ctrl, on_sub_search_changed, |c, t: SharedString| { c.sub_search(&t); });
    cb!(b, ctrl, on_sub_select, |c, id: i32, name: SharedString| {
        c.with_bridge(|b| {
            b.set_sub_selected_id(id);
            b.set_sub_selected_name(name.clone());
        });
    });
    cb!(b, ctrl, on_sub_confirm, |c, permanent: bool| { c.sub_confirm(permanent); });

    // Cronômetro
    cb!(b, ctrl, on_open_timer, |c| {
        if c.timer.status != TimerStatus::Running && c.timer.context.is_empty() {
            if let Some(cur) = c.with_bridge(|b| {
                let i = b.get_session_index() as usize;
                b.get_session_exercises().row_data(i)
            }) {
                if c.page() == P_SESSION {
                    c.timer.context = format!("{} — descanso", cur.name);
                    if cur.rest > 0 && c.timer.status != TimerStatus::Paused {
                        c.timer.set_duration(cur.rest as i64 * 1000);
                    }
                }
            }
        }
        c.navigate(P_TIMER);
    });
    cb!(b, ctrl, on_timer_start, |c| {
        c.apply_timer_prefs();
        c.timer.start(Clock::now());
        c.after_timer_start();
    });
    cb!(b, ctrl, on_timer_pause, |c| {
        c.timer.pause(Clock::now());
        c.save_timer();
        c.refresh_timer();
    });
    cb!(b, ctrl, on_timer_reset, |c| {
        c.timer.phase = TimerPhase::Rest;
        c.timer.reset();
        c.save_timer();
        c.refresh_timer();
    });
    cb!(b, ctrl, on_timer_adjust, |c, dir: i32| {
        let step = c.prefs.timer_adjust_step * 1000 * dir as i64;
        c.timer.adjust(Clock::now(), step);
        c.save_timer();
        c.refresh_timer();
    });
    cb!(b, ctrl, on_timer_preset, |c, secs: i32| {
        if c.timer.is_running() {
            c.timer.set_duration(secs as i64 * 1000);
            c.timer.restart(Clock::now());
            c.after_timer_start();
        } else {
            c.timer.set_duration(secs as i64 * 1000);
            c.save_timer();
            c.refresh_timer();
        }
    });
    cb!(b, ctrl, on_timer_custom, |c| {
        let cur = (c.timer.duration_ms / 1000).to_string();
        c.ask_text(TextCtx::TimerCustom, "Duração do cronômetro", "Em segundos (ex.: 75) ou minutos:segundos (ex.: 1:15).", &cur, false, false);
    });

    // Calendário
    cb!(b, ctrl, on_cal_prev, |c| { c.cal_shift(-1); });
    cb!(b, ctrl, on_cal_next, |c| { c.cal_shift(1); });
    cb!(b, ctrl, on_cal_today, |c| {
        let t = cal::today();
        c.cal_selected = t;
        c.cal_month = cal::add_months(t, 0);
        let r = c.refresh_calendar();
        c.report(r);
    });
    cb!(b, ctrl, on_cal_set_week_view, |c, w: bool| {
        c.with_bridge(|b| b.set_cal_week_view(w));
        c.cal_month = cal::add_months(c.cal_selected, 0);
        let r = c.refresh_calendar();
        c.report(r);
    });
    cb!(b, ctrl, on_cal_select, |c, key: SharedString| {
        if let Some(d) = parse_date_key(&key) {
            c.cal_selected = d;
            let week_view = c.with_bridge(|b| b.get_cal_week_view());
            if !week_view && (d.month() != c.cal_month.month() || d.year() != c.cal_month.year()) {
                c.cal_month = cal::add_months(d, 0);
            }
            let r = c.refresh_calendar();
            c.report(r);
        }
    });
    cb!(b, ctrl, on_cal_entry, |c, i: i32| { c.cal_entry(i as usize); });

    // Histórico
    cb!(b, ctrl, on_hist_set_tab, |c, i: i32| {
        c.with_bridge(|b| b.set_hist_tab(i));
        let r = c.refresh_history();
        c.report(r);
    });
    cb!(b, ctrl, on_hist_open, |c, id: i32| { c.open_detail(id as Id); });
    cb!(b, ctrl, on_evo_open, |c, key: SharedString| {
        c.evo_key = key.to_string();
        c.navigate(P_EVO);
    });
    cb!(b, ctrl, on_evo_set_period, |c, i: i32| {
        c.evo_period = i;
        let r = c.refresh_evolution();
        c.report(r);
    });
    cb!(b, ctrl, on_evo_set_metric, |c, i: i32| {
        c.evo_metric = i;
        let r = c.refresh_evolution();
        c.report(r);
    });

    // Configurações
    cb!(b, ctrl, on_settings_open, |c, i: i32| {
        c.with_bridge(|b| b.set_settings_section(i));
        let r = c.refresh_settings();
        c.report(r);
    });
    cb!(b, ctrl, on_pref_toggle, |c, k: SharedString| { c.pref_toggle(&k); });
    cb!(b, ctrl, on_pref_choose, |c, k: SharedString| { c.pref_choose(&k); });
    cb!(b, ctrl, on_pref_step, |c, k: SharedString, d: i32| { c.pref_step(&k, d); });
    cb!(b, ctrl, on_test_alert, |c| {
        if c.prefs.sound_enabled {
            c.platform.play_tone(c.prefs.tone_rest_end, 600);
        }
        if c.prefs.vibrate {
            c.platform.vibrate(400);
        }
        if !c.prefs.sound_enabled && !c.prefs.vibrate {
            c.toast("Som e vibração estão desligados.");
        }
    });
    cb!(b, ctrl, on_notif_settings, |c| { c.platform.open_notification_settings(); });
    cb!(b, ctrl, on_data_import, |c| {
        c.file_op = Some(FileOp::ImportXlsx);
        c.platform.open_document(XLSX_MIMES);
    });
    cb!(b, ctrl, on_data_export, |c| {
        c.ask_choice(
            ChoiceCtx::ExportKind,
            "Exportar planilha",
            "Escolha o que exportar. Depois você escolhe onde salvar o arquivo.",
            vec!["Fichas de treino".into(), "Histórico de sessões".into(), "Tudo (fichas + histórico)".into()],
            -1,
        );
    });
    cb!(b, ctrl, on_data_template, |c| { c.data_template(); });
    cb!(b, ctrl, on_data_backup, |c| { c.data_backup(); });
    cb!(b, ctrl, on_data_restore, |c| {
        c.file_op = Some(FileOp::Restore);
        c.platform.open_document(&["application/zip", "application/x-zip-compressed", "application/octet-stream"]);
    });
    cb!(b, ctrl, on_data_ics, |c| { c.data_ics(); });

    // Importação
    cb!(b, ctrl, on_imp_choose_sheet, |c| {
        let (names, sel) = match c.import.as_ref() {
            Some(st) => (st.workbook.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), st.sheet as i32),
            None => return,
        };
        c.ask_choice(ChoiceCtx::ImportSheet, "Aba da planilha", "", names, sel);
    });
    cb!(b, ctrl, on_imp_map, |c, col: i32| {
        let Some(st) = c.import.as_ref() else { return };
        let cur = st.mapping.columns.get(col as usize).copied().flatten();
        let mut opts = vec!["Ignorar esta coluna".to_string()];
        opts.extend(Field::ALL.iter().map(|f| f.label().to_string()));
        let sel = cur.map(|f| f.index() as i32 + 1).unwrap_or(0);
        c.ask_choice(ChoiceCtx::ImportMap(col as usize), "Campo da coluna", "", opts, sel);
    });
    cb!(b, ctrl, on_imp_set_replace, |c, v: bool| {
        if let Some(st) = c.import.as_mut() {
            st.replace = v;
        }
        c.refresh_import();
    });
    cb!(b, ctrl, on_imp_set_reviewed, |c, v: bool| { c.with_bridge(|b| b.set_imp_reviewed(v)); });
    cb!(b, ctrl, on_imp_confirm, |c| { c.imp_confirm(); });
    cb!(b, ctrl, on_imp_cancel, |c| {
        c.import = None;
        c.back();
    });
}
