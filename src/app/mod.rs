//! Controlador da interface: liga a UI Slint (Bridge) aos casos de uso em Rust.

mod format;

use crate::i18n::{tr, Lang};
use crate::application::prefs::{AnimMode, Prefs, RestEndAction, ThemeMode, TONE_NAMES};
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
use crate::application::undo::{UndoKind, UndoOp};
use crate::application::gestures::{self, GestureAction, GestureContext, GestureEvent};
use crate::application::{shortcuts, storage, widget};
use crate::domain::input::{self, FieldKind};
use crate::platform::{FileResult, HapticEvent, Platform, ALARM_END, ALARM_PRE};
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
const P_FOCUS: i32 = 16;
const P_SEARCH: i32 = 17;
const P_ONBOARDING: i32 = 18;
const P_EXDETAIL: i32 = 19;
const P_TOOLS: i32 = 22;
const P_COMPARE: i32 = 23;

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
    DeleteLocalBackup(std::path::PathBuf),
}

#[derive(Debug, Clone)]
enum ChoiceCtx {
    Tone(&'static str),
    Appearance(&'static str),
    Unit,
    LoadStep,
    LibFilter(&'static str, Vec<String>),
    ImportSheet,
    ImportMap(usize),
    ExportKind,
    Template,
    Language,
    Illustration,
    Effort(Id, &'static str),
    Feeling,
    Compare(Vec<Id>),
    Shortcut(usize),
}

#[derive(Debug, Clone)]
enum TextCtx {
    SessionNotes,
    ExerciseNotes(Id),
    TimerCustom,
    ToolsValue(&'static str),
    BackupPassword,
    RestorePassword,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InfoCtx {
    NotifPermission { then_enable: bool },
    ActiveSession,
    /// Sessão aberta encontrada ao iniciar o app (fechamento inesperado, bateria etc.).
    RecoverSession,
    Template(&'static str),
    Plain,
}

#[derive(Debug, Clone, PartialEq)]
enum FileOp {
    ImportXlsx,
    Restore,
    /// (mensagem já traduzida, é um backup salvo pelo usuário fora do app?)
    Save(String, bool),
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
    /// Índice da aba; `usize::MAX` = todas as abas.
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
    /// Ilustração escolhida no formulário de exercício ('' = automática).
    exf_illustration: String,
    lib_pick: bool,
    /// Quantos itens da biblioteca mostrar (carregamento incremental).
    lib_limit: usize,
    /// Escolhendo na biblioteca uma alternativa para este exercício.
    alt_for: Option<Id>,
    /// Arquivo de backup aguardando senha para validar.
    restore_path: Option<std::path::PathBuf>,
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
    /// Modo foco: índice da série no exercício atual e rascunhos não salvos por série.
    focus_set: usize,
    drafts: HashMap<Id, (String, String)>,
    /// Exercícios da sessão em que o usuário pediu "Repetir valores da sessão anterior" (8.3).
    repeat_prev: std::collections::HashSet<Id>,
    /// Estado das calculadoras (8.5/8.6).
    tools: ToolsState,
    /// Sessões comparadas (A = anterior, B = escolhida).
    cmp: Option<(Id, Id)>,
    /// Sessões carregadas na lista do histórico (paginação).
    hist_limit: usize,
    /// Mês do relatório (ano, mês).
    rep_month: (i32, u32),
    /// Última operação reversível (aviso "Desfazer").
    undo: Option<UndoOp>,
    /// Próxima tecla substitui o valor (campo recém-selecionado/preenchido).
    focus_fresh: bool,
    /// Área do gráfico de evolução em px (para o caminho em escala real).
    evo_chart: (f32, f32),
    evo_selected: i32,
    search_hits: Vec<crate::application::search::SearchHit>,
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
        self.with_bridge(|b| {
            b.set_toast(ss(&m));
            b.set_toast_action(ss(""));
        });
        self.toast_ticks = 14; // ~3.5 s
    }

    /// Aviso com a ação "Desfazer" por 5 segundos (3.6).
    fn toast_undo(&mut self, msg: impl AsRef<str>, op: UndoOp) {
        let m = msg.as_ref().to_string();
        self.undo = Some(op);
        self.with_bridge(|b| {
            b.set_toast(ss(&m));
            b.set_toast_action(ss(tr("Desfazer")));
        });
        self.toast_ticks = 20; // 5 s
    }

    fn haptic(&self, e: HapticEvent) {
        if self.prefs.haptics {
            self.platform.haptic(e);
        }
    }

    fn undo_last(&mut self) {
        let Some(op) = self.undo.take() else { return };
        self.with_bridge(|b| {
            b.set_toast(ss(""));
            b.set_toast_action(ss(""));
        });
        if op.expired(now_ms()) {
            self.toast(tr("Não é mais possível desfazer."));
            return;
        }
        match op.revert(&mut self.db) {
            Ok(()) => {
                self.haptic(HapticEvent::Undo);
                if let UndoKind::RecordSet { set_id, .. } | UndoKind::ClearSet { set_id, .. } = &op.kind {
                    self.drafts.remove(set_id);
                }
                let p = self.page();
                self.refresh_page(p);
                if p == P_FOCUS {
                    let r = self.refresh_session();
                    self.report(r);
                }
                self.toast(trf!("Desfeito: {}", op.label.to_lowercase()));
            }
            Err(e) => self.toast(trf!("Não foi possível desfazer: {}", e)),
        }
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
            b.set_text_password(false);
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
        // Ordem visual da barra inferior: Calendário, Treinos, Home, Histórico.
        let tab_pos = |p: i32| -> i32 { match p {
            P_CAL => 0,
            P_PLANS => 1,
            P_HOME => 2,
            P_HIST => 3,
            _ => 10,
        } };
        let cur = self.page();
        let dir = if page < 10 && cur < 10 {
            (tab_pos(page) - tab_pos(cur) as i32).signum()
        } else if page < 10 {
            -1
        } else {
            1
        };
        self.with_bridge(|b| b.set_nav_dir(dir));
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
            P_EXDETAIL => self.refresh_exercise_detail(),
            P_COMPARE => self.refresh_compare(),
            P_TOOLS => {
                self.refresh_tools();
                Ok(())
            }
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
            P_FOCUS => {
                self.refresh_focus();
                Ok(())
            }
            P_SEARCH => {
                self.refresh_search();
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
        if page == P_ONBOARDING {
            let step = self.with_bridge(|b| b.get_onboarding_step());
            if step > 0 {
                self.with_bridge(|b| b.set_onboarding_step(step - 1));
            } else {
                self.onboarding_finish("explore");
            }
            return;
        }
        if page == P_SETTINGS && self.with_bridge(|b| b.get_settings_section()) > 0 {
            self.with_bridge(|b| b.set_settings_section(0));
            return;
        }
        if page == P_EDITOR {
            self.autosave_plan_header();
        }
        self.with_bridge(|b| b.set_nav_dir(-1));
        if page == P_LIBRARY && self.alt_for.is_some() {
            let base = self.alt_for.take();
            self.lib_pick = false;
            self.with_bridge(|b| b.set_lib_mode(ss("browse")));
            let r = self.refresh_library();
            self.report(r);
            if let Some(id) = base {
                self.open_exercise_form(Some(id));
            }
            return;
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
                tr("SESSÃO EM ANDAMENTO"),
                format!("{} — {}", f.session.plan_name, f.session.schedule_name),
                trf!(
                    "{} de {} séries · iniciada às {}",
                    f.completed_sets(),
                    f.total_sets(),
                    local_time_of(f.session.started_at)
                ),
                tr("Continuar"),
                sid,
            ));
        }

        let planned = cal::planned_on(today, today, &plans, &next, &done_plan_ids);
        if planned.is_empty() {
            let l2 = if plans.is_empty() {
                tr("Crie sua primeira ficha em Treinos.").to_string()
            } else {
                tr("Nenhum treino planejado para hoje. Toque para escolher uma ficha.").to_string()
            };
            let l1 = if done_today.is_empty() { tr("Dia livre").to_string() } else { tr("Treino do dia concluído").to_string() };
            cards.push(card("plans", tr("TREINO DE HOJE"), l1, l2, tr("Treinos"), 0));
        } else {
            for p in &planned {
                let done = done_today.iter().any(|s| s.schedule_name == p.day_name && s.plan_name == p.plan_name);
                cards.push(card(
                    "today",
                    if p.from_sequence { tr("TREINO DE HOJE · PRÓXIMO DA SEQUÊNCIA") } else { tr("TREINO DE HOJE") },
                    format!("{} — {}", p.plan_name, p.day_name),
                    trf!("{} exercício(s){}", p.exercises, if done { tr(" · já realizado hoje") } else { "" }),
                    tr("Iniciar"),
                    p.day_id,
                ));
            }
        }

        let favs: Vec<&PlanFull> = plans.iter().filter(|p| p.plan.favorite).collect();
        if let Some(first) = favs.first() {
            let names = favs.iter().map(|p| p.plan.name.clone()).collect::<Vec<_>>();
            cards.push(card(
                "favorite",
                tr("ACESSO RÁPIDO · FICHAS FAVORITAS"),
                names.iter().take(3).cloned().collect::<Vec<_>>().join(" · "),
                if favs.len() > 3 { trf!("+{} favorita(s)", favs.len() - 3) } else { tr("Toque para abrir").into() },
                tr("Abrir"),
                first.plan.id,
            ));
        }

        let mut upcoming = Vec::new();
        for i in 1..=7 {
            let d = today + Duration::days(i);
            for p in cal::planned_on(d, today, &plans, &next, &[]) {
                upcoming.push(format!(
                    "{} {:02}/{:02} — {} ({})",
                    tr(WEEKDAY_SHORT[cal::weekday_index(d) as usize]),
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
                Some(trf!("Próximo da sequência: {} ({})", d.day.name, p.plan.name))
            })
            .collect();
        if !upcoming.is_empty() || !seq_names.is_empty() {
            let mut lines: Vec<String> = upcoming.into_iter().take(3).collect();
            lines.extend(seq_names.into_iter().take(2));
            let first = lines.remove(0);
            cards.push(card("calendar", tr("PRÓXIMOS TREINOS"), first, lines.join("\n"), tr("Calendário"), 0));
        }

        let all = self.db.list_session_summaries(None, None)?;
        if let Some(last) = all.first() {
            let dur = last.ended_at.map(|e| format!(" · {}", fmt_minutes(e - last.started_at))).unwrap_or_default();
            cards.push(card(
                "last",
                tr("ÚLTIMA SESSÃO"),
                format!("{} — {}", last.plan_name, last.schedule_name),
                trf!(
                    "{} · {} exercício(s) · {} série(s){}",
                    fmt_date_time(last.started_at),
                    last.exercises,
                    last.sets,
                    dur
                ),
                tr("Detalhes"),
                last.id,
            ));
        }

        if self.prefs.module_records {
            let recs = self.db.recent_records(now_ms() - 30 * 86_400_000, None)?;
            if let Some(r) = recs.first() {
                cards.push(card(
                    "records",
                    tr("RECORDES RECENTES"),
                    format!("★ {} — {}", r.exercise_name, r.kinds[0].label()),
                    trf!(
                        "{} × {} em {}{}",
                        format_load_unit(r.load_g, self.prefs.unit),
                        r.reps,
                        fmt_date_short(r.at_ms),
                        if recs.len() > 1 { trf!(" · {} recorde(s) em 30 dias", recs.len()) } else { String::new() }
                    ),
                    tr("Evolução"),
                    0,
                ));
            }
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
            tr("SESSÕES NA SEMANA"),
            trf!("{} sessão(ões) concluída(s)", week.len()),
            if planned_week > 0 {
                trf!("{} planejada(s) por dia da semana · {} séries registradas", planned_week, sets)
            } else {
                trf!("{} séries registradas", sets)
            },
            tr("Histórico"),
            0,
        ));

        if self.prefs.module_backup_reminder {
            let last_ext = self.db.get_pref(storage::PREF_LAST_EXTERNAL)?.and_then(|v| v.parse::<i64>().ok());
            let has_data = !all.is_empty() || !plans.is_empty();
            if storage::external_copy_reminder_due(last_ext, has_data, now_ms()) {
                let local = storage::list_local_backups(&self.platform.data_dir());
                cards.push(card(
                    "backup",
                    tr("LEMBRETE · CÓPIA DO BACKUP"),
                    match last_ext {
                        Some(t) => trf!("Última cópia salva por você fora do app: {}", fmt_date_short(t)),
                        None => tr("Nenhuma cópia salva fora do app por aqui").to_string(),
                    },
                    trf!(
                        "{} backup(s) no aparelho. Eles não protegem contra perda ou desinstalação: salve uma cópia fora do aparelho.",
                        local.len()
                    ),
                    tr("Backups"),
                    0,
                ));
            }
        }

        let sc: Vec<ShortcutRow> = self
            .home_shortcuts()
            .iter()
            .map(|c| ShortcutRow { code: ss(c), label: ss(shortcuts::label(c)), icon: ss(shortcuts::icon(c)) })
            .collect();
        self.update_widget(&cards);
        let date = cal::date_long(today);
        self.with_bridge(|b| {
            b.set_home_date(ss(capitalize(&date)));
            b.set_home_cards(model(cards));
            b.set_home_shortcuts(model(sc));
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
            "favorite" => self.open_plan(id),
            "records" => {
                self.with_bridge(|b| b.set_hist_tab(1));
                self.navigate(P_HIST);
            }
            "backup" => self.home_shortcut("backups"),
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
                        if mask == 0 { tr("Dias da semana").to_string() } else { weekdays_text(mask) }
                    }
                    ScheduleType::Sequence => trf!(
                        "Sequência {}",
                        p.days.iter().map(|d| d.day.name.clone()).collect::<Vec<_>>().join(" → ")
                    ),
                };
                PlanRow {
                    id: p.plan.id as i32,
                    name: ss(&p.plan.name),
                    subtitle: ss(trf!("{} dia(s) · {} exercício(s) · {}", p.days.len(), n, org)),
                    active: p.plan.active,
                    favorite: p.plan.favorite,
                }
            })
            .collect();
        self.with_bridge(|b| b.set_plans(model(rows)));
        Ok(())
    }

    fn new_plan(&mut self) {
        let mut name = tr("Nova ficha").to_string();
        let mut i = 2;
        while self.db.find_plan_by_name(&name).ok().flatten().is_some() {
            name = trf!("Nova ficha {}", i);
            i += 1;
        }
        match self.db.create_plan(&name, "", ScheduleType::Weekday) {
            Ok(id) => {
                self.plan_id = Some(id);
                self.navigate(P_EDITOR);
                self.toast(tr("Ficha criada. Dê um nome e adicione os dias de treino."));
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
        let mut sugg: HashMap<Id, crate::training::progression::ProgressionSuggestion> = HashMap::new();
        if self.prefs.module_progression {
            for d in &p.days {
                for e in &d.exercises {
                    if let Ok(Some(s)) = self.db.progression_for(e.id, self.prefs.load_step_g, unit) {
                        let dismissed = self.db.get_pref(&format!("progression.dismissed.{}", e.id)).ok().flatten();
                        if dismissed.as_deref() != Some(&s.based_on_session.to_string()) {
                            sugg.insert(e.id, s);
                        }
                    }
                }
            }
        }
        let days: Vec<DayRow> = p
            .days
            .iter()
            .map(|d| {
                let subtitle = match p.plan.schedule_type {
                    ScheduleType::Weekday => {
                        if d.day.weekdays == 0 {
                            tr("Sem dia da semana definido — toque em editar").to_string()
                        } else {
                            weekdays_text(d.day.weekdays)
                        }
                    }
                    ScheduleType::Sequence => trf!("Posição {} na sequência", d.day.sequence_order + 1),
                };
                DayRow {
                    id: d.day.id as i32,
                    name: ss(&d.day.name),
                    subtitle: ss(subtitle),
                    exercises: model(
                        d.exercises
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (i, e, plan_group_info(&d.exercises, i)))
                            .map(|(_, e, (glabel, gpos, linked, last))| PlanExRow {
                                id: e.id as i32,
                                name: ss(&e.exercise.name),
                                detail: ss(target_text(&e.target, e.rest_seconds, unit)),
                                notes: ss(&e.notes),
                                suggestion: ss(sugg.get(&e.id).map(|s| trf!(
                                    "Sugestão: aumentar de {} para {}",
                                    format_load_unit(s.current_load_g, unit),
                                    format_load_unit(s.suggested_load_g, unit)
                                )).unwrap_or_default()),
                                suggestion_detail: ss(sugg.get(&e.id).map(|s| s.justification.clone()).unwrap_or_default()),
                                kinds: model(
                                    (1..=e.target.sets)
                                        .map(|n| {
                                            let k = e.kind_of(n);
                                            ss(if k == SetKind::Normal { n.to_string() } else { format!("{} · {}", n, k.short()) })
                                        })
                                        .collect::<Vec<_>>(),
                                ),
                                group_label: ss(glabel),
                                group_pos: gpos,
                                linked,
                                last,
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
                self.toast(tr("Ficha salva."));
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
                tr("Sessão em andamento"),
                tr("Já existe uma sessão em andamento. Continue-a ou finalize/descarte antes de iniciar outra."),
                tr("Abrir sessão"),
                tr("Fechar"),
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
                (trf!("Treino {}", letter), 0)
            }
        };
        self.day_ctx = Some((day, mask));
        self.with_bridge(|b| {
            b.set_day_title(ss(if day.is_some() { tr("Editar dia") } else { tr("Novo dia de treino") }));
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
        self.fill_pe_form(tr("Adicionar exercício"), "", defaults, self.prefs.timer_default_seconds, "", false);
        self.open_library(true);
    }

    fn edit_plan_ex(&mut self, id: Id) {
        match self.db.get_plan_exercise(id) {
            Ok(pe) => {
                self.pe = Some(PeCtx { day_id: pe.schedule_id, plan_ex_id: Some(id), exercise_id: Some(pe.exercise.id) });
                self.fill_pe_form(tr("Editar exercício"), &pe.exercise.name, pe.target, pe.rest_seconds, &pe.notes, true);
                self.with_bridge(|b| b.set_dialog(ss("plan-ex")));
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn pe_save(&mut self, sets: &str, reps: &str, load: &str, rest: &str, notes: &str) {
        let Some(ctx) = self.pe.clone() else { return };
        let r = (|| -> AppResult<()> {
            let Some(ex_id) = ctx.exercise_id else { return validation(tr("Escolha o exercício.")) };
            let sets: i64 = sets.trim().parse().map_err(|_| AppError::Validation(tr("Séries: informe um número.").into()))?;
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
            exercise_type: b.get_lib_type().to_string(),
            favorites_only: b.get_lib_favorites(),
            custom_only: b.get_lib_custom(),
            include_archived: false,
            archived_only: b.get_lib_show_archived() && !self.lib_pick,
        })
    }

    fn refresh_library(&mut self) -> AppResult<()> {
        let f = self.lib_filter_struct();
        let page = self.db.list_exercises_page(&f, self.lib_limit)?;
        let active = [!f.muscle.is_empty(), !f.equipment.is_empty(), !f.pattern.is_empty(), !f.exercise_type.is_empty(), f.favorites_only, f.custom_only, f.archived_only]
            .iter()
            .filter(|x| **x)
            .count() as i32;
        let count = if page.items.len() < page.total {
            trf!("Mostrando {} de {} exercício(s)", page.items.len(), page.total)
        } else {
            trf!("{} exercício(s)", page.total)
        };
        let has_more = page.items.len() < page.total;
        let rows: Vec<ExRow> = page.items.iter().map(|e| ex_row(e, "")).collect();
        self.with_bridge(|b| {
            b.set_library(model(rows));
            b.set_lib_active_filters(active);
            b.set_lib_count(ss(count));
            b.set_lib_has_more(has_more);
        });
        Ok(())
    }

    fn lib_item(&mut self, id: Id) {
        if let Some(base) = self.alt_for {
            self.alt_for = None;
            self.lib_pick = false;
            self.with_bridge(|b| b.set_lib_mode(ss("browse")));
            let r = if id == base {
                validation(tr("Escolha um exercício diferente."))
            } else {
                self.db
                    .add_alternative(base, id, tr("Cadastrada pelo usuário"))
                    .and_then(|_| self.db.add_alternative(id, base, tr("Cadastrada pelo usuário")))
            };
            match r {
                Ok(()) => self.toast(tr("Alternativa cadastrada (vale nos dois sentidos).")),
                Err(e) => self.toast(e.to_string()),
            }
            let r = self.refresh_library();
            self.report(r);
            self.open_exercise_form(Some(base));
            return;
        }
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
            self.open_exercise_detail(id);
        }
    }

    /// Detalhes do exercício (evolução 5.1): ilustração, técnica, observações, alternativas e histórico.
    fn open_exercise_detail(&mut self, id: Id) {
        self.with_bridge(|b| b.set_exd_id(id as i32));
        if self.page() == P_EXDETAIL {
            let r = self.refresh_exercise_detail();
            self.report(r);
        } else {
            self.navigate(P_EXDETAIL);
        }
    }

    fn refresh_exercise_detail(&mut self) -> AppResult<()> {
        let id = self.with_bridge(|b| b.get_exd_id()) as Id;
        let e = self.db.get_exercise(id)?;
        let unit = self.prefs.unit;
        let meta = [e.muscle_focus.as_str(), e.exercise_type.as_str(), e.equipment.as_str(), e.movement_pattern.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let key = format!("id:{}", id);
        let pts = self.db.exercise_evolution(&key, None)?;
        let last = match pts.last() {
            Some(p) => trf!(
                "Última vez em {}: {} série(s), carga máxima {}, melhor série com {} reps. {} sessão(ões) no total.",
                fmt_date_short(p.started_at),
                p.sets,
                format_load_unit(p.max_load_g, unit),
                p.best_reps,
                pts.len()
            ),
            None => tr("Você ainda não registrou este exercício em uma sessão concluída.").to_string(),
        };
        let bests = if pts.is_empty() || !self.prefs.module_records {
            String::new()
        } else {
            let b = self.db.current_bests(&key)?;
            trf!(
                "Melhores marcas — carga: {} · repetições: {} · 1RM estimada: {}",
                format_load_unit(b.load_g, unit),
                b.reps,
                format_load_unit(b.e1rm_g, unit)
            )
        };
        let alts: Vec<ExRow> = self.db.list_alternatives(id).unwrap_or_default().iter().map(|(a, r)| ex_row(a, r)).collect();
        let art = crate::exercise_media::resolve(&e);
        self.with_bridge(|b| {
            b.set_exd_name(ss(&e.name));
            b.set_exd_art(ss(art));
            b.set_exd_meta(ss(meta));
            b.set_exd_description(ss(&e.description));
            b.set_exd_notes(ss(&e.notes));
            b.set_exd_video(ss(&e.video_url));
            b.set_exd_favorite(e.favorite);
            b.set_exd_archived(e.archived);
            b.set_exd_custom(e.custom);
            b.set_exd_last(ss(last));
            b.set_exd_bests(ss(bests));
            b.set_exd_has_history(!pts.is_empty());
            b.set_exd_alternatives(model(alts));
        });
        Ok(())
    }

    fn exd_open_video(&mut self) {
        let id = self.with_bridge(|b| b.get_exd_id()) as Id;
        let Ok(e) = self.db.get_exercise(id) else { return };
        if e.video_url.is_empty() {
            return;
        }
        if !self.platform.open_url(&e.video_url) {
            self.toast(tr("Nenhum aplicativo disponível para abrir o vídeo. Verifique o link."));
        }
    }

    fn open_exercise_form(&mut self, id: Option<Id>) {
        let e = id.and_then(|i| self.db.get_exercise(i).ok()).unwrap_or_default();
        self.exf_id = id;
        self.exf_illustration = e.illustration.clone();
        self.with_bridge(|b| {
            b.set_exf_title(ss(if id.is_some() { tr("Editar exercício") } else { tr("Novo exercício") }));
            b.set_exf_name(ss(&e.name));
            b.set_exf_muscle(ss(&e.muscle_focus));
            b.set_exf_type(ss(&e.exercise_type));
            b.set_exf_equipment(ss(&e.equipment));
            b.set_exf_pattern(ss(&e.movement_pattern));
            b.set_exf_description(ss(&e.description));
            b.set_exf_notes(ss(&e.notes));
            b.set_exf_art(ss(if e.illustration.is_empty() { crate::exercise_media::resolve(&e) } else { e.illustration.as_str() }));
            b.set_exf_art_label(ss(if e.illustration.is_empty() { tr("Automática") } else { crate::exercise_media::label(&e.illustration) }));
            b.set_exf_video(ss(&e.video_url));
            b.set_exf_favorite(e.favorite);
            b.set_exf_editing(id.is_some());
            b.set_exf_archived(e.archived);
            b.set_dialog(ss("exercise"));
        });
        self.refresh_exf_alternatives();
    }

    fn refresh_exf_alternatives(&mut self) {
        let rows: Vec<ExRow> = match self.exf_id {
            Some(id) => self.db.list_alternatives(id).unwrap_or_default().iter().map(|(e, r)| ex_row(e, r)).collect(),
            None => vec![],
        };
        self.with_bridge(|b| b.set_exf_alternatives(model(rows)));
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
                    self.toast(tr("Exercício salvo."));
                    if self.page() == P_EXDETAIL {
                        let r = self.refresh_exercise_detail();
                        self.report(r);
                    }
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
            "type" => b.get_lib_type(),
            _ => b.get_lib_pattern(),
        });
        let mut opts = vec![tr("Todos").to_string()];
        opts.extend(values.iter().cloned());
        let sel = values.iter().position(|v| v.as_str() == current.as_str()).map(|i| i as i32 + 1).unwrap_or(0);
        let title = match field {
            "muscle" => tr("Grupo muscular"),
            "equipment" => tr("Equipamento"),
            "type" => tr("Tipo de exercício"),
            _ => tr("Padrão de movimento"),
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
            _ => self.toast(tr("Nenhuma sessão em andamento.")),
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
        let feeling = f.session.feeling.map(crate::history::analytics::feeling_label).unwrap_or("");
        self.with_bridge(|b| b.set_session_feeling(ss(feeling)));
        let mut rows = Vec::new();
        let tags: Vec<String> = f.exercises.iter().map(|e| e.group_tag.clone()).collect();
        for (idx, e) in f.exercises.iter().enumerate() {
            // Sessão anterior do mesmo exercício (8.3): sempre exibida; só preenche os campos se o usuário
            // ativou "Sugerir carga da última sessão" ou tocou em "Repetir valores da sessão anterior".
            let last = e.performed_exercise_id.and_then(|id| self.db.last_performance(id, sid).ok().flatten());
            let fill_prev = self.prefs.prefill_last || self.repeat_prev.contains(&e.id);
            let (prev_title, prev_lines) = match &last {
                Some(l) if l.sets.iter().any(|x| x.1.is_some()) => (
                    trf!("SESSÃO ANTERIOR · {}", fmt_date_short(l.session_started_at)),
                    l.sets
                        .iter()
                        .filter_map(|(n, r, ld)| {
                            let r = (*r)?;
                            Some(ss(trf!("Série {}: {} × {}", n, format_load_unit(ld.unwrap_or(0), unit), r)))
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => (String::new(), vec![]),
            };
            let (ga, gb) = crate::training::groups::members(&tags, idx);
            let group = match crate::training::groups::GroupKind::of_size(gb - ga + 1) {
                Some(k) => {
                    let next = if idx < gb { trf!(" · depois: {}", f.exercises[idx + 1].performed_name) } else { tr(" · depois: descanso").to_string() };
                    trf!("{} {}/{}{}", k.label(), idx - ga + 1, gb - ga + 1, next)
                }
                None => String::new(),
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
                            let pl = if fill_prev { prev.and_then(|p| p.2) } else { None };
                            let l = pl.unwrap_or(s.planned_load_g);
                            let r = if self.repeat_prev.contains(&e.id) { prev.and_then(|p| p.1) } else { None }
                                .unwrap_or(s.planned_reps_max);
                            (if l > 0 { format_load(l, unit) } else { String::new() }, r.to_string())
                        }
                    };
                    let hint = match prev {
                        Some((_, Some(r), Some(l))) => trf!(
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
                        planned: ss(trf!(
                            "plan. {}{}",
                            if s.planned_load_g > 0 { trf!("{} × ", format_load_unit(s.planned_load_g, unit)) } else { String::new() },
                            format_reps(s.planned_reps_min, s.planned_reps_max)
                        )),
                        load: ss(load),
                        reps: ss(reps),
                        done: s.completed_at.is_some(),
                        hint: ss(hint),
                        kind: ss(s.kind.short()),
                        rpe: ss(s.rpe.map(fmt_rpe).unwrap_or_default()),
                        rir: ss(s.rir.map(|r| r.to_string()).unwrap_or_default()),
                    }
                })
                .collect();
            let planned = match e.sets.first() {
                Some(s) => trf!(
                    "{} série(s) · {} reps{}{}",
                    e.sets.len(),
                    format_reps(s.planned_reps_min, s.planned_reps_max),
                    if s.planned_load_g > 0 { format!(" · {}", format_load_unit(s.planned_load_g, unit)) } else { String::new() },
                    if e.rest_seconds > 0 { trf!(" · descanso {}", format_duration_s(e.rest_seconds)) } else { String::new() }
                ),
                None => tr("Sem séries").to_string(),
            };
            let substitution = match e.substitution {
                SubstitutionType::None => String::new(),
                SubstitutionType::Temporary => trf!("Substitui {} · só nesta sessão", e.original_name),
                SubstitutionType::Permanent => trf!("Substitui {} · ficha atualizada", e.original_name),
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
                technique: ss(e
                    .performed_exercise_id
                    .and_then(|id| self.db.get_exercise(id).ok())
                    .map(|x| [x.description, x.notes].iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default()),
                art: ss(self.session_art(e)),
                group: ss(group),
                prev_title: ss(prev_title),
                prev_lines: model(prev_lines),
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
            b.set_session_progress(ss(trf!("{} · {}/{} séries", f.session.plan_name, done, total)));
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
        match input::to_values(load, reps, self.prefs.unit) {
            Ok((load_g, reps_n)) => {
                self.record_set_values(set_id, load_g, reps_n);
            }
            Err(e) => self.toast(e),
        }
    }

    /// Registra a série (usado pela sessão e pelo modo foco). Devolve true se gravou.
    fn record_set_values(&mut self, set_id: Id, load_g: i64, reps_n: i64) -> bool {
        let before = self.db.get_session_set(set_id).ok();
        let r = (|| -> AppResult<(String, i64)> {
            self.db.record_set(set_id, reps_n, load_g)?;
            let sid = self.session_id.unwrap_or(0);
            let f = self.db.get_session_full(sid)?;
            let (ex, set) = f
                .exercises
                .iter()
                .find_map(|e| e.sets.iter().find(|s| s.id == set_id).map(|s| (e.clone(), s.clone())))
                .ok_or_else(|| AppError::NotFound(tr("Série não encontrada.").into()))?;
            Ok((trf!("{} — após a série {}", ex.performed_name, set.set_number), ex.rest_seconds))
        })();
        match r {
            Ok((context, rest)) => {
                self.drafts.remove(&set_id);
                self.haptic(HapticEvent::SetRegistered);
                let was_done = before.as_ref().map(|b| b.completed_at.is_some()).unwrap_or(false);
                // O descanso anterior termina quando a série seguinte é registrada.
                if !was_done {
                    self.finalize_rest();
                }
                let r = self.refresh_session();
                self.report(r);
                let record = self.check_record(set_id);
                let mut msg = if was_done { tr("Série atualizada.").to_string() } else { tr("Série registrada.").to_string() };
                // Grupos (8.2): dentro do grupo vai direto ao próximo exercício, sem descanso.
                let mut group_rest = true;
                if !was_done {
                    if let Some((go_to, rest, next_name)) = self.group_after_set(set_id) {
                        group_rest = rest;
                        if let Some(k) = go_to {
                            self.with_bridge(|b| b.set_session_index(k as i32));
                            if let Some(e) = self.current_session_exercise() {
                                self.focus_set = Self::first_open_set(&e);
                            }
                            let r = self.refresh_session();
                            self.report(r);
                            if self.page() == P_FOCUS {
                                self.refresh_focus();
                            }
                        }
                        if !rest {
                            msg = trf!("Série registrada. Agora: {} (sem descanso no grupo).", next_name);
                        }
                    }
                }
                if !was_done && !group_rest {
                    // Sem descanso entre exercícios do grupo.
                } else if !was_done && self.prefs.auto_start_rest {
                    let secs = if rest > 0 { rest } else { self.prefs.timer_default_seconds };
                    self.start_rest(secs, &context);
                    self.timer.linked_set = Some(set_id);
                    self.save_timer();
                    msg = trf!("Série registrada. Descanso de {} iniciado.", format_duration_s(secs));
                } else if !was_done {
                    if !self.timer.is_running() {
                        self.timer.status = TimerStatus::Idle;
                    }
                    self.timer.context = context;
                    self.timer.linked_set = Some(set_id);
                    self.timer.set_duration(if rest > 0 { rest * 1000 } else { self.timer.duration_ms });
                    self.save_timer();
                    self.refresh_timer();
                }
                if let Some(rec) = record {
                    msg = trf!("★ Novo recorde: {}. {}", rec, msg);
                }
                if let Some(b) = before {
                    self.toast_undo(msg, UndoOp::new(UndoKind::RecordSet { set_id, before: b }, tr("Registro da série")));
                } else {
                    self.toast(msg);
                }
                true
            }
            Err(e) => {
                self.toast(e.to_string());
                false
            }
        }
    }

    /// Para exercícios agrupados: (exercício seguinte, iniciar descanso?, nome do seguinte).
    fn group_after_set(&self, set_id: Id) -> Option<(Option<usize>, bool, String)> {
        let f = self.db.get_session_full(self.session_id?).ok()?;
        let idx = f.exercises.iter().position(|e| e.sets.iter().any(|s| s.id == set_id))?;
        let tags: Vec<String> = f.exercises.iter().map(|e| e.group_tag.clone()).collect();
        if tags[idx].is_empty() {
            return None;
        }
        let open: Vec<usize> = f.exercises.iter().map(|e| e.sets.iter().filter(|s| s.completed_at.is_none()).count()).collect();
        let a = crate::training::groups::after_set(&tags, &open, idx);
        let name = a.go_to.and_then(|k| f.exercises.get(k)).map(|e| e.performed_name.clone()).unwrap_or_default();
        Some((a.go_to, a.rest, name))
    }

    /// Verificação de recorde pessoal após registrar (implementada no grupo C).
    fn check_record(&mut self, set_id: Id) -> Option<String> {
        if !self.prefs.module_records {
            return None;
        }
        let kinds = self.db.records_for_set(set_id).ok()?;
        let text = kinds.iter().map(|k| k.label()).collect::<Vec<_>>().join(", ");
        self.with_bridge(|b| b.set_focus_record(ss(if text.is_empty() { String::new() } else { trf!("Novo recorde: {}", text) })));
        if text.is_empty() { None } else { Some(text) }
    }

    /// Valor para preenchimento rápido (3.3). Não registra nada.
    fn fill_values(&self, set_id: Id, source: &str) -> Option<(i64, i64)> {
        match source {
            "previous" => self.db.previous_set_values(set_id).ok().flatten(),
            "planned" => self.db.planned_set_values(set_id).ok(),
            "last" => self.db.last_execution_values(set_id).ok().flatten(),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Modo foco (3.1)
    // ------------------------------------------------------------------

    fn current_session_exercise(&self) -> Option<SessionExercise> {
        let sid = self.session_id?;
        let f = self.db.get_session_full(sid).ok()?;
        let i = self.with_bridge(|b| b.get_session_index()).max(0) as usize;
        f.exercises.get(i.min(f.exercises.len().saturating_sub(1))).cloned()
    }

    // ------------------------------------------------------------------
    // Calculadoras (evolução 8.5/8.6)
    // ------------------------------------------------------------------

    fn load_tools(&mut self) {
        use crate::domain::calculators::*;
        if self.tools.loaded {
            return;
        }
        let unit = self.prefs.unit;
        let pref = |k: &str| self.db.get_pref(k).ok().flatten();
        let plates = pref("plates.available").map(|v| parse_plates(&v)).filter(|v| !v.is_empty()).unwrap_or_else(|| default_plates(unit));
        let bar = pref("plates.bar_g").and_then(|v| v.parse().ok()).unwrap_or_else(|| default_bar(unit));
        let steps = parse_warmup(&pref("warmup.steps").unwrap_or_default());
        self.tools = ToolsState { loaded: true, target_g: bar + 40_000, bar_g: bar, work_g: 60_000, plates, steps };
    }

    fn save_tools(&mut self) {
        use crate::domain::calculators::*;
        let t = self.tools.clone();
        let r = self
            .db
            .set_pref("plates.available", &format_plates(&t.plates))
            .and_then(|_| self.db.set_pref("plates.bar_g", &t.bar_g.to_string()))
            .and_then(|_| self.db.set_pref("warmup.steps", &format_warmup(&t.steps)));
        self.report(r);
    }

    /// Abre as calculadoras; a partir da sessão, usa a carga da próxima série em aberto do exercício.
    fn open_tools(&mut self, se_id: Id) {
        self.load_tools();
        if se_id > 0 {
            if let Some(f) = self.session_id.and_then(|sid| self.db.get_session_full(sid).ok()) {
                if let Some(e) = f.exercises.iter().find(|e| e.id == se_id) {
                    let s = e.sets.iter().find(|s| s.completed_at.is_none()).or(e.sets.last());
                    if let Some(s) = s {
                        let g = s.actual_load_g.filter(|g| *g > 0).unwrap_or(s.planned_load_g);
                        if g > 0 {
                            self.tools.target_g = g.max(self.tools.bar_g);
                            self.tools.work_g = g;
                        }
                    }
                }
            }
        }
        self.navigate(P_TOOLS);
    }

    fn refresh_tools(&mut self) {
        use crate::domain::calculators::*;
        self.load_tools();
        let unit = self.prefs.unit;
        let t = self.tools.clone();
        let (result, detail, exact, heights) = match plates_for(t.target_g, t.bar_g, &t.plates) {
            Ok(r) => {
                let maxw = t.plates.iter().map(|p| p.weight_g).max().unwrap_or(1).max(1) as f32;
                let mut heights: Vec<f32> = Vec::new();
                for (w, n) in &r.per_side {
                    for _ in 0..*n {
                        heights.push(0.3 + 0.7 * (*w as f32 / maxw));
                    }
                }
                let side = if r.per_side.is_empty() {
                    tr("Só a barra").to_string()
                } else {
                    r.per_side
                        .iter()
                        .map(|(w, n)| if *n > 1 { format!("{} × {}", n, format_load_unit(*w, unit)) } else { format_load_unit(*w, unit) })
                        .collect::<Vec<_>>()
                        .join(" + ")
                };
                let detail = if r.exact {
                    trf!("Total montado: {} (barra {}).", format_load_unit(r.total_g, unit), format_load_unit(t.bar_g, unit))
                } else {
                    trf!(
                        "Não é possível montar exatamente {} com as anilhas cadastradas. Mais próximo abaixo: {}.",
                        format_load_unit(t.target_g, unit),
                        format_load_unit(r.total_g, unit)
                    )
                };
                (side, detail, r.exact, heights)
            }
            Err(e) => (e.to_string(), String::new(), false, vec![]),
        };
        let inventory: Vec<SharedString> = t.plates.iter().map(|p| ss(format_load_unit(p.weight_g, unit))).collect();
        let pairs: Vec<SharedString> = t.plates.iter().map(|p| ss(trf!("{} par(es)", p.pairs))).collect();
        let warm = warmup_sets(t.work_g, &t.steps, self.prefs.load_step_g, t.bar_g.min(t.work_g));
        let warm_lines: Vec<SharedString> = warm
            .iter()
            .enumerate()
            .map(|(i, (g, r))| ss(trf!("{}. {} × {} reps", i + 1, format_load_unit(*g, unit), r)))
            .chain(std::iter::once(ss(trf!("Trabalho: {}", format_load_unit(t.work_g, unit)))))
            .collect();
        let steps: Vec<SharedString> = t.steps.iter().map(|s| ss(trf!("{}% × {} reps", s.percent, s.reps))).collect();
        self.with_bridge(|b| {
            b.set_tools_target(ss(format_load_unit(t.target_g, unit)));
            b.set_tools_bar(ss(format_load_unit(t.bar_g, unit)));
            b.set_tools_result(ss(trf!("Cada lado: {}", result)));
            b.set_tools_detail(ss(detail));
            b.set_tools_exact(exact);
            b.set_tools_plates(model(heights));
            b.set_tools_inventory(model(inventory));
            b.set_tools_plate_labels(model(pairs));
            b.set_tools_work(ss(format_load_unit(t.work_g, unit)));
            b.set_tools_warmup(model(warm_lines));
            b.set_tools_steps(model(steps));
        });
    }

    fn tools_step(&mut self, field: &str, d: i32) {
        self.load_tools();
        let step = self.prefs.load_step_g.max(250);
        let v = match field {
            "bar" => &mut self.tools.bar_g,
            "work" => &mut self.tools.work_g,
            _ => &mut self.tools.target_g,
        };
        *v = (*v + d as i64 * step).clamp(0, MAX_LOAD_G);
        self.save_tools();
        self.refresh_tools();
    }

    /// Ilustração do exercício realizado (catálogo atual; se ele não existir mais, pelo nome).
    fn session_art(&self, e: &SessionExercise) -> &'static str {
        match e.performed_exercise_id.and_then(|id| self.db.get_exercise(id).ok()) {
            Some(x) => crate::exercise_media::resolve(&x),
            None => crate::exercise_media::guess("", &e.performed_name),
        }
    }

    fn first_open_set(e: &SessionExercise) -> usize {
        e.sets.iter().position(|s| s.completed_at.is_none()).unwrap_or(e.sets.len().saturating_sub(1))
    }

    fn open_focus(&mut self) {
        let Some(e) = self.current_session_exercise() else {
            self.toast(tr("Nenhuma sessão em andamento."));
            return;
        };
        self.focus_set = Self::first_open_set(&e);
        self.with_bridge(|b| b.set_focus_field(ss("load")));
        self.navigate(P_FOCUS);
    }

    /// Rascunho da série: o que o usuário digitou, senão o realizado, senão a sugestão.
    fn focus_draft(&self, set: &SessionSet) -> (String, String) {
        match self.drafts.get(&set.id) {
            Some(d) => d.clone(),
            None => self.focus_base(set),
        }
    }

    /// Valor inicial sem rascunho: realizado (se registrada) ou sugestão (última vez / planejado).
    fn focus_base(&self, set: &SessionSet) -> (String, String) {
        let unit = self.prefs.unit;
        if set.completed_at.is_some() {
            return (
                set.actual_load_g.map(|g| format_load(g, unit)).unwrap_or_default(),
                set.actual_reps.map(|r| r.to_string()).unwrap_or_default(),
            );
        }
        let base = if self.prefs.prefill_last { self.db.last_execution_values(set.id).ok().flatten() } else { None };
        let (l, r) = base.unwrap_or((set.planned_load_g, set.planned_reps_max));
        (if l > 0 { format_load(l, unit) } else { String::new() }, r.to_string())
    }

    fn refresh_focus(&mut self) {
        self.focus_fresh = true;
        let Some(e) = self.current_session_exercise() else { return };
        let art = self.session_art(&e);
        if e.sets.is_empty() {
            self.with_bridge(|b| {
                b.set_focus_exercise(ss(&e.performed_name));
            b.set_focus_art(ss(art));
                b.set_focus_set_label(ss(tr("Sem séries — adicione na tela da sessão")));
            });
            return;
        }
        let n = e.sets.len();
        self.focus_set = self.focus_set.min(n - 1);
        let set = e.sets[self.focus_set].clone();
        let unit = self.prefs.unit;
        let (load, reps) = self.focus_draft(&set);
        let idx = self.with_bridge(|b| b.get_session_index());
        let count = self.with_bridge(|b| b.get_session_exercises().row_count()) as i32;
        let planned = trf!(
            "Planejado: {}{} reps",
            if set.planned_load_g > 0 { trf!("{} × ", format_load_unit(set.planned_load_g, unit)) } else { String::new() },
            format_reps(set.planned_reps_min, set.planned_reps_max)
        );
        let hint = match self.db.last_execution_values(set.id).ok().flatten() {
            Some((l, r)) => trf!("Última vez: {} × {}", format_load_unit(l, unit), r),
            None => String::new(),
        };
        let meta = [e.muscle_focus.as_str(), e.exercise_type.as_str(), e.equipment.as_str()]
            .iter()
            .filter(|x| !x.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let done_text = match (set.actual_load_g, set.actual_reps) {
            (Some(l), Some(r)) => trf!("{} × {}", format_load_unit(l, unit), r),
            _ => String::new(),
        };
        let has_prev = self.db.previous_set_values(set.id).ok().flatten().is_some();
        let has_last = !hint.is_empty();
        let step = format_load(self.prefs.load_step_g, unit);
        let fs = self.focus_set;
        self.with_bridge(|b| {
            b.set_focus_exercise(ss(&e.performed_name));
            b.set_focus_art(ss(art));
            b.set_focus_meta(ss(meta));
            b.set_focus_set_label(ss(trf!("Série {} de {}", set.set_number, n)));
            b.set_focus_planned(ss(planned));
            b.set_focus_load(ss(&load));
            b.set_focus_reps(ss(&reps));
            b.set_focus_done(set.completed_at.is_some());
            b.set_focus_done_text(ss(done_text));
            b.set_focus_hint(ss(hint));
            b.set_focus_step_label(ss(step));
            b.set_focus_has_previous(has_prev);
            b.set_focus_has_last(has_last);
            b.set_focus_can_prev_set(fs > 0);
            b.set_focus_can_next_set(fs + 1 < n);
            b.set_focus_can_prev_ex(idx > 0);
            b.set_focus_can_next_ex(idx + 1 < count);
            b.set_session_unit(ss(unit.as_str()));
        });
    }

    fn focus_set_id(&self) -> Option<Id> {
        let e = self.current_session_exercise()?;
        e.sets.get(self.focus_set).map(|s| s.id)
    }

    /// Guarda o rascunho atual (antes de navegar) — a navegação nunca descarta dados.
    fn focus_store_draft(&mut self) {
        if let Some(id) = self.focus_set_id() {
            let (l, r) = self.with_bridge(|b| (b.get_focus_load().to_string(), b.get_focus_reps().to_string()));
            let set = self.db.get_session_set(id).ok();
            let unchanged = set.as_ref().map(|s| self.focus_base(s) == (l.clone(), r.clone())).unwrap_or(false);
            if !unchanged {
                self.drafts.insert(id, (l, r));
            }
        }
    }

    fn focus_edit(&mut self, f: impl FnOnce(&str, FieldKind) -> String) {
        let field = self.with_bridge(|b| b.get_focus_field().to_string());
        let kind = if field == "reps" { FieldKind::Reps } else { FieldKind::Load };
        let cur = self.with_bridge(|b| if field == "reps" { b.get_focus_reps() } else { b.get_focus_load() }).to_string();
        let next = f(&cur, kind);
        self.with_bridge(|b| if field == "reps" { b.set_focus_reps(ss(&next)) } else { b.set_focus_load(ss(&next)) });
        if let Some(id) = self.focus_set_id() {
            let (l, r) = self.with_bridge(|b| (b.get_focus_load().to_string(), b.get_focus_reps().to_string()));
            self.drafts.insert(id, (l, r));
        }
    }

    fn focus_nav(&mut self, what: &str) {
        let idx = self.with_bridge(|b| b.get_session_index());
        let count = self.with_bridge(|b| b.get_session_exercises().row_count()) as i32;
        match what {
            "prev-set" => self.focus_set = self.focus_set.saturating_sub(1),
            "next-set" => self.focus_set += 1,
            "prev-ex" | "next-ex" => {
                let ni = if what == "prev-ex" { idx - 1 } else { idx + 1 };
                if ni < 0 || ni >= count {
                    return;
                }
                self.with_bridge(|b| b.set_session_index(ni));
                if let Some(e) = self.current_session_exercise() {
                    self.focus_set = Self::first_open_set(&e);
                }
                self.with_bridge(|b| b.set_nav_dir(if what == "prev-ex" { -1 } else { 1 }));
            }
            _ => {}
        }
        self.refresh_focus();
    }

    fn focus_confirm(&mut self) {
        let Some(id) = self.focus_set_id() else { return };
        let (l, r) = self.with_bridge(|b| (b.get_focus_load().to_string(), b.get_focus_reps().to_string()));
        match input::to_values(&l, &r, self.prefs.unit) {
            Ok((load_g, reps)) => {
                let was_done = self.db.get_session_set(id).map(|s| s.completed_at.is_some()).unwrap_or(false);
                if self.record_set_values(id, load_g, reps) && !was_done {
                    // Avança para a próxima série não registrada do exercício (sem registrar nada).
                    if let Some(e) = self.current_session_exercise() {
                        if let Some(next) = e.sets.iter().position(|s| s.completed_at.is_none()) {
                            self.focus_set = next;
                        }
                    }
                }
                self.refresh_focus();
            }
            Err(e) => self.toast(e),
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
            trf!("{} (original: {})", e.performed_name, e.original_name)
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
            self.toast(tr("Escolha o exercício substituto."));
            return;
        }
        match self.db.substitute_exercise(se, id, permanent) {
            Ok(()) => {
                self.close_dialog();
                let r = self.refresh_session();
                self.report(r);
                self.toast(if permanent {
                    tr("Substituído. A ficha foi atualizada para as próximas sessões.")
                } else {
                    tr("Substituído somente nesta sessão. A ficha não mudou.")
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
                self.toast(tr("Sessão finalizada e salva no histórico."));
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

    /// Grava o descanso cumprido na série ligada ao cronômetro (se houver) e desfaz a ligação.
    fn finalize_rest(&mut self) {
        let Some(set) = self.timer.linked_set else { return };
        if self.timer.phase != TimerPhase::Rest || self.timer.status == TimerStatus::Idle {
            return;
        }
        self.timer.linked_set = None;
        let secs = self.timer.elapsed(Clock::now()) / 1000;
        if secs >= 1 {
            if let Err(e) = self.db.set_rest_taken(set, secs) {
                log::warn!("descanso não gravado: {e}");
            }
        }
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
                tr("Alerta com a tela bloqueada"),
                tr("Para avisar o fim do descanso quando o app estiver em segundo plano ou com a tela bloqueada, o EMPETO precisa enviar notificações. Sem essa permissão, o cronômetro continua funcionando normalmente na tela (e a vibração, se ativada)."),
                tr("Permitir"),
                tr("Agora não"),
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
                    self.finalize_rest();
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
                        self.toast(tr("Descanso encerrado."));
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
                        self.toast(tr("Hora de iniciar a série."));
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
            TimerPhase::Rest => tr("Descanso"),
            TimerPhase::Prepare => tr("Preparação — próxima série"),
        };
        let notice = if self.platform.is_android() && self.prefs.background_alert && !self.platform.notifications_allowed() {
            tr("Notificações desativadas: com o app em segundo plano o alerta pode não aparecer. O cronômetro continua correto ao voltar.")
        } else if self.platform.is_android() && !self.prefs.background_alert {
            tr("Alerta em segundo plano desativado nas preferências.")
        } else {
            ""
        };
        let display = t.display(now);
        let progress = t.progress(now);
        let ring = ring_path(if t.status == TimerStatus::Idle { 1.0 } else { 1.0 - progress });
        let ctx = t.context.clone();
        let duration = format_duration_s(t.duration_ms / 1000);
        let step = trf!("{} s", self.prefs.timer_adjust_step);
        self.with_bridge(|b| {
            b.set_timer_display(ss(&display));
            b.set_timer_status(ss(status));
            b.set_timer_progress(progress);
            b.set_timer_ring(ss(&ring));
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
                TimerPhase::Rest => tr("Descanso encerrado"),
                TimerPhase::Prepare => tr("Hora de iniciar a série"),
            };
            let text = if self.timer.context.is_empty() { tr("Toque para abrir o EMPETO").to_string() } else { self.timer.context.clone() };
            self.platform.schedule_alarm(ALARM_END, end_boot, title, &text, self.prefs.sound_enabled, self.prefs.vibrate);
            if self.timer.pre_alert_ms > 0 && rem > self.timer.pre_alert_ms + 1000 {
                self.platform.schedule_alarm(
                    ALARM_PRE,
                    end_boot - self.timer.pre_alert_ms,
                    &trf!("Faltam {} s de descanso", self.timer.pre_alert_ms / 1000),
                    &text,
                    self.prefs.sound_enabled,
                    self.prefs.vibrate,
                );
            }
            self.platform.show_countdown(now.wall_ms + rem, tr("Descanso em andamento"), &text);
        }
    }

    fn on_resume(&mut self) {
        self.apply_appearance();
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
                self.toast(tr("O descanso terminou enquanto o app estava em segundo plano."));
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
        let mut vol: HashMap<NaiveDate, i64> = HashMap::new();
        for s in &sessions {
            *vol.entry(local_date_of(s.started_at)).or_default() += s.volume_g;
        }
        let max_vol = vol.values().copied().max().unwrap_or(0).max(1);
        let unit = self.prefs.unit;
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
                heat: vol.get(d).map(|v| *v as f32 / max_vol as f32).unwrap_or(0.0),
                heat_text: ss(match vol.get(d) {
                    Some(v) => trf!("{} sessão(ões), volume {}", done.len(), fmt_volume(*v, unit)),
                    None => String::new(),
                }),
            });
            self.cal_cache.push((*d, planned, done));
        }
        let title = if week_view {
            format!("{} – {}", fmt_dm(dates[0]), fmt_dm(dates[6]))
        } else {
            cal::month_title(self.cal_month.year(), self.cal_month.month())
        };
        let labels: Vec<SharedString> = dates.iter().take(7).map(|d| ss(tr(WEEKDAY_SHORT[cal::weekday_index(*d) as usize]))).collect();
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
                        subtitle: ss(trf!(
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
                    subtitle: ss(trf!(
                        "{} exercício(s){}",
                        p.exercises,
                        if p.from_sequence { tr(" · próximo da sequência") } else { "" }
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

    /// Abre o app de agenda do aparelho com o evento preenchido (intent; sem permissão de calendário).
    fn cal_add_to_agenda(&mut self, i: usize) {
        let entries = self.with_bridge(|b| b.get_cal_entries());
        let Some(e) = entries.row_data(i) else { return };
        let (begin, end, all_day) = if e.kind == "done" {
            match self.db.get_session(e.session_id as Id) {
                Ok(s) => (s.started_at, s.ended_at.unwrap_or(s.started_at + 3_600_000), false),
                Err(_) => return,
            }
        } else {
            let (a, b) = cal::day_bounds_ms(self.cal_selected);
            (a, b, true)
        };
        let title = if e.kind == "done" { trf!("Treino realizado: {}", e.title) } else { trf!("Treino: {}", e.title) };
        if !self.platform.add_calendar_event(&title, &e.subtitle, begin, end, all_day) {
            self.toast(tr("Nenhum app de agenda encontrado no aparelho."));
        }
    }

    fn refresh_search(&mut self) {
        let q = self.with_bridge(|b| b.get_search_query().to_string());
        self.search_hits = crate::application::search::search(&self.db, &q).unwrap_or_default();
        let rows: Vec<SearchRow> = self
            .search_hits
            .iter()
            .map(|h| SearchRow { kind: ss(h.kind.label()), id: h.id as i32, title: ss(&h.title), subtitle: ss(&h.subtitle) })
            .collect();
        self.with_bridge(|b| b.set_search_results(model(rows)));
    }

    fn show_diagnostics(&mut self) {
        let dir = self.platform.data_dir();
        let trace = std::fs::read_to_string(dir.join("startup.log")).unwrap_or_else(|_| "(sem registro)".into());
        let integrity = if self.db.integrity_ok().unwrap_or(false) { "ok" } else { "FALHOU" };
        let text = trf!(
            "Versão do app: {}\nSchema do banco: {}\nIntegridade do banco: {}\nNotificações permitidas: {}\nCronômetro: {}\n\nEtapas da última abertura:\n{}",
            self.platform.app_version(),
            schema_version(&self.db.conn).unwrap_or(0),
            integrity,
            if self.platform.notifications_allowed() { tr("sim") } else { tr("não") },
            format!("{:?}", self.timer.status),
            trace.lines().rev().take(25).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
        );
        self.show_info(InfoCtx::Plain, tr("Diagnóstico"), &text, tr("OK"), "");
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
        let total = self.db.count_completed_sessions()?;
        let list = self.db.list_session_summaries_limit(None, None, Some(self.hist_limit))?;
        let rows: Vec<HistRow> = list
            .iter()
            .map(|s| HistRow {
                id: s.id as i32,
                date: ss(capitalize(&fmt_date_time(s.started_at))),
                title: ss(format!("{} — {}", s.plan_name, s.schedule_name)),
                subtitle: ss(trf!(
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
                detail: ss(trf!("{} sessão(ões) · última em {}", h.sessions, fmt_date_short(h.last_at))),
            })
            .collect();
        let summary = if list.is_empty() { String::new() } else { trf!("{} sessão(ões) concluída(s)", total) };
        let more = (list.len() as i64) < total;
        self.with_bridge(|b| {
            b.set_hist_has_more(more);
            b.set_hist_rows(model(rows));
            b.set_evo_exercises(model(evo));
            b.set_hist_summary(ss(summary));
        });
        match self.with_bridge(|b| b.get_hist_tab()) {
            2 => self.refresh_report()?,
            3 => self.refresh_diary()?,
            _ => {}
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Análise (evolução 8.7–8.9)
    // ------------------------------------------------------------------

    fn refresh_report(&mut self) -> AppResult<()> {
        let unit = self.prefs.unit;
        let (y, m) = self.rep_month;
        let r = self.db.month_report(y, m)?;
        let title = capitalize(&cal::month_title(y, m));
        let row = |label: &str, value: String| CmpRow { label: ss(label), a: ss(value), b: ss(""), delta: ss(""), trend: 0 };
        let mut rows = vec![
            row(tr("Treinos realizados"), r.sessions.to_string()),
            row(tr("Dias com treino"), r.days_trained.to_string()),
            row(tr("Duração total"), fmt_minutes(r.total_duration_ms)),
            row(tr("Duração média"), fmt_minutes(r.avg_duration_ms)),
            row(tr("Exercícios diferentes"), r.distinct_exercises.to_string()),
            row(tr("Séries registradas"), r.sets.to_string()),
            row(tr("Volume total"), fmt_volume(r.volume_g, unit)),
            row(tr("Recordes pessoais"), r.records.len().to_string()),
        ];
        if let Some(rpe) = r.avg_rpe {
            rows.push(row(tr("RPE médio"), fmt_rpe(rpe)));
        }
        if let Some(f) = r.avg_feeling {
            rows.push(row(tr("Avaliação média"), format!("{:.1} / 5", f).replace('.', &crate::i18n::decimal_sep().to_string())));
        }
        let wmax = r.weeks.iter().map(|w| w.1).max().unwrap_or(1).max(1) as f32;
        let weeks: Vec<BarRow> = r
            .weeks
            .iter()
            .map(|(d, n)| BarRow { label: ss(trf!("Semana de {}", fmt_dm(*d))), value: ss(n.to_string()), frac: *n as f32 / wmax })
            .collect();
        let mmax = r.muscles.iter().map(|m| m.1).max().unwrap_or(1).max(1) as f32;
        let muscles: Vec<BarRow> =
            r.muscles.iter().map(|(m, n)| BarRow { label: ss(m), value: ss(trf!("{} série(s)", n)), frac: *n as f32 / mmax }).collect();
        let records: Vec<SharedString> = r
            .records
            .iter()
            .take(10)
            .map(|e| {
                ss(trf!(
                    "{} — {} ({} × {}, {})",
                    e.exercise_name,
                    e.kinds.iter().map(|k| k.label()).collect::<Vec<_>>().join(", "),
                    format_load_unit(e.load_g, unit),
                    e.reps,
                    fmt_date_short(e.at_ms)
                ))
            })
            .collect();
        let empty = r.sessions == 0;
        self.with_bridge(|b| {
            b.set_rep_title(ss(title));
            b.set_rep_rows(model(rows));
            b.set_rep_weeks(model(weeks));
            b.set_rep_muscles(model(muscles));
            b.set_rep_records(model(records));
            b.set_rep_empty(empty);
        });
        Ok(())
    }

    /// Texto do relatório para compartilhar (opcional; o usuário escolhe o app no seletor do sistema).
    fn report_text(&self) -> String {
        let mut out = format!("EMPETO — {}\n", self.with_bridge(|b| b.get_rep_title()));
        self.with_bridge(|b| {
            for r in b.get_rep_rows().iter() {
                out.push_str(&format!("• {}: {}\n", r.label, r.a));
            }
            let m = b.get_rep_muscles();
            if m.row_count() > 0 {
                out.push_str(&format!("\n{}\n", tr("Séries por grupo muscular")));
                for r in m.iter() {
                    out.push_str(&format!("• {}: {}\n", r.label, r.value));
                }
            }
            let rec = b.get_rep_records();
            if rec.row_count() > 0 {
                out.push_str(&format!("\n{}\n", tr("Recordes do mês")));
                for r in rec.iter() {
                    out.push_str(&format!("★ {}\n", r));
                }
            }
        });
        out
    }

    fn refresh_diary(&mut self) -> AppResult<()> {
        let rows: Vec<DiaryRow> = self
            .db
            .diary(200)?
            .into_iter()
            .map(|d| {
                let mut text = d.notes.clone();
                for (ex, n) in &d.exercise_notes {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&format!("{}: {}", ex, n));
                }
                DiaryRow {
                    id: d.session_id as i32,
                    date: ss(capitalize(&fmt_date_time(d.started_at))),
                    title: ss(&d.title),
                    feeling: ss(d.feeling.map(crate::history::analytics::feeling_label).unwrap_or("")),
                    text: ss(text),
                }
            })
            .collect();
        self.with_bridge(|b| b.set_diary(model(rows)));
        Ok(())
    }

    fn detail_compare(&mut self) {
        let Some(id) = self.detail_id else { return };
        let list = self.db.comparable_sessions(id).unwrap_or_default();
        if list.is_empty() {
            self.toast(tr("Ainda não há outra sessão concluída para comparar."));
            return;
        }
        let ids: Vec<Id> = list.iter().map(|s| s.id).collect();
        let opts: Vec<String> = list
            .iter()
            .map(|s| format!("{} · {} — {}", fmt_date_short(s.started_at), s.plan_name, s.schedule_name))
            .collect();
        self.ask_choice(
            ChoiceCtx::Compare(ids),
            tr("Comparar com"),
            tr("Sessões do mesmo dia da ficha aparecem primeiro."),
            opts,
            0,
        );
    }

    fn refresh_compare(&mut self) -> AppResult<()> {
        let Some((a, b)) = self.cmp else { return Ok(()) };
        let unit = self.prefs.unit;
        let sa = self.db.session_stats(a)?;
        let sb = self.db.session_stats(b)?;
        fn delta_i(a: i64, b: i64, f: &dyn Fn(i64) -> String) -> (String, i32) {
            let d = b - a;
            if d == 0 {
                ("=".into(), 0)
            } else if d > 0 {
                (format!("↑ {}", f(d)), 1)
            } else {
                (format!("↓ {}", f(-d)), -1)
            }
        }
        let mk = |label: &str, av: String, bv: String, (delta, trend): (String, i32)| CmpRow {
            label: ss(label),
            a: ss(av),
            b: ss(bv),
            delta: ss(delta),
            trend,
        };
        let load = |g: i64| format_load_unit(g, unit);
        let vol = |g: i64| fmt_volume(g, unit);
        let num = |n: i64| n.to_string();
        let mins = |ms: i64| fmt_minutes(ms);
        let mut rows = vec![
            mk(tr("Data"), fmt_date_short(sa.started_at), fmt_date_short(sb.started_at), (String::new(), 0)),
            mk(
                tr("Duração"),
                sa.duration_ms.map(mins).unwrap_or_default(),
                sb.duration_ms.map(mins).unwrap_or_default(),
                match (sa.duration_ms, sb.duration_ms) {
                    (Some(x), Some(y)) => {
                        let (d, _) = delta_i(x / 60_000, y / 60_000, &|m| format!("{m} min"));
                        (d, 0)
                    }
                    _ => (String::new(), 0),
                },
            ),
            mk(tr("Séries"), num(sa.sets), num(sb.sets), delta_i(sa.sets, sb.sets, &num)),
            mk(tr("Volume"), vol(sa.volume_g), vol(sb.volume_g), delta_i(sa.volume_g, sb.volume_g, &vol)),
        ];
        if sa.avg_rpe.is_some() || sb.avg_rpe.is_some() {
            rows.push(mk(tr("RPE médio"), sa.avg_rpe.map(fmt_rpe).unwrap_or_default(), sb.avg_rpe.map(fmt_rpe).unwrap_or_default(), (String::new(), 0)));
        }
        let mut keys: Vec<String> = sb.exercises.iter().map(|e| e.key.clone()).collect();
        for e in &sa.exercises {
            if !keys.contains(&e.key) {
                keys.push(e.key.clone());
            }
        }
        let empty = crate::history::analytics::ExerciseStats {
            key: String::new(),
            name: String::new(),
            sets: 0,
            max_load_g: 0,
            total_reps: 0,
            volume_g: 0,
            e1rm_g: 0,
            avg_rpe: None,
        };
        let exercises: Vec<CmpEx> = keys
            .iter()
            .map(|k| {
                let ea = sa.exercises.iter().find(|e| &e.key == k).unwrap_or(&empty);
                let eb = sb.exercises.iter().find(|e| &e.key == k).unwrap_or(&empty);
                let name = if eb.name.is_empty() { ea.name.clone() } else { eb.name.clone() };
                CmpEx {
                    name: ss(name),
                    rows: model(vec![
                        mk(tr("Séries"), num(ea.sets), num(eb.sets), delta_i(ea.sets, eb.sets, &num)),
                        mk(tr("Carga máx."), load(ea.max_load_g), load(eb.max_load_g), delta_i(ea.max_load_g, eb.max_load_g, &load)),
                        mk(tr("Repetições"), num(ea.total_reps), num(eb.total_reps), delta_i(ea.total_reps, eb.total_reps, &num)),
                        mk(tr("Volume"), vol(ea.volume_g), vol(eb.volume_g), delta_i(ea.volume_g, eb.volume_g, &vol)),
                        mk(tr("1RM est."), load(ea.e1rm_g), load(eb.e1rm_g), delta_i(ea.e1rm_g, eb.e1rm_g, &load)),
                    ]),
                }
            })
            .collect();
        self.with_bridge(|bb| {
            bb.set_cmp_a_title(ss(format!("{} · {}", fmt_date_short(sa.started_at), sa.title)));
            bb.set_cmp_b_title(ss(format!("{} · {}", fmt_date_short(sb.started_at), sb.title)));
            bb.set_cmp_rows(model(rows));
            bb.set_cmp_exercises(model(exercises));
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
                    SubstitutionType::Temporary => trf!("Substituiu {} (só nesta sessão)", e.original_name),
                    SubstitutionType::Permanent => trf!("Substituiu {} (permanente)", e.original_name),
                };
                let mut lines: Vec<SharedString> = e
                    .sets
                    .iter()
                    .map(|s| {
                        let actual = match (s.actual_load_g, s.actual_reps) {
                            (Some(l), Some(r)) => trf!("{} × {}", format_load_unit(l, unit), r),
                            _ => tr("não registrada").to_string(),
                        };
                        let rest = match s.rest_taken_s {
                            Some(secs) => trf!(" · descanso {}", format_duration_s(secs)),
                            None => interval
                                .get(&s.id)
                                .map(|secs| trf!(" · intervalo {}", format_duration_s(*secs)))
                                .unwrap_or_default(),
                        };
                        ss(trf!(
                            "Série {}{}: {}  (plan. {}{}){}",
                            s.set_number,
                            if s.kind == SetKind::Normal { String::new() } else { format!(" [{}]", s.kind.label()) },
                            format!(
                                "{}{}{}",
                                actual,
                                s.rpe.map(|r| format!(" · RPE {}", fmt_rpe(r))).unwrap_or_default(),
                                s.rir.map(|r| format!(" · RIR {}", r)).unwrap_or_default()
                            ),
                            if s.planned_load_g > 0 { trf!("{} × ", format_load_unit(s.planned_load_g, unit)) } else { String::new() },
                            format_reps(s.planned_reps_min, s.planned_reps_max),
                            rest
                        ))
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push(ss(tr("Não realizado nesta sessão.")));
                }
                DetailEx {
                    title: ss(&e.performed_name),
                    subtitle: ss(format!(
                        "{}{}{}",
                        e.muscle_focus,
                        if e.exercise_type.is_empty() { String::new() } else { format!(" · {}", e.exercise_type) },
                        if e.rest_seconds > 0 { trf!(" · descanso planejado {}", format_duration_s(e.rest_seconds)) } else { String::new() }
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
            None => trf!("{} · em andamento", capitalize(&cal::date_long(local_date_of(f.session.started_at)))),
        };
        let mut summary = trf!(
            "{} série(s) · volume {}",
            f.completed_sets(),
            fmt_volume(f.volume_g(), unit)
        );
        if let Some(fl) = f.session.feeling {
            summary.push_str(&trf!(" · treino: {}", crate::history::analytics::feeling_label(fl)));
        }
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
                EvolutionMetric::BestReps => trf!("{} reps", v),
                EvolutionMetric::Volume => fmt_volume(v, unit),
                _ => format_load_unit(v, unit),
            }
        };
        let values: Vec<i64> = pts.iter().map(|p| p.value(metric)).collect();
        let (mn, mx) = (values.iter().copied().min().unwrap_or(0), values.iter().copied().max().unwrap_or(0));
        // Eixo vertical a partir de 0 (não distorce pequenas variações); x pela ordem das sessões.
        let top = (mx as f64 * 1.1).max(1.0);
        let n = values.len();
        let norm: Vec<(f64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = if n <= 1 { 0.5 } else { 0.04 + 0.92 * i as f64 / (n - 1) as f64 };
                (x, 1.0 - *v as f64 / top)
            })
            .collect();
        let (w, h) = (self.evo_chart.0 as f64, self.evo_chart.1 as f64);
        let path = if n >= 2 {
            norm.iter()
                .enumerate()
                .map(|(i, (x, y))| format!("{} {:.1} {:.1}", if i == 0 { "M" } else { "L" }, x * w, y * h))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            String::new()
        };
        // Tendência (regressão linear) só com dados suficientes.
        let trend = if n >= 3 {
            let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let ys: Vec<f64> = values.iter().map(|v| *v as f64).collect();
            let (mx_, my) = (xs.iter().sum::<f64>() / n as f64, ys.iter().sum::<f64>() / n as f64);
            let num: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx_) * (y - my)).sum();
            let den: f64 = xs.iter().map(|x| (x - mx_).powi(2)).sum();
            let slope = if den > 0.0 { num / den } else { 0.0 };
            let at = |i: f64| (my + slope * (i - mx_)).clamp(0.0, top);
            let (y0, y1) = (1.0 - at(0.0) / top, 1.0 - at((n - 1) as f64) / top);
            format!("M {:.1} {:.1} L {:.1} {:.1}", 0.04 * w, y0 * h, 0.96 * w, y1 * h)
        } else {
            String::new()
        };
        let metric_name = [tr("Carga máxima"), tr("Volume"), tr("1RM estimada"), tr("Repetições")][self.evo_metric.clamp(0, 3) as usize];
        let points: Vec<EvoPt> = norm
            .iter()
            .zip(&pts)
            .map(|((x, y), p)| EvoPt {
                x: *x as f32,
                y: *y as f32,
                label: ss(format!("{} · {} · {} · {}", fmt_date_short(p.started_at), fmt_v(p.value(metric)), name, metric_name)),
            })
            .collect();
        let sel = if self.evo_selected >= 0 && (self.evo_selected as usize) < points.len() { self.evo_selected } else { -1 };
        let sel_label = if sel >= 0 { points[sel as usize].label.to_string() } else { String::new() };
        let bests = self.db.current_bests(&self.evo_key)?;
        let bests_text = if bests.count > 0 {
            trf!(
                "Maior carga: {} · Mais repetições: {} · Maior volume em uma série: {} · 1RM estimada: {}",
                format_load_unit(bests.load_g, unit),
                bests.reps,
                fmt_volume(bests.set_volume, unit),
                format_load_unit(bests.e1rm_g, unit)
            )
        } else {
            String::new()
        };
        let records: Vec<SharedString> = self
            .db
            .recent_records(0, Some(&self.evo_key))?
            .iter()
            .take(8)
            .map(|r| {
                ss(trf!(
                    "{} — {} ({} × {})",
                    fmt_date_short(r.at_ms),
                    r.kinds.iter().map(|k| k.label()).collect::<Vec<_>>().join(", "),
                    format_load_unit(r.load_g, unit),
                    r.reps
                ))
            })
            .collect();
        let axis_max = fmt_v(top as i64);
        let summary = if values.len() >= 2 {
            let first = values[0];
            let last = *values.last().unwrap();
            let diff = last - first;
            let pct = if first > 0 { format!(" ({:+.0}%)", diff as f64 * 100.0 / first as f64) } else { String::new() };
            trf!(
                "{} sessão(ões) no período. Variação: {}{}{}",
                values.len(),
                if diff >= 0 { "+" } else { "−" },
                fmt_v(diff.abs()),
                pct
            )
        } else if values.len() == 1 {
            tr("1 sessão no período.").to_string()
        } else {
            String::new()
        };
        let rows: Vec<EvoRow> = pts
            .iter()
            .rev()
            .map(|p| EvoRow {
                date: ss(fmt_date_short(p.started_at)),
                value: ss(fmt_v(p.value(metric))),
                detail: ss(trf!("{} série(s) · melhor {} reps", p.sets, p.best_reps)),
            })
            .collect();
        self.with_bridge(|b| {
            b.set_evo_name(ss(&name));
            b.set_evo_period(self.evo_period);
            b.set_evo_metric(self.evo_metric);
            b.set_evo_has_data(!values.is_empty());
            b.set_evo_path(ss(&path));
            b.set_evo_trend(ss(&trend));
            b.set_evo_points(model(points));
            b.set_evo_selected(sel);
            b.set_evo_selected_label(ss(&sel_label));
            b.set_evo_bests(ss(&bests_text));
            b.set_evo_records(model(records));
            b.set_evo_axis_max(ss(&axis_max));
            b.set_evo_min(ss(trf!("mín. {}", fmt_v(mn))));
            b.set_evo_max(ss(trf!("máx. {}", fmt_v(mx))));
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
        let notif = if !self.platform.is_android() {
            tr("Em segundo plano, o alerta usa uma notificação do sistema.").to_string()
        } else if self.platform.notifications_allowed() {
            tr("Notificações permitidas. O Android pode atrasar o alerta alguns segundos em modo de economia de energia.").to_string()
        } else {
            tr("Notificações não permitidas: em segundo plano, apenas a vibração (se ativada) pode ocorrer. O cronômetro continua correto na tela.").to_string()
        };
        let data = trf!(
            "{} ficha(s) · {} exercício(s) na biblioteca · {} sessão(ões) · {} série(s) registradas. Tudo fica no armazenamento privado do app (sem internet, sem nuvem).",
            counts.plans,
            counts.exercises,
            counts.sessions,
            counts.sets,
        );
        let data_dir = self.platform.data_dir();
        let rep = storage::storage_report(&data_dir, self.db.path.as_deref(), &self.platform.tmp_dir());
        let backups = storage::list_local_backups(&data_dir);
        let auto_on = self.db.get_pref(storage::PREF_AUTO)?.as_deref() != Some("0");
        let last_auto = backups.iter().filter(|b| b.automatic && b.valid).map(|b| b.created_at).max();
        let last_ext = self.db.get_pref(storage::PREF_LAST_EXTERNAL)?.and_then(|v| v.parse::<i64>().ok());
        let backups_info = trf!(
            "{} backup(s) no aparelho · {}\nÚltimo automático: {}\nÚltima cópia que você salvou fora do app: {}\n\nOs backups ficam na área privada do app: são apagados se o app for desinstalado.",
            backups.len(),
            storage::fmt_bytes(rep.backups_bytes),
            last_auto.map(fmt_date_time).unwrap_or_else(|| tr("nenhum ainda").into()),
            last_ext.map(fmt_date_time).unwrap_or_else(|| tr("nenhuma registrada").into()),
        );
        let backup_rows: Vec<BackupRow> = backups
            .iter()
            .enumerate()
            .map(|(i, b)| BackupRow {
                index: i as i32,
                title: ss(fmt_date_time(b.created_at)),
                subtitle: ss(if b.valid {
                    trf!("{} · {} ficha(s) · {} sessão(ões){}", storage::fmt_bytes(b.bytes), b.plans, b.sessions, if b.encrypted { tr(" · com senha") } else { "" })
                } else {
                    trf!("{} · arquivo ilegível (não pode ser restaurado)", storage::fmt_bytes(b.bytes))
                }),
                automatic: b.automatic,
                valid: b.valid,
            })
            .collect();
        let sc = self.home_shortcuts();
        let slots: Vec<SharedString> = (0..shortcuts::MAX_SHORTCUTS)
            .map(|i| ss(sc.get(i).map(|c| shortcuts::label(c)).unwrap_or(tr("Nenhum"))))
            .collect();
        self.with_bridge(|b| {
            b.set_storage_db(ss(storage::fmt_bytes(rep.db_bytes)));
            b.set_storage_backups(ss(trf!("{} · {} arquivo(s)", storage::fmt_bytes(rep.backups_bytes), rep.backups_count)));
            b.set_storage_temp(ss(storage::fmt_bytes(rep.temp_bytes)));
            b.set_storage_total(ss(storage::fmt_bytes(rep.total())));
            b.set_set_auto_backup(auto_on);
            b.set_backups_info(ss(&backups_info));
            b.set_local_backups(model(backup_rows));
            b.set_shortcut_slots(model(slots));
            b.set_set_mod_records(p.module_records);
            b.set_set_effort(p.effort);
            b.set_effort_enabled(p.effort);
            b.set_set_mod_heatmap(p.module_heatmap);
            b.set_set_mod_progression(p.module_progression);
            b.set_set_mod_gestures(p.module_gestures);
            b.set_set_mod_decorative(p.module_decorative_anim);
            b.set_set_mod_backup_reminder(p.module_backup_reminder);
        });
        let version = self.platform.app_version();
        let schema = schema_version(&self.db.conn).unwrap_or(0);
        let about = format!(
            "Versão do app: {version}\nVersão do banco de dados (schema): {schema}\n\n\
             Aplicativo pessoal e offline: não usa internet, não tem conta, anúncios nem sincronização. \
             Os dados ficam somente neste aparelho — faça backups periódicos.\n\n\
             O EMPETO é um organizador de treinos. Não é ferramenta de diagnóstico, prescrição ou recomendação médica \
             e não substitui orientação de profissional de educação física ou saúde.\n\n\
             Alternativas de exercícios são sugestões organizacionais e não indicam equivalência biomecânica.\n\n\
             A precisão dos alertas em segundo plano depende das políticas e permissões do Android.\n\n\
             Permissões usadas: notificações (alerta do cronômetro, opcional) e vibração.\n\n\
             Feito com Rust, Slint e SQLite. Licenças de terceiros: veja o repositório do projeto."
        );
        let step = format_load_unit(p.load_step_g, p.unit);
        self.with_bridge(|b| {
            b.set_set_sound(p.sound_enabled);
            b.set_set_tone_rest(ss(tr(TONE_NAMES[p.tone_rest_end as usize])));
            b.set_set_tone_start(ss(tr(TONE_NAMES[p.tone_set_start as usize])));
            b.set_set_tone_pre(ss(tr(TONE_NAMES[p.tone_pre_alert as usize])));
            b.set_set_vibrate(p.vibrate);
            b.set_set_haptics(p.haptics);
            b.set_set_app_keypad(p.app_keypad);
            b.set_use_app_keypad(p.app_keypad);
            b.set_set_pre_alert(p.pre_alert);
            b.set_set_pre_alert_s(ss(trf!("{} s", p.pre_alert_seconds)));
            b.set_set_background(p.background_alert);
            b.set_set_timer_default(ss(format_duration_s(p.timer_default_seconds)));
            b.set_set_adjust_step(ss(trf!("{} s", p.timer_adjust_step)));
            b.set_set_auto_rest(p.auto_start_rest);
            b.set_set_rest_end_prepare(p.rest_end_action == RestEndAction::PrepareCountdown);
            b.set_set_prepare_s(ss(trf!("{} s", p.prepare_seconds)));
            b.set_set_unit(ss(if p.unit == LoadUnit::Kg { tr("Quilogramas (kg)") } else { tr("Libras (lb)") }));
            b.set_set_load_step(ss(step));
            b.set_set_prefill_last(p.prefill_last);
            b.set_set_default_sets(ss(p.default_sets.to_string()));
            b.set_set_default_reps(ss(p.default_reps.to_string()));
            b.set_set_week_monday(p.week_starts_monday);
            b.set_set_week_view(p.calendar_week_view);
            b.set_set_theme(ss(match p.theme_mode {
                ThemeMode::System => tr("Seguir sistema"),
                ThemeMode::Light => tr("Claro"),
                ThemeMode::Dark => tr("Escuro"),
            }));
            b.set_set_text_size(ss(tr(TEXT_SIZES[p.text_size.clamp(0, 3) as usize])));
            b.set_set_density(ss(tr(DENSITIES[p.density as usize])));
            b.set_set_anim(ss(tr(ANIM_MODES[anim_index(p.anim_mode)])));
            b.set_set_anim_speed(ss(tr(ANIM_SPEEDS[p.anim_speed as usize])));
            b.set_set_language(ss(language_label(&p.language)));
            b.set_set_keep_screen(p.keep_screen_on);
            b.set_set_confirm_finish(p.confirm_finish);
            b.set_notif_status(ss(notif));
            b.set_data_info(ss(data));
            b.set_app_version(ss(&version));
            b.set_about_text(ss(about));
        });
        Ok(())
    }

    /// Resolve tema, tipografia, densidade e animações e entrega os tokens já resolvidos à UI.
    fn apply_appearance(&mut self) {
        let p = &self.prefs;
        let dark = match p.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => self.platform.system_dark_mode(),
        };
        let (enabled, reduced) = match p.anim_mode {
            AnimMode::Off => (false, false),
            AnimMode::Reduced => (true, true),
            AnimMode::Normal => (true, false),
            AnimMode::System => (self.platform.system_animations_enabled(), false),
        };
        let scale = match p.anim_speed {
            0 => 0.5,
            2 => 1.6,
            _ => 1.0,
        } * if reduced { 0.6 } else { 1.0 };
        let (ts, dn) = (p.text_size, p.density);
        let ui = self.ui();
        let t = ui.global::<Theme>();
        t.set_dark(dark);
        t.set_text_size(ts);
        t.set_font_scale(self.platform.font_scale().clamp(0.8, 1.6));
        t.set_density(dn);
        t.set_anim_enabled(enabled);
        t.set_anim_reduced(reduced);
        t.set_anim_scale(scale);
        t.set_decorative_enabled(self.prefs.module_decorative_anim);
        self.apply_modules();
    }

    /// Reflete os módulos opcionais (7.5) na interface. Desligar só esconde: nada é apagado.
    fn apply_modules(&mut self) {
        let p = self.prefs.clone();
        self.with_bridge(|b| {
            b.set_gestures_enabled(p.module_gestures);
            b.set_cal_heatmap_available(p.module_heatmap);
            b.set_cal_heatmap(p.module_heatmap && p.calendar_heatmap);
        });
    }

    fn home_shortcuts(&self) -> Vec<String> {
        let v = self.db.get_pref(shortcuts::PREF_KEY).ok().flatten();
        shortcuts::parse(v.as_deref())
    }

    fn home_shortcut(&mut self, code: &str) {
        match code {
            "next_workout" => {
                if self.db.active_session_id().ok().flatten().is_some() {
                    self.open_session();
                    return;
                }
                let today = cal::today();
                let plans = self.db.list_plans_full().unwrap_or_default();
                let next = self.next_seq_map(&plans);
                let done_ids: Vec<Id> = self
                    .sessions_on(today)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|s| self.db.get_session(s.id).ok().and_then(|x| x.plan_id))
                    .collect();
                let planned = cal::planned_on(today, today, &plans, &next, &done_ids);
                match planned.first() {
                    Some(p) => self.start_day(p.day_id),
                    None => {
                        self.toast(tr("Nenhum treino planejado para hoje. Escolha uma ficha."));
                        self.navigate(P_PLANS);
                    }
                }
            }
            "timer" => self.navigate(P_TIMER),
            "library" => {
                self.with_bridge(|b| b.set_lib_mode(ss("browse")));
                self.lib_pick = false;
                self.navigate(P_LIBRARY);
            }
            "search" => self.navigate(P_SEARCH),
            "evolution" => {
                self.with_bridge(|b| b.set_hist_tab(1));
                self.navigate(P_HIST);
            }
            "calendar" => self.navigate(P_CAL),
            "history" => {
                self.with_bridge(|b| b.set_hist_tab(0));
                self.navigate(P_HIST);
            }
            "new_plan" => self.new_plan(),
            "templates" => self.open_templates(),
            "tools" => self.open_tools(0),
            "backups" => {
                self.with_bridge(|b| b.set_settings_section(10));
                self.navigate(P_SETTINGS);
                let r = self.refresh_settings();
                self.report(r);
            }
            _ => {}
        }
    }

    /// Boas-vindas (evolução 3.2): só no primeiro acesso. Quem atualiza o app e já tem dados não
    /// vê de novo; a conclusão fica salva nas preferências e pode ser refeita em Configurações › Sobre.
    fn maybe_onboarding(&mut self) -> bool {
        if self.prefs.onboarding_done {
            return false;
        }
        let has_data = self.db.counts().map(|c| c.plans > 0 || c.sessions > 0).unwrap_or(false);
        if has_data {
            self.prefs.onboarding_done = true;
            let r = self.prefs.save(&self.db);
            self.report(r);
            return false;
        }
        self.open_onboarding();
        true
    }

    fn open_onboarding(&mut self) {
        let r = self.refresh_settings();
        self.report(r);
        self.with_bridge(|b| b.set_onboarding_step(0));
        self.stack.clear();
        self.stack.push(P_HOME);
        self.navigate(P_ONBOARDING);
    }

    fn onboarding_finish(&mut self, action: &str) {
        self.prefs.onboarding_done = true;
        let r = self.prefs.save(&self.db);
        self.report(r);
        self.with_bridge(|b| b.set_nav_dir(-1));
        match action {
            "create" => {
                self.navigate(P_PLANS);
                self.new_plan();
            }
            _ => self.navigate(P_HOME),
        }
    }

    /// 8.12: ao abrir o app com uma sessão ainda aberta, pergunta se deseja continuar ou descartar.
    /// Nada é concluído automaticamente; as séries já registradas estão salvas no banco.
    fn offer_session_recovery(&mut self) {
        let Some(sid) = self.session_id else { return };
        let Ok(f) = self.db.get_session_full(sid) else { return };
        let text = trf!(
            "Encontramos a sessão \"{} — {}\", iniciada em {}, que não foi finalizada (o app pode ter sido fechado ou o aparelho desligado).\n\n{} de {} série(s) registradas estão salvas. Deseja continuar de onde parou?",
            f.session.plan_name,
            f.session.schedule_name,
            fmt_date_time(f.session.started_at),
            f.completed_sets(),
            f.total_sets()
        );
        self.show_info(InfoCtx::RecoverSession, tr("Sessão interrompida"), &text, tr("Continuar"), tr("Descartar…"));
    }

    fn open_templates(&mut self) {
        let opts = crate::training::templates::TEMPLATES
            .iter()
            .map(|t| format!("{} — {}", tr(t.name), tr(t.description)))
            .collect();
        self.ask_choice(ChoiceCtx::Template, tr("Modelos de fichas"), tr("Escolha um modelo para ver a prévia. Nada é criado antes de confirmar."), opts, -1);
    }

    fn shortcut_slot(&mut self, slot: usize) {
        let cur = self.home_shortcuts();
        let mut opts = vec![tr("Nenhum").to_string()];
        opts.extend(shortcuts::SHORTCUTS.iter().map(|s| s.1.to_string()));
        let selected = cur
            .get(slot)
            .and_then(|c| shortcuts::SHORTCUTS.iter().position(|s| s.0 == c))
            .map(|i| i as i32 + 1)
            .unwrap_or(0);
        self.ask_choice(
            ChoiceCtx::Shortcut(slot),
            &trf!("Atalho da posição {}", slot + 1),
            tr("Se o atalho já estiver em outra posição, os dois trocam de lugar."),
            opts,
            selected,
        );
    }

    fn gesture(&mut self, ctx: &str, ev: &str) {
        let (Some(ctx), Some(ev)) = (GestureContext::parse(ctx), GestureEvent::parse(ev)) else { return };
        match gestures::resolve(self.prefs.module_gestures, ctx, ev) {
            Some(GestureAction::NextSet) => {
                self.focus_store_draft();
                self.focus_nav("next-set");
            }
            Some(GestureAction::PrevSet) => {
                self.focus_store_draft();
                self.focus_nav("prev-set");
            }
            Some(GestureAction::NextPeriod) => self.cal_shift(1),
            Some(GestureAction::PrevPeriod) => self.cal_shift(-1),
            None => {}
        }
    }

    // ------------------------------------------------------------------
    // Fase 6: armazenamento e backups no aparelho
    // ------------------------------------------------------------------

    fn local_backup_create(&mut self) {
        let data = self.platform.data_dir();
        let work = self.platform.tmp_dir();
        let version = self.platform.app_version();
        match storage::create_local_backup(&self.db, &data, &work, &version, false) {
            Ok(_) => self.toast(tr("Backup criado no aparelho. Salve uma cópia fora dele para mais segurança.")),
            Err(e) => self.toast(e.to_string()),
        }
        let r = self.refresh_settings();
        self.report(r);
    }

    fn local_backup_at(&self, i: i32) -> Option<storage::LocalBackup> {
        storage::list_local_backups(&self.platform.data_dir()).into_iter().nth(i.max(0) as usize)
    }

    fn local_backup_restore(&mut self, i: i32) {
        let Some(bk) = self.local_backup_at(i) else { return };
        // A validação consome o arquivo: trabalha numa cópia temporária, o backup local fica intacto.
        let tmp = self.tmp_file(&format!("restore-{}.zip", now_ms()));
        if let Err(e) = std::fs::copy(&bk.path, &tmp) {
            self.toast(e.to_string());
            return;
        }
        self.begin_restore(tmp);
    }

    fn local_backup_export(&mut self, i: i32) {
        let Some(bk) = self.local_backup_at(i) else { return };
        let tmp = self.tmp_file(&format!("copy-{}.zip", now_ms()));
        if let Err(e) = std::fs::copy(&bk.path, &tmp) {
            self.toast(e.to_string());
            return;
        }
        let name = bk.file_name.clone();
        self.save_via_picker(tmp, &name, "application/zip", tr("Cópia do backup salva"));
    }

    fn local_backup_delete(&mut self, i: i32) {
        let Some(bk) = self.local_backup_at(i) else { return };
        let text = trf!(
            "Excluir o backup de {}{}? Esta ação não pode ser desfeita. Seus dados atuais não são afetados.",
            fmt_date_time(bk.created_at),
            if bk.automatic { tr(" (automático)") } else { "" }
        );
        self.ask_confirm(ConfirmAction::DeleteLocalBackup(bk.path), tr("Excluir backup"), &text, tr("Excluir"));
    }

    fn storage_clear_temp(&mut self) {
        let freed = storage::clear_temp(&self.platform.tmp_dir());
        self.toast(trf!("Temporários limpos: {} liberados.", storage::fmt_bytes(freed)));
        let r = self.refresh_settings();
        self.report(r);
    }

    /// Backup automático semanal (executado ao abrir o app).
    fn auto_backup(&mut self) {
        if self.db.path.is_none() {
            return; // modo temporário: nada a proteger
        }
        let data = self.platform.data_dir();
        let work = self.platform.tmp_dir();
        let version = self.platform.app_version();
        match storage::run_auto_backup(&self.db, &data, &work, &version) {
            Ok(Some(p)) => log::info!("backup automático criado: {}", p.display()),
            Ok(None) => {}
            Err(e) => log::warn!("backup automático falhou: {e}"),
        }
    }

    /// Resumo derivado para o widget (sem segunda fonte de dados).
    fn update_widget(&mut self, cards: &[CardRow]) {
        let pick = cards
            .iter()
            .find(|c| c.kind == "session")
            .or_else(|| cards.iter().find(|c| c.kind == "today" || c.kind == "plans"))
            .or_else(|| cards.first());
        let s = match pick {
            Some(c) => widget::WidgetSummary { title: c.title.to_string(), line1: c.line1.to_string(), line2: c.line2.to_string() },
            None => widget::WidgetSummary::default(),
        };
        if widget::write(&self.platform.data_dir(), &s) {
            self.platform.update_widget();
        }
    }

    /// Resolve o idioma (preferência ou sistema) e aplica à interface Slint e às frases do Rust.
    fn apply_language(&mut self) {
        let lang = match self.prefs.language.as_str() {
            "pt-BR" => Lang::Pt,
            "en" => Lang::En,
            "es" => Lang::Es,
            _ => Lang::from_system(&self.platform.system_locale()),
        };
        crate::i18n::set_lang(lang);
        if let Err(e) = slint::select_bundled_translation(lang.slint_code()) {
            log::warn!("idioma {} indisponível na interface: {e:?}", lang.code());
        }
        self.with_bridge(|b| b.set_decimal_sep(ss(lang.decimal_sep().to_string())));
    }

    /// Depois de trocar o idioma: refaz todos os textos montados pelo Rust.
    fn relocalize(&mut self) {
        self.apply_language();
        let r = self.refresh_settings();
        self.report(r);
        let _ = self.refresh_home();
        let _ = self.refresh_calendar();
        self.refresh_timer();
        let p = self.page();
        self.refresh_page(p);
    }

    fn save_prefs(&mut self) {
        let r = self.prefs.save(&self.db);
        self.report(r);
        self.apply_timer_prefs();
        self.apply_appearance();
        let r = self.refresh_settings();
        self.report(r);
    }

    fn pref_toggle(&mut self, key: &str) {
        let p = &mut self.prefs;
        match key {
            "sound" => p.sound_enabled = !p.sound_enabled,
            "vibrate" => p.vibrate = !p.vibrate,
            "haptics" => p.haptics = !p.haptics,
            "app-keypad" => p.app_keypad = !p.app_keypad,
            "pre-alert" => p.pre_alert = !p.pre_alert,
            "background" => {
                if !p.background_alert && self.platform.is_android() && !self.platform.notifications_allowed() {
                    self.show_info(
                        InfoCtx::NotifPermission { then_enable: true },
                        tr("Permitir notificações"),
                        tr("O alerta com o app em segundo plano é exibido como notificação. Ao continuar, o Android vai pedir sua autorização. Você pode negar: o restante do app continua funcionando."),
                        tr("Continuar"),
                        tr("Cancelar"),
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
            "keep-screen" => p.keep_screen_on = !p.keep_screen_on,
            "confirm-finish" => p.confirm_finish = !p.confirm_finish,
            "effort" => p.effort = !p.effort,
            "mod-records" => p.module_records = !p.module_records,
            "mod-heatmap" => p.module_heatmap = !p.module_heatmap,
            "mod-progression" => p.module_progression = !p.module_progression,
            "mod-gestures" => p.module_gestures = !p.module_gestures,
            "mod-decorative" => p.module_decorative_anim = !p.module_decorative_anim,
            "mod-backup-reminder" => p.module_backup_reminder = !p.module_backup_reminder,
            "auto-backup" => {
                let on = self.db.get_pref(storage::PREF_AUTO).ok().flatten().as_deref() != Some("0");
                let r = self.db.set_pref(storage::PREF_AUTO, if on { "0" } else { "1" });
                self.report(r);
                let r = self.refresh_settings();
                self.report(r);
                return;
            }
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
        let tones: Vec<String> = TONE_NAMES.iter().map(|s| tr(s).to_string()).collect();
        match key {
            "tone-rest" => self.ask_choice(ChoiceCtx::Tone("rest"), tr("Som do fim do descanso"), "", tones, self.prefs.tone_rest_end),
            "tone-start" => self.ask_choice(ChoiceCtx::Tone("start"), tr("Som do início de série"), "", tones, self.prefs.tone_set_start),
            "tone-pre" => self.ask_choice(ChoiceCtx::Tone("pre"), tr("Som do pré-alerta"), "", tones, self.prefs.tone_pre_alert),
            "unit" => self.ask_choice(
                ChoiceCtx::Unit,
                tr("Unidade de carga"),
                tr("Os valores são guardados com precisão em gramas; trocar a unidade só muda a exibição."),
                vec![tr("Quilogramas (kg)").into(), tr("Libras (lb)").into()],
                if self.prefs.unit == LoadUnit::Kg { 0 } else { 1 },
            ),
            "language" => {
                let opts: Vec<String> = LANG_CODES.iter().map(|c| language_label(c)).collect();
                let sel = LANG_CODES.iter().position(|c| *c == self.prefs.language).unwrap_or(0) as i32;
                self.ask_choice(
                    ChoiceCtx::Language,
                    tr("Idioma"),
                    tr("Muda os textos do aplicativo. Nomes de exercícios, fichas e anotações não são alterados."),
                    opts,
                    sel,
                );
            }
            "theme" => self.ask_choice(
                ChoiceCtx::Appearance("theme"),
                tr("Tema"),
                tr("\"Seguir sistema\" acompanha o modo claro/escuro do Android."),
                vec![tr("Seguir sistema").into(), tr("Claro").into(), tr("Escuro").into()],
                match self.prefs.theme_mode {
                    ThemeMode::System => 0,
                    ThemeMode::Light => 1,
                    ThemeMode::Dark => 2,
                },
            ),
            "text-size" => self.ask_choice(
                ChoiceCtx::Appearance("text-size"),
                tr("Tamanho do texto"),
                "",
                TEXT_SIZES.iter().map(|s| tr(s).to_string()).collect(),
                self.prefs.text_size,
            ),
            "density" => self.ask_choice(
                ChoiceCtx::Appearance("density"),
                tr("Densidade da interface"),
                tr("A densidade compacta não reduz o tamanho mínimo dos botões."),
                DENSITIES.iter().map(|s| tr(s).to_string()).collect(),
                self.prefs.density,
            ),
            "anim" => self.ask_choice(
                ChoiceCtx::Appearance("anim"),
                tr("Animações"),
                tr("\"Seguir sistema\" respeita a opção \"Remover animações\" do Android."),
                ANIM_MODES.iter().map(|s| tr(s).to_string()).collect(),
                anim_index(self.prefs.anim_mode) as i32,
            ),
            "anim-speed" => self.ask_choice(
                ChoiceCtx::Appearance("anim-speed"),
                tr("Velocidade das animações"),
                "",
                ANIM_SPEEDS.iter().map(|s| tr(s).to_string()).collect(),
                self.prefs.anim_speed,
            ),
            "load-step" => {
                let opts: Vec<String> = LOAD_STEPS.iter().map(|g| format_load_unit(*g, self.prefs.unit)).collect();
                let sel = LOAD_STEPS.iter().position(|g| *g == self.prefs.load_step_g).map(|i| i as i32).unwrap_or(-1);
                self.ask_choice(ChoiceCtx::LoadStep, tr("Incremento de carga"), "", opts, sel);
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
            ChoiceCtx::Appearance(which) => {
                let p = &mut self.prefs;
                match which {
                    "theme" => p.theme_mode = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark][(i as usize).min(2)],
                    "text-size" => p.text_size = i.clamp(0, 3),
                    "density" => p.density = i.min(2),
                    "anim" => {
                        p.anim_mode = [AnimMode::System, AnimMode::Normal, AnimMode::Reduced, AnimMode::Off][(i as usize).min(3)]
                    }
                    _ => p.anim_speed = i.min(2),
                }
                self.save_prefs();
            }
            ChoiceCtx::Effort(set_id, field) => {
                let r = self.db.get_session_set(set_id).and_then(|s| match field {
                    "rpe" => {
                        let v = if i <= 0 { None } else { crate::history::analytics::RPE_VALUES.get(i as usize - 1).copied() };
                        self.db.set_set_effort(set_id, v, s.rir)
                    }
                    _ => {
                        let v = if i <= 0 { None } else { Some(i as i64 - 1) };
                        self.db.set_set_effort(set_id, s.rpe, v)
                    }
                });
                if let Err(e) = r {
                    self.toast(e.to_string());
                }
                let r = self.refresh_session();
                self.report(r);
            }
            ChoiceCtx::Feeling => {
                if let Some(sid) = self.session_id {
                    let v = if i <= 0 { None } else { Some(i as i64) };
                    if let Err(e) = self.db.set_session_feeling(sid, v) {
                        self.toast(e.to_string());
                    }
                }
                let r = self.refresh_session();
                self.report(r);
            }
            ChoiceCtx::Compare(ids) => {
                if let (Some(b), Some(a)) = (self.detail_id, ids.get(i.max(0) as usize).copied()) {
                    // A = a sessão escolhida (normalmente a anterior), B = a sessão aberta.
                    let (a, b) = match (self.db.get_session(a), self.db.get_session(b)) {
                        (Ok(x), Ok(y)) if x.started_at > y.started_at => (b, a),
                        _ => (a, b),
                    };
                    self.cmp = Some((a, b));
                    self.navigate(P_COMPARE);
                }
            }
            ChoiceCtx::Illustration => {
                // 0 = automática; demais = ILLUSTRATIONS[i-1]
                let (id, label) = if i <= 0 {
                    (String::new(), tr("Automática"))
                } else {
                    let (k, _) = crate::exercise_media::ILLUSTRATIONS[(i as usize - 1).min(crate::exercise_media::ILLUSTRATIONS.len() - 1)];
                    (k.to_string(), crate::exercise_media::label(k))
                };
                let preview = if id.is_empty() {
                    let name = self.with_bridge(|b| b.get_exf_name().to_string());
                    let pattern = self.with_bridge(|b| b.get_exf_pattern().to_string());
                    crate::exercise_media::guess(&pattern, &name).to_string()
                } else {
                    id.clone()
                };
                self.exf_illustration = id;
                self.with_bridge(|b| {
                    b.set_exf_art(ss(preview));
                    b.set_exf_art_label(ss(label));
                    b.set_dialog(ss("exercise"));
                });
            }
            ChoiceCtx::Language => {
                self.prefs.language = LANG_CODES.get(i.max(0) as usize).unwrap_or(&"system").to_string();
                let r = self.prefs.save(&self.db);
                self.report(r);
                self.relocalize();
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
                    "type" => b.set_lib_type(ss(&v)),
                    _ => b.set_lib_pattern(ss(&v)),
                });
                self.lib_limit = LIB_PAGE;
                let r = self.refresh_library();
                self.report(r);
            }
            ChoiceCtx::ImportSheet => {
                if let Some(st) = self.import.as_mut() {
                    if i as usize >= st.workbook.sheets.len() {
                        st.sheet = usize::MAX; // todas as abas
                    } else {
                        st.sheet = i as usize;
                        st.mapping = detect_mapping(&st.workbook.sheets[st.sheet]);
                    }
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
            ChoiceCtx::Shortcut(slot) => {
                // 0 = "Nenhum"; demais = SHORTCUTS[i-1]
                let code = if i <= 0 { "" } else { shortcuts::SHORTCUTS.get(i as usize - 1).map(|s| s.0).unwrap_or("") };
                let cur = self.home_shortcuts();
                let next = shortcuts::set_slot(&cur, slot, code);
                let r = self.db.set_pref(shortcuts::PREF_KEY, &shortcuts::serialize(&next));
                self.report(r);
                let r = self.refresh_settings();
                self.report(r);
            }
            ChoiceCtx::Template => {
                let Some(t) = crate::training::templates::TEMPLATES.get(i as usize) else { return };
                match self.db.template_preview(t.code) {
                    Ok(p) => {
                        let mut text = p.text;
                        if !p.missing_exercises.is_empty() {
                            text.push_str(&trf!(
                                "\nAtenção: {} exercício(s) não estão na biblioteca e serão criados como personalizados: {}.",
                                p.missing_exercises.len(),
                                p.missing_exercises.join(", ")
                            ));
                        }
                        text.push_str("\nA ficha criada é independente: você poderá editar exercícios, séries e cargas.");
                        self.show_info(InfoCtx::Template(t.code), &trf!("Modelo {}", tr(t.name)), &text, tr("Criar ficha"), tr("Cancelar"));
                    }
                    Err(e) => self.toast(e.to_string()),
                }
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
        let is_backup = mime == "application/zip";
        self.file_op = Some(FileOp::Save(what.to_string(), is_backup));
        self.platform.save_document(name, mime, &path);
    }

    fn do_export(&mut self, kind: ExportKind) {
        let name = match kind {
            ExportKind::Plans => format!("empeto-fichas-{}.xlsx", Self::today_tag()),
            ExportKind::History => format!("empeto-historico-{}.xlsx", Self::today_tag()),
            ExportKind::All => format!("empeto-dados-{}.xlsx", Self::today_tag()),
        };
        let path = self.tmp_file(&format!("export-{}.xlsx", now_ms()));
        match export_xlsx(&self.db, kind, &path) {
            Ok(_) => self.save_via_picker(path, &name, XLSX_MIME, tr("Planilha exportada")),
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn data_template(&mut self) {
        let path = self.tmp_file(&format!("modelo-{}.xlsx", now_ms()));
        match export_template(&path) {
            Ok(()) => self.save_via_picker(path, "modelo-importacao-empeto.xlsx", XLSX_MIME, tr("Modelo salvo")),
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn data_backup(&mut self) {
        self.ask_text(
            TextCtx::BackupPassword,
            tr("Senha do backup (opcional)"),
            tr("Com senha, o banco dentro do arquivo é criptografado (AES-256) e só pode ser restaurado com ela. Deixe em branco para não usar senha. Não há como recuperar uma senha esquecida."),
            "",
            false,
            false,
        );
        self.with_bridge(|b| b.set_text_password(true));
    }

    fn do_backup(&mut self, password: Option<&str>) {
        let path = self.tmp_file(&format!("backup-{}.zip", now_ms()));
        let work = self.platform.tmp_dir();
        let version = self.platform.app_version();
        match create_backup(&self.db, &path, &work, &version, password) {
            Ok(_) => {
                let name = format!("empeto-backup-{}.zip", Self::today_tag());
                self.save_via_picker(path, &name, "application/zip", tr("Backup criado"));
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
                self.save_via_picker(path, "empeto-calendario.ics", "text/calendar", tr("Calendário exportado"));
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
            (FileOp::Save(what, is_backup), FileResult::Saved { name }) => {
                if is_backup {
                    // Só registramos a cópia externa quando o próprio usuário a salvou.
                    let r = self.db.set_pref(storage::PREF_LAST_EXTERNAL, &now_ms().to_string());
                    self.report(r);
                }
                self.toast(format!("{}: {}", what, name))
            }
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
                    Err(e) => self.show_info(InfoCtx::Plain, tr("Não foi possível importar"), &e.to_string(), tr("OK"), ""),
                }
            }
            (FileOp::Restore, FileResult::Opened { path, name: _ }) => self.begin_restore(path),
            (op, res) => log::warn!("resultado de arquivo inesperado: {op:?} {res:?}"),
        }
    }

    /// Início da restauração (arquivo escolhido ou backup local): pede senha se preciso e valida.
    fn begin_restore(&mut self, path: std::path::PathBuf) {
        {
            {
                match crate::import_export::backup::read_manifest(&path) {
                    Ok(m) if m.encrypted => {
                        self.restore_path = Some(path);
                        self.ask_text(
                            TextCtx::RestorePassword,
                            tr("Backup protegido por senha"),
                            &trf!("Backup de {}. Informe a senha usada ao criá-lo.", fmt_date_time(m.created_at)),
                            "",
                            false,
                            false,
                        );
                        self.with_bridge(|b| b.set_text_password(true));
                    }
                    _ => self.validate_restore(path, None),
                }
            }
        }
    }

    fn validate_restore(&mut self, path: std::path::PathBuf, password: Option<&str>) {
        {
            {
                let work = self.platform.tmp_dir();
                let r = validate_backup(&path, &work, password);
                let wrong_password = matches!(&r, Err(AppError::Validation(m)) if m.contains("incorreta"));
                if wrong_password {
                    // Mantém o arquivo para nova tentativa.
                    self.restore_path = Some(path);
                    self.toast(tr("Senha do backup incorreta. Tente novamente."));
                    self.ask_text(TextCtx::RestorePassword, tr("Backup protegido por senha"), tr("Senha incorreta. Informe novamente."), "", false, false);
                    self.with_bridge(|b| b.set_text_password(true));
                    return;
                }
                let _ = std::fs::remove_file(&path);
                match r {
                    Ok(v) => {
                        let m = &v.manifest;
                        let text = trf!(
                            "Backup de {} (app {}): {} ficha(s), {} sessão(ões), {} série(s).\n\nOs dados atuais serão substituídos por este backup. Uma cópia do estado atual é guardada para desfazer em caso de falha. Deseja continuar?",
                            fmt_date_time(m.created_at),
                            m.app_version,
                            m.counts.plans,
                            m.counts.sessions,
                            m.counts.sets
                        );
                        self.pending_restore = Some(v);
                        self.ask_confirm(ConfirmAction::Restore, tr("Restaurar backup"), &text, tr("Restaurar"));
                    }
                    Err(e) => self.show_info(InfoCtx::Plain, tr("Backup inválido"), &e.to_string(), tr("OK"), ""),
                }
            }
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
            if st.sheet == usize::MAX {
                let parts = st
                    .workbook
                    .sheets
                    .iter()
                    .map(|sh| build_preview(&self.db, sh, &detect_mapping(sh), defaults))
                    .collect();
                st.preview = crate::import_export::xlsx_import::merge_previews(parts);
            } else {
                let sheet = &st.workbook.sheets[st.sheet];
                st.preview = build_preview(&self.db, sheet, &st.mapping, defaults);
            }
        }
        self.with_bridge(|b| b.set_imp_reviewed(false));
        self.refresh_import();
    }

    fn refresh_import(&mut self) {
        let Some(st) = self.import.as_ref() else { return };
        let all = st.sheet == usize::MAX;
        let sheet = &st.workbook.sheets[if all { 0 } else { st.sheet }];
        let header = if all { vec![] } else { sheet.rows.get(st.mapping.header_row).cloned().unwrap_or_default() };
        let sample_row = sheet.rows.get(st.mapping.header_row + 1).cloned().unwrap_or_default();
        let ncols = if all { 0 } else { st.mapping.columns.len() };
        let cols: Vec<MapCol> = (0..ncols)
            .filter(|c| header.get(*c).map(|h| !h.trim().is_empty()).unwrap_or(false) || st.mapping.columns[*c].is_some())
            .map(|c| MapCol {
                index: c as i32,
                header: ss(header.get(c).filter(|h| !h.trim().is_empty()).cloned().unwrap_or_else(|| trf!("Coluna {}", c + 1))),
                sample: ss(sample_row.get(c).map(|s| trf!("Ex.: {}", s)).unwrap_or_default()),
                field: ss(st.mapping.columns[c].map(|f| f.label()).unwrap_or(tr("Ignorar"))),
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
                                .map(|e| trf!(
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
        let summary = trf!(
            "{} linha(s) com dados, {} válida(s) → {} ficha(s), {} exercício(s). Nada é gravado até você confirmar.",
            st.preview.data_rows,
            st.preview.valid_rows,
            st.preview.plans.len(),
            st.preview.exercise_count()
        );
        let p = &st.preview;
        let (file, sheet_name, multi, can, conflicts, replace) = (
            st.file_name.clone(),
            if all { trf!("Todas as abas ({})", st.workbook.sheets.len()) } else { sheet.name.clone() },
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
            b.set_imp_all_sheets(all);
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
            self.toast(tr("Confira os avisos antes de importar."));
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
                    tr("Importação concluída"),
                    &trf!(
                        "{} ficha(s), {} dia(s) e {} exercício(s) importados.{}{}",
                        r.plans,
                        r.days,
                        r.exercises,
                        if r.new_catalog_exercises > 0 {
                            trf!("\n{} exercício(s) novo(s) adicionados à biblioteca.", r.new_catalog_exercises)
                        } else {
                            String::new()
                        },
                        if r.replaced > 0 { trf!("\n{} ficha(s) substituída(s).", r.replaced) } else { String::new() }
                    ),
                    tr("OK"),
                    "",
                );
            }
            Err(e) => self.show_info(InfoCtx::Plain, tr("Importação não realizada"), &trf!("{}\n\nNenhum dado foi gravado.", e), tr("OK"), ""),
        }
    }

    fn do_restore(&mut self) {
        let Some(v) = self.pending_restore.take() else { return };
        match self.db.restore_from(v) {
            Ok(()) => {
                self.prefs = Prefs::load(&self.db).unwrap_or_default();
                self.session_id = self.db.active_session_id().ok().flatten();
                self.load_timer();
                self.apply_appearance();
                self.navigate(P_HOME);
                self.show_info(InfoCtx::Plain, tr("Backup restaurado"), tr("Fichas, sessões, exercícios e preferências foram restaurados."), tr("OK"), "");
            }
            Err(e) => self.show_info(
                InfoCtx::Plain,
                tr("Falha na restauração"),
                &trf!("{}\n\nOs dados anteriores foram mantidos.", e),
                tr("OK"),
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
                    self.toast(tr("Ficha excluída. Sessões já realizadas continuam no histórico."));
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
                let before = self.db.get_plan_exercise(id).ok().map(|pe| {
                    let pos = self.db.list_plan_exercises(pe.schedule_id).map(|l| l.iter().position(|x| x.id == id).unwrap_or(0)).unwrap_or(0);
                    (pe, pos)
                });
                let r = self.db.delete_plan_exercise(id);
                if let (Ok(()), Some((pe, pos))) = (&r, before) {
                    self.toast_undo(
                        trf!("{} removido da ficha.", pe.exercise.name),
                        UndoOp::new(
                            UndoKind::DeletePlanExercise {
                                day_id: pe.schedule_id,
                                exercise_id: pe.exercise.id,
                                target: pe.target,
                                rest_seconds: pe.rest_seconds,
                                notes: pe.notes.clone(),
                                position: pos,
                            },
                            tr("Remoção do exercício"),
                        ),
                    );
                }
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
                            self.toast(tr("Sessão descartada. Nada foi gravado no histórico."));
                        }
                        Err(e) => self.toast(e.to_string()),
                    }
                }
            }
            ConfirmAction::Restore => self.do_restore(),
            ConfirmAction::DeleteLocalBackup(path) => {
                let r = storage::delete_local_backup(&self.platform.data_dir(), &path);
                match r {
                    Ok(()) => self.toast(tr("Backup excluído.")),
                    Err(e) => self.toast(e.to_string()),
                }
                let r = self.refresh_settings();
                self.report(r);
            }
            ConfirmAction::ArchiveExercise(id, archive) => {
                let r = self.db.set_exercise_archived(id, archive);
                match r {
                    Ok(()) => self.toast_undo(
                        if archive { tr("Exercício arquivado.") } else { tr("Exercício reativado.") },
                        UndoOp::new(
                            UndoKind::ArchiveExercise { exercise_id: id, was_archived: !archive },
                            if archive { tr("Arquivamento") } else { tr("Reativação") },
                        ),
                    ),
                    Err(e) => self.toast(e.to_string()),
                }
                let r = self.refresh_library();
                self.report(r);
            }
        }
    }

    fn text_saved(&mut self, value: &str) {
        let Some(ctx) = self.text.clone() else { return };
        match ctx {
            TextCtx::BackupPassword => {
                let pw = value.trim().to_string();
                if !pw.is_empty() && pw.chars().count() < 4 {
                    self.toast(tr("A senha deve ter pelo menos 4 caracteres (ou deixe em branco)."));
                    return;
                }
                self.close_dialog();
                self.do_backup(if pw.is_empty() { None } else { Some(&pw) });
                return;
            }
            TextCtx::RestorePassword => {
                self.close_dialog();
                if let Some(path) = self.restore_path.take() {
                    let pw = value.to_string();
                    self.validate_restore(path, Some(&pw));
                }
                return;
            }
            _ => {}
        }
        let r = match ctx {
            TextCtx::BackupPassword | TextCtx::RestorePassword => Ok(()),
            TextCtx::SessionNotes => self.session_id.map(|sid| self.db.set_session_notes(sid, value)).unwrap_or(Ok(())),
            TextCtx::ExerciseNotes(se) => self.db.set_session_exercise_notes(se, value),
            TextCtx::ToolsValue(field) => match parse_load(value, self.prefs.unit) {
                Ok(g) => {
                    match field {
                        "bar" => self.tools.bar_g = g,
                        "work" => self.tools.work_g = g,
                        _ => self.tools.target_g = g,
                    }
                    self.save_tools();
                    self.refresh_tools();
                    Ok(())
                }
                Err(e) => Err(e),
            },
            TextCtx::TimerCustom => {
                let secs = parse_rest(value).and_then(|s| if s < 1 { validation(tr("Informe pelo menos 1 segundo.")) } else { Ok(s) });
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
            Some(InfoCtx::ActiveSession) | Some(InfoCtx::RecoverSession) => self.open_session(),
            Some(InfoCtx::Template(code)) => match self.db.create_from_template(code) {
                Ok(id) => {
                    self.plan_id = Some(id);
                    self.navigate(P_EDITOR);
                    self.toast(tr("Ficha criada a partir do modelo. Ajuste à vontade — o modelo não muda."));
                }
                Err(e) => self.toast(e.to_string()),
            },
            _ => {}
        }
    }

    fn info_alt(&mut self) {
        let ctx = self.info;
        self.close_dialog();
        if let Some(InfoCtx::RecoverSession) = ctx {
            self.ask_confirm(
                ConfirmAction::DiscardSession,
                tr("Descartar sessão"),
                tr("Descartar a sessão interrompida? As séries registradas nela não irão para o histórico. Para guardá-las, escolha Cancelar e finalize a sessão."),
                tr("Descartar"),
            );
            return;
        }
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
                self.toast(tr("Notificações permitidas. Você receberá o alerta com a tela bloqueada."));
            } else {
                self.toast(tr("Permissão negada. O cronômetro continua funcionando na tela; em segundo plano o alerta pode não aparecer."));
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
                self.with_bridge(|b| {
                    b.set_toast(ss(""));
                    b.set_toast_action(ss(""));
                });
                self.undo = None;
            }
        }
    }
}

const LIB_PAGE: usize = 40;
const LOAD_STEPS: [i64; 6] = [500, 1_000, 1_250, 2_000, 2_500, 5_000];
const TEXT_SIZES: [&str; 4] = ["Pequena", "Média", "Grande", "Extragrande"];
const DENSITIES: [&str; 3] = ["Compacta", "Padrão", "Espaçosa"];
const ANIM_MODES: [&str; 4] = ["Seguir sistema", "Normais", "Reduzidas", "Desativadas"];
const ANIM_SPEEDS: [&str; 3] = ["Rápida", "Normal", "Lenta"];

/// Opções de idioma: o nome de cada idioma aparece nele mesmo; "Seguir o sistema" é traduzido.
const LANG_CODES: [&str; 4] = ["system", "pt-BR", "en", "es"];

fn language_label(code: &str) -> String {
    match code {
        "pt-BR" => Lang::Pt.native_name().to_string(),
        "en" => Lang::En.native_name().to_string(),
        "es" => Lang::Es.native_name().to_string(),
        _ => tr("Seguir o sistema").to_string(),
    }
}

fn anim_index(m: AnimMode) -> usize {
    match m {
        AnimMode::System => 0,
        AnimMode::Normal => 1,
        AnimMode::Reduced => 2,
        AnimMode::Off => 3,
    }
}

/// Arco (viewbox 100x100, raio 42) da fração restante, a partir do topo no sentido horário.
fn ring_path(remaining: f32) -> String {
    let f = remaining.clamp(0.0, 1.0) as f64;
    if f <= 0.001 {
        return String::new();
    }
    if f >= 0.999 {
        return "M 50 8 A 42 42 0 1 1 49.99 8".into();
    }
    let a = std::f64::consts::TAU * f;
    let (x, y) = (50.0 + 42.0 * a.sin(), 50.0 - 42.0 * a.cos());
    format!("M 50 8 A 42 42 0 {} 1 {:.2} {:.2}", if f > 0.5 { 1 } else { 0 }, x, y)
}

/// Estado das calculadoras de anilhas e aquecimento (8.5/8.6).
#[derive(Debug, Clone, Default)]
struct ToolsState {
    loaded: bool,
    target_g: i64,
    bar_g: i64,
    work_g: i64,
    plates: Vec<crate::domain::calculators::Plate>,
    steps: Vec<crate::domain::calculators::WarmupStep>,
}

/// Grupo do exercício `i` no dia: (rótulo no primeiro membro, posição 0 solto/1 primeiro/2 meio/3 último,
/// ligado ao próximo?, é o último do dia?).
fn plan_group_info(list: &[PlanExercise], i: usize) -> (String, i32, bool, bool) {
    let tags: Vec<String> = list.iter().map(|e| e.group_tag.clone()).collect();
    let (a, b) = crate::training::groups::members(&tags, i);
    let links = crate::training::groups::links_of(&tags);
    let last = i + 1 >= list.len();
    match crate::training::groups::GroupKind::of_size(b - a + 1) {
        None => (String::new(), 0, false, last),
        Some(k) => {
            let label = if i == a { k.label().to_uppercase() } else { String::new() };
            let pos = if i == a { 1 } else if i == b { 3 } else { 2 };
            (label, pos, links.get(i).copied().unwrap_or(false), last)
        }
    }
}

fn fmt_rpe(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v).replace('.', &crate::i18n::decimal_sep().to_string())
    }
}

fn ex_row(e: &Exercise, note: &str) -> ExRow {
    let detail = [e.muscle_focus.as_str(), e.exercise_type.as_str(), e.equipment.as_str(), e.movement_pattern.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    ExRow {
        id: e.id as i32,
        name: ss(&e.name),
        detail: ss(detail),
        note: ss(note),
        archived: e.archived,
        favorite: e.favorite,
        custom: e.custom,
        art: ss(crate::exercise_media::resolve(e)),
    }
}

fn target_text(t: &Target, rest: i64, unit: LoadUnit) -> String {
    let mut s = trf!("{} × {}", t.sets, format_reps(t.reps_min, t.reps_max));
    if t.load_g > 0 {
        s.push_str(&format!(" · {}", format_load_unit(t.load_g, unit)));
    }
    if rest > 0 {
        s.push_str(&trf!(" · descanso {}", format_duration_s(rest)));
    }
    s
}

/// Constrói a aplicação: abre o banco, cria a janela, registra callbacks e roda o loop de eventos.
pub fn run(platform: Box<dyn Platform>) -> Result<(), slint::PlatformError> {
    crate::platform::crumbs::crumb("criando janela (AppWindow::new)");
    let ui = AppWindow::new()?;
    crate::platform::crumbs::crumb("janela criada; abrindo banco de dados");
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
    crate::platform::crumbs::crumb("banco aberto; carregando preferências");
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
        exf_illustration: String::new(),
        lib_pick: false,
        lib_limit: LIB_PAGE,
        alt_for: None,
        restore_path: None,
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
        focus_set: 0,
        drafts: HashMap::new(),
        repeat_prev: Default::default(),
        tools: ToolsState::default(),
        cmp: None,
        hist_limit: 100,
        rep_month: (today.year(), today.month()),
        undo: None,
        focus_fresh: true,
        evo_chart: (300.0, 160.0),
        evo_selected: -1,
        search_hits: Vec::new(),
    }));

    {
        let mut c = ctrl.borrow_mut();
        c.load_timer();
        c.session_id = c.db.active_session_id().ok().flatten();
        let week_view = c.prefs.calendar_week_view;
        c.apply_language();
        c.apply_appearance();
        let b = ui.global::<Bridge>();
        b.set_cal_week_view(week_view);
        b.set_lib_mode(ss("browse"));
        b.set_timer_presets(model(vec![30, 45, 60, 90, 120, 180]));
        b.set_use_app_keypad(c.prefs.app_keypad);
        c.apply_modules();
        c.auto_backup();
        c.refresh_timer();
        c.navigate(P_HOME);
        if !c.maybe_onboarding() {
            c.offer_session_recovery();
        }
        if let Some(err) = open_error {
            c.show_info(
                InfoCtx::Plain,
                tr("Erro ao abrir os dados"),
                &trf!("{}\n\nO app abriu em modo temporário (nada será salvo). Restaure um backup ou reinstale o app.", err),
                tr("OK"),
                "",
            );
        }
    }
    crate::platform::crumbs::crumb("estado inicial pronto; registrando callbacks");
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
        crate::platform::crumbs::crumb("interface pronta (laço de eventos rodando)");
    });

    crate::platform::crumbs::crumb("iniciando laço de eventos (ui.run)");
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
            Ok(_) => c.toast(tr("Ficha duplicada.")),
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
            tr("Excluir ficha"),
            &trf!("Excluir \"{}\"? As sessões já realizadas com esta ficha continuam no histórico.", name),
            tr("Excluir"),
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
            tr("Excluir dia"),
            &trf!("Excluir \"{}\" e seus exercícios desta ficha? O histórico não é afetado.", name),
            tr("Excluir"),
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
            c.ask_confirm(ConfirmAction::DeletePlanEx(id), tr("Remover exercício"), tr("Remover este exercício da ficha? O histórico não é afetado."), tr("Remover"));
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
            "type" => "type",
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
            b.set_lib_type(ss(""));
            b.set_lib_favorites(false);
            b.set_lib_custom(false);
            b.set_lib_show_archived(false);
        });
        c.lib_limit = LIB_PAGE;
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_toggle, |c, k: SharedString| {
        c.with_bridge(|b| match k.as_str() {
            "open" => b.set_lib_filters_open(!b.get_lib_filters_open()),
            "favorites" => b.set_lib_favorites(!b.get_lib_favorites()),
            "custom" => b.set_lib_custom(!b.get_lib_custom()),
            _ => {}
        });
        c.lib_limit = LIB_PAGE;
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_more, |c| {
        c.lib_limit += LIB_PAGE;
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_favorite, |c, id: i32| {
        let r = c.db.get_exercise(id as Id).and_then(|e| c.db.set_exercise_favorite(e.id, !e.favorite));
        c.report(r);
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_plan_favorite, |c, id: i32| {
        let r = c.db.get_plan(id as Id).and_then(|p| c.db.set_plan_favorite(p.id, !p.favorite));
        c.report(r);
        let r = c.refresh_plans();
        c.report(r);
    });
    cb!(b, ctrl, on_open_templates, |c| { c.open_templates(); });
    cb!(b, ctrl, on_open_tools, |c, se: i32| { c.open_tools(se as Id); });
    cb!(b, ctrl, on_session_effort, |c, set_id: i32, f: SharedString| {
        let field: &'static str = if f.as_str() == "rpe" { "rpe" } else { "rir" };
        let cur = c.db.get_session_set(set_id as Id).ok();
        let (title, text, opts, sel) = if field == "rpe" {
            let mut o = vec![tr("Não informar").to_string()];
            o.extend(crate::history::analytics::RPE_VALUES.iter().map(|v| fmt_rpe(*v)));
            let sel = cur
                .and_then(|s| s.rpe)
                .and_then(|r| crate::history::analytics::RPE_VALUES.iter().position(|v| *v == r))
                .map(|p| p as i32 + 1)
                .unwrap_or(0);
            (tr("RPE — esforço percebido"), tr("10 = esforço máximo; 8 = ainda daria para fazer cerca de 2 repetições."), o, sel)
        } else {
            let mut o = vec![tr("Não informar").to_string()];
            o.extend((0..=5).map(|n| if n == 5 { tr("5 ou mais").to_string() } else { n.to_string() }));
            let sel = cur.and_then(|s| s.rir).map(|r| r.min(5) as i32 + 1).unwrap_or(0);
            (tr("RIR — repetições em reserva"), tr("Quantas repetições você ainda conseguiria fazer com boa técnica."), o, sel)
        };
        c.ask_choice(ChoiceCtx::Effort(set_id as Id, field), title, text, opts, sel);
    });
    cb!(b, ctrl, on_session_choose_feeling, |c| {
        let mut opts = vec![tr("Não informar").to_string()];
        opts.extend((1..=5).map(|f| crate::history::analytics::feeling_label(f).to_string()));
        let sel = c.session_id.and_then(|s| c.db.get_session(s).ok()).and_then(|s| s.feeling).unwrap_or(0) as i32;
        c.ask_choice(ChoiceCtx::Feeling, tr("Como foi o treino?"), tr("Fica no diário de treinamento junto das observações."), opts, sel);
    });
    cb!(b, ctrl, on_detail_compare, |c| { c.detail_compare(); });
    cb!(b, ctrl, on_hist_more, |c| {
        c.hist_limit += 100;
        let r = c.refresh_history();
        c.report(r);
    });
    cb!(b, ctrl, on_rep_shift, |c, d: i32| {
        let (y, m) = c.rep_month;
        let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_else(cal::today);
        let n = cal::add_months(first, d);
        c.rep_month = (n.year(), n.month());
        let r = c.refresh_report();
        c.report(r);
    });
    cb!(b, ctrl, on_rep_share, |c| {
        let text = c.report_text();
        if !c.platform.share_text(tr("Resumo do mês — EMPETO"), &text) {
            c.toast(tr("Nenhum aplicativo disponível para compartilhar."));
        }
    });
    cb!(b, ctrl, on_tools_step, |c, f: SharedString, d: i32| { c.tools_step(&f, d); });
    cb!(b, ctrl, on_tools_edit, |c, f: SharedString| {
        let field: &'static str = match f.as_str() {
            "bar" => "bar",
            "work" => "work",
            _ => "target",
        };
        let current = match field {
            "bar" => c.tools.bar_g,
            "work" => c.tools.work_g,
            _ => c.tools.target_g,
        };
        let unit = c.prefs.unit;
        c.ask_text(TextCtx::ToolsValue(field), tr("Carga"), unit.as_str(), &format_load(current, unit), false, true);
    });
    cb!(b, ctrl, on_tools_plate_pairs, |c, i: i32, d: i32| {
        c.load_tools();
        if let Some(p) = c.tools.plates.get_mut(i.max(0) as usize) {
            p.pairs = (p.pairs + d as i64).clamp(0, 10);
        }
        c.save_tools();
        c.refresh_tools();
    });
    cb!(b, ctrl, on_tools_step_edit, |c, i: i32, dp: i32, dr: i32| {
        c.load_tools();
        if let Some(s) = c.tools.steps.get_mut(i.max(0) as usize) {
            s.percent = (s.percent + dp as i64).clamp(5, 95);
            s.reps = (s.reps + dr as i64).clamp(1, 30);
        }
        c.save_tools();
        c.refresh_tools();
    });
    cb!(b, ctrl, on_pe_cycle_kind, |c, pe_id: i32, n: i32| {
        let r = c.db.get_plan_exercise(pe_id as Id).and_then(|pe| {
            let k = pe.kind_of(n as i64).next();
            c.db.set_plan_set_kind(pe.id, n as i64, k)
        });
        c.report(r);
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_pe_toggle_group, |c, pe_id: i32| {
        let r = c.db.toggle_group_with_next(pe_id as Id);
        if let Err(e) = r {
            c.toast(e.to_string());
        }
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_session_cycle_kind, |c, set_id: i32| {
        let r = c.db.get_session_set(set_id as Id).and_then(|s| c.db.set_session_set_kind(s.id, s.kind.next()));
        match r {
            Ok(()) => {
                let k = c.db.get_session_set(set_id as Id).map(|s| s.kind).unwrap_or_default();
                c.toast(trf!("Tipo da série: {}", k.label()));
            }
            Err(e) => c.toast(e.to_string()),
        }
        let r = c.refresh_session();
        c.report(r);
    });
    cb!(b, ctrl, on_session_repeat_previous, |c, se_id: i32| {
        c.repeat_prev.insert(se_id as Id);
        // Também vale para o modo foco: descarta rascunhos das séries em aberto deste exercício.
        if let Some(sid) = c.session_id {
            if let Ok(f) = c.db.get_session_full(sid) {
                if let Some(e) = f.exercises.iter().find(|e| e.id == se_id as Id) {
                    for s in &e.sets {
                        if s.completed_at.is_none() {
                            c.drafts.remove(&s.id);
                        }
                    }
                }
            }
        }
        c.toast(tr("Valores da sessão anterior preenchidos. Confira e registre cada série."));
        let r = c.refresh_session();
        c.report(r);
    });
    cb!(b, ctrl, on_onboarding_skip, |c| { c.onboarding_finish("explore"); });
    cb!(b, ctrl, on_onboarding_finish, |c, a: SharedString| { c.onboarding_finish(&a); });
    cb!(b, ctrl, on_onboarding_restart, |c| { c.open_onboarding(); });
    cb!(b, ctrl, on_gesture, |c, ctx: SharedString, ev: SharedString| { c.gesture(&ctx, &ev); });
    cb!(b, ctrl, on_home_shortcut, |c, code: SharedString| { c.home_shortcut(&code); });
    cb!(b, ctrl, on_shortcut_slot, |c, i: i32| { c.shortcut_slot(i.max(0) as usize); });
    cb!(b, ctrl, on_shortcuts_reset, |c| {
        let r = c.db.set_pref(shortcuts::PREF_KEY, &shortcuts::serialize(&shortcuts::defaults()));
        c.report(r);
        c.toast(tr("Atalhos padrão restaurados."));
        let r = c.refresh_settings();
        c.report(r);
    });
    cb!(b, ctrl, on_storage_clear_temp, |c| { c.storage_clear_temp(); });
    cb!(b, ctrl, on_backup_local_create, |c| { c.local_backup_create(); });
    cb!(b, ctrl, on_backup_local_restore, |c, i: i32| { c.local_backup_restore(i); });
    cb!(b, ctrl, on_backup_local_delete, |c, i: i32| { c.local_backup_delete(i); });
    cb!(b, ctrl, on_backup_local_export, |c, i: i32| { c.local_backup_export(i); });
    cb!(b, ctrl, on_drag_plan_ex, |c, id: i32, delta: i32| {
        let r = c.db.get_plan_exercise(id as Id).and_then(|pe| {
            let list = c.db.list_plan_exercises(pe.schedule_id)?;
            let idx = list.iter().position(|x| x.id == pe.id).unwrap_or(0) as i64;
            c.db.move_plan_exercise_to(pe.id, (idx + delta as i64).max(0) as usize)
        });
        c.report(r);
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_drag_day, |c, id: i32, delta: i32| {
        let r = c.db.get_day(id as Id).and_then(|d| {
            let list = c.db.list_days(d.plan_id)?;
            let idx = list.iter().position(|x| x.id == d.id).unwrap_or(0) as i64;
            c.db.move_day_to(d.id, (idx + delta as i64).max(0) as usize)
        });
        c.report(r);
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_lib_toggle_archived, |c| {
        c.with_bridge(|b| b.set_lib_show_archived(!b.get_lib_show_archived()));
        let r = c.refresh_library();
        c.report(r);
    });
    cb!(b, ctrl, on_lib_item, |c, id: i32| { c.lib_item(id as Id); });
    cb!(b, ctrl, on_lib_new, |c| { c.open_exercise_form(None); });
    cb!(b, ctrl, on_exf_choose_art, |c| {
        let mut opts = vec![tr("Automática (pelo padrão de movimento)").to_string()];
        opts.extend(crate::exercise_media::ILLUSTRATIONS.iter().map(|(_, l)| tr(l).to_string()));
        let sel = crate::exercise_media::ILLUSTRATIONS
            .iter()
            .position(|(k, _)| *k == c.exf_illustration)
            .map(|i| i as i32 + 1)
            .unwrap_or(0);
        c.ask_choice(ChoiceCtx::Illustration, tr("Ilustração"), tr("Desenhos próprios do EMPETO, disponíveis sem internet."), opts, sel);
    });
    cb!(b, ctrl, on_exd_edit, |c| {
        let id = c.with_bridge(|b| b.get_exd_id()) as Id;
        c.open_exercise_form(Some(id));
    });
    cb!(b, ctrl, on_exd_open_video, |c| { c.exd_open_video(); });
    cb!(b, ctrl, on_exd_evolution, |c| {
        let id = c.with_bridge(|b| b.get_exd_id());
        c.evo_key = format!("id:{}", id);
        c.navigate(P_EVO);
    });
    cb!(b, ctrl, on_exd_favorite_toggle, |c| {
        let id = c.with_bridge(|b| b.get_exd_id()) as Id;
        let r = c.db.get_exercise(id).and_then(|e| c.db.set_exercise_favorite(id, !e.favorite));
        c.report(r);
        let r = c.refresh_exercise_detail();
        c.report(r);
    });
    cb!(b, ctrl, on_exd_open, |c, id: i32| { c.open_exercise_detail(id as Id); });
    cb!(b, ctrl, on_lib_edit, |c, id: i32| { c.open_exercise_form(Some(id as Id)); });
    cb!(b, ctrl, on_lib_archive, |c, _id: i32| {
        if let Some(id) = c.exf_id {
            let archived = c.db.get_exercise(id).map(|e| e.archived).unwrap_or(false);
            c.close_dialog();
            c.ask_confirm(
                ConfirmAction::ArchiveExercise(id, !archived),
                if archived { tr("Reativar exercício") } else { tr("Arquivar exercício") },
                if archived {
                    tr("O exercício volta a aparecer na biblioteca.")
                } else {
                    tr("O exercício deixa de aparecer na biblioteca. O histórico continua com os registros.")
                },
                if archived { tr("Reativar") } else { tr("Arquivar") },
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
            description: c.with_bridge(|b| b.get_exf_description().to_string()),
            notes: c.with_bridge(|b| b.get_exf_notes().to_string()),
            illustration: c.exf_illustration.clone(),
            video_url: c.with_bridge(|b| b.get_exf_video().to_string()),
        });
    });

    // Diálogos
    cb!(b, ctrl, on_confirm_accepted, |c| { c.confirm_accepted(); });
    cb!(b, ctrl, on_dialog_closed, |c| {
        if matches!(c.text, Some(TextCtx::RestorePassword)) {
            if let Some(p) = c.restore_path.take() {
                let _ = std::fs::remove_file(p);
            }
        }
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
        let before = c.db.get_session_set(id as Id).ok();
        match c.db.clear_set(id as Id) {
            Ok(()) => {
                if let Some(bf) = before {
                    c.toast_undo(tr("Registro da série desfeito."), UndoOp::new(UndoKind::ClearSet { set_id: id as Id, before: bf }, tr("Desfazer registro")));
                }
            }
            Err(e) => c.toast(e.to_string()),
        }
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
        if r.is_ok() {
            c.toast_undo(tr("Série removida."), UndoOp::new(UndoKind::RemoveSet { session_exercise_id: id as Id }, tr("Remoção da série")));
        }
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
                    c.toast(tr("Exercício original restaurado nesta sessão."));
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
        c.ask_text(TextCtx::ExerciseNotes(id as Id), tr("Observação do exercício"), "", &notes, true, false);
    });
    cb!(b, ctrl, on_session_edit_notes, |c| {
        let notes = c.session_id.and_then(|sid| c.db.get_session(sid).ok()).map(|s| s.notes).unwrap_or_default();
        c.ask_text(TextCtx::SessionNotes, tr("Observações da sessão"), "", &notes, true, false);
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
            c.toast(tr("Registre pelo menos uma série antes de finalizar, ou descarte a sessão."));
            return;
        }
        if c.prefs.confirm_finish {
            let pending = f.total_sets() - f.completed_sets();
            let text = if pending > 0 {
                trf!("{} série(s) não registrada(s) serão descartadas. Após finalizar, a sessão vira registro histórico e não pode ser alterada.", pending)
            } else {
                tr("Após finalizar, a sessão vira registro histórico e não pode ser alterada.").to_string()
            };
            c.ask_confirm(ConfirmAction::FinishSession, tr("Finalizar sessão"), &text, tr("Finalizar"));
        } else {
            c.finish_session_now();
        }
    });
    cb!(b, ctrl, on_session_discard, |c| {
        c.ask_confirm(
            ConfirmAction::DiscardSession,
            tr("Descartar sessão"),
            tr("Descartar esta sessão? Nada dela será salvo no histórico."),
            tr("Descartar"),
        );
    });
    {
        let ctrl = ctrl.clone();
        b.on_adjust_load(move |text, dir| match ctrl.try_borrow() {
            Ok(c) => ss(c.adjust_load(&text, dir)),
            Err(_) => text,
        });
    }
    {
        let ctrl = ctrl.clone();
        b.on_fill_load(move |set_id, source| match ctrl.try_borrow() {
            Ok(c) => match c.fill_values(set_id as Id, &source) {
                Some((l, _)) => ss(if l > 0 { format_load(l, c.prefs.unit) } else { String::new() }),
                None => ss(""),
            },
            Err(_) => ss(""),
        });
    }
    {
        let ctrl = ctrl.clone();
        b.on_fill_reps(move |set_id, source| match ctrl.try_borrow() {
            Ok(c) => c.fill_values(set_id as Id, &source).map(|(_, r)| ss(r.to_string())).unwrap_or_default(),
            Err(_) => ss(""),
        });
    }
    cb!(b, ctrl, on_toast_action_clicked, |c| { c.undo_last(); });
    cb!(b, ctrl, on_session_focus, |c| { c.open_focus(); });
    cb!(b, ctrl, on_focus_exit, |c| { c.focus_store_draft(); c.back(); });
    cb!(b, ctrl, on_focus_key, |c, k: SharedString| {
        let unit = c.prefs.unit;
        let fresh = std::mem::replace(&mut c.focus_fresh, false);
        let replaces = fresh && k.as_str() != "back";
        c.focus_edit(|cur, kind| input::apply_key(if replaces { "" } else { cur }, &k, kind, unit));
    });
    cb!(b, ctrl, on_focus_step, |c, d: i32| {
        let (unit, step) = (c.prefs.unit, c.prefs.load_step_g);
        c.focus_edit(|cur, kind| input::step(cur, d, kind, unit, step));
    });
    cb!(b, ctrl, on_focus_field_select, |c, f: SharedString| {
        c.focus_fresh = true;
        c.with_bridge(|b| b.set_focus_field(f.clone()));
    });
    cb!(b, ctrl, on_focus_typed, |c, f: SharedString, t: SharedString| {
        c.with_bridge(|b| if f == "reps" { b.set_focus_reps(t.clone()) } else { b.set_focus_load(t.clone()) });
        if let Some(id) = c.focus_set_id() {
            let v = c.with_bridge(|b| (b.get_focus_load().to_string(), b.get_focus_reps().to_string()));
            c.drafts.insert(id, v);
        }
    });
    cb!(b, ctrl, on_focus_fill, |c, src: SharedString| {
        let Some(id) = c.focus_set_id() else { return };
        match c.fill_values(id, &src) {
            Some((l, r)) => {
                let unit = c.prefs.unit;
                c.with_bridge(|b| {
                    b.set_focus_load(ss(if l > 0 { format_load(l, unit) } else { String::new() }));
                    b.set_focus_reps(ss(r.to_string()));
                });
                let v = c.with_bridge(|b| (b.get_focus_load().to_string(), b.get_focus_reps().to_string()));
                c.drafts.insert(id, v);
            }
            None => c.toast(tr("Sem valores para preencher.")),
        }
    });
    cb!(b, ctrl, on_focus_confirm, |c| { c.focus_confirm(); });
    cb!(b, ctrl, on_focus_nav, |c, w: SharedString| {
        c.focus_store_draft();
        c.focus_nav(&w);
    });
    cb!(b, ctrl, on_focus_select_exercise, |c, i: i32| {
        c.focus_store_draft();
        c.with_bridge(|b| b.set_session_index(i));
        if let Some(e) = c.current_session_exercise() {
            c.focus_set = Controller::first_open_set(&e);
        }
        c.refresh_focus();
    });
    cb!(b, ctrl, on_focus_undo_set, |c| {
        let Some(id) = c.focus_set_id() else { return };
        let before = c.db.get_session_set(id).ok();
        match c.db.clear_set(id) {
            Ok(()) => {
                if let Some(bf) = before {
                    c.toast_undo(tr("Registro da série desfeito."), UndoOp::new(UndoKind::ClearSet { set_id: id, before: bf }, tr("Desfazer registro")));
                }
                let r = c.refresh_session();
                c.report(r);
                c.refresh_focus();
            }
            Err(e) => c.toast(e.to_string()),
        }
    });
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
                    c.timer.context = trf!("{} — descanso", cur.name);
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
        c.finalize_rest();
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
            c.finalize_rest();
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
        c.ask_text(TextCtx::TimerCustom, tr("Duração do cronômetro"), tr("Em segundos (ex.: 75) ou minutos:segundos (ex.: 1:15)."), &cur, false, false);
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
    cb!(b, ctrl, on_cal_toggle_heatmap, |c| {
        c.prefs.calendar_heatmap = !c.prefs.calendar_heatmap;
        let on = c.prefs.calendar_heatmap && c.prefs.module_heatmap;
        let _ = c.prefs.save(&c.db);
        c.with_bridge(|b| b.set_cal_heatmap(on));
    });
    cb!(b, ctrl, on_evo_chart_resized, |c, w: f32, h: f32| {
        if (c.evo_chart.0 - w).abs() > 0.5 || (c.evo_chart.1 - h).abs() > 0.5 {
            c.evo_chart = (w.max(10.0), h.max(10.0));
            if c.page() == P_EVO {
                let r = c.refresh_evolution();
                c.report(r);
            }
        }
    });
    cb!(b, ctrl, on_evo_select_point, |c, i: i32| {
        c.evo_selected = if c.evo_selected == i { -1 } else { i };
        let r = c.refresh_evolution();
        c.report(r);
    });
    cb!(b, ctrl, on_progression_accept, |c, id: i32| {
        let r = (|| -> AppResult<()> {
            let s = c.db.progression_for(id as Id, c.prefs.load_step_g, c.prefs.unit)?.ok_or_else(|| AppError::Validation(tr("Sugestão indisponível.").into()))?;
            let pe = c.db.get_plan_exercise(id as Id)?;
            let mut t = pe.target;
            t.load_g = s.suggested_load_g;
            c.db.update_plan_exercise(pe.id, pe.exercise.id, t, pe.rest_seconds, &pe.notes)
        })();
        match r {
            Ok(()) => c.toast(tr("Carga-alvo atualizada na ficha.")),
            Err(e) => c.toast(e.to_string()),
        }
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_progression_dismiss, |c, id: i32| {
        if let Ok(Some(s)) = c.db.progression_for(id as Id, c.prefs.load_step_g, c.prefs.unit) {
            let _ = c.db.set_pref(&format!("progression.dismissed.{}", id), &s.based_on_session.to_string());
        }
        c.report_refresh_editor();
    });
    cb!(b, ctrl, on_open_search, |c| { c.navigate(P_SEARCH); });
    cb!(b, ctrl, on_search_changed, |c, _t: SharedString| { c.refresh_search(); });
    cb!(b, ctrl, on_search_open, |c, i: i32| {
        use crate::application::search::HitKind;
        let Some(h) = c.search_hits.get(i as usize).cloned() else { return };
        match h.kind {
            HitKind::Plan => c.open_plan(h.id),
            HitKind::Session => c.open_detail(h.id),
            HitKind::Exercise => {
                c.open_library(false);
                c.open_exercise_form(Some(h.id));
            }
        }
    });
    cb!(b, ctrl, on_cal_add_to_agenda, |c, i: i32| { c.cal_add_to_agenda(i as usize); });
    cb!(b, ctrl, on_show_diagnostics, |c| { c.show_diagnostics(); });
    cb!(b, ctrl, on_exf_add_alt, |c| {
        let Some(id) = c.exf_id else { return };
        c.close_dialog();
        c.alt_for = Some(id);
        c.lib_pick = true;
        c.with_bridge(|b| b.set_lib_mode(ss("pick")));
        let r = c.refresh_library();
        c.report(r);
        c.toast(tr("Escolha o exercício alternativo."));
    });
    cb!(b, ctrl, on_exf_remove_alt, |c, alt: i32| {
        if let Some(id) = c.exf_id {
            let r = c.db.remove_alternative(id, alt as Id).and_then(|_| c.db.remove_alternative(alt as Id, id));
            c.report(r);
            c.refresh_exf_alternatives();
        }
    });

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
            c.toast(tr("Som e vibração estão desligados."));
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
            tr("Exportar planilha"),
            tr("Escolha o que exportar. Depois você escolhe onde salvar o arquivo."),
            vec![tr("Fichas de treino").into(), tr("Histórico de sessões").into(), tr("Tudo (fichas + histórico)").into()],
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
            Some(st) => {
                let mut v = st.workbook.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
                v.push(tr("Todas as abas (juntar)").into());
                let sel = if st.sheet == usize::MAX { v.len() as i32 - 1 } else { st.sheet as i32 };
                (v, sel)
            }
            None => return,
        };
        c.ask_choice(ChoiceCtx::ImportSheet, tr("Aba da planilha"), "", names, sel);
    });
    cb!(b, ctrl, on_imp_map, |c, col: i32| {
        let Some(st) = c.import.as_ref() else { return };
        let cur = st.mapping.columns.get(col as usize).copied().flatten();
        let mut opts = vec![tr("Ignorar esta coluna").to_string()];
        opts.extend(Field::ALL.iter().map(|f| f.label().to_string()));
        let sel = cur.map(|f| f.index() as i32 + 1).unwrap_or(0);
        c.ask_choice(ChoiceCtx::ImportMap(col as usize), tr("Campo da coluna"), "", opts, sel);
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
