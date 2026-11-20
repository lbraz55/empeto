//! Domínio: entidades, validações e regras independentes de interface e banco.

pub mod input;
pub mod timer;

use crate::i18n::tr;
use serde::{Deserialize, Serialize};
use std::fmt;

pub type Id = i64;

/// Erro de domínio/aplicação com mensagem legível em português.
#[derive(Debug, Clone, PartialEq)]
pub enum AppError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Storage(String),
    File(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Validation(m)
            | AppError::NotFound(m)
            | AppError::Conflict(m)
            | AppError::Storage(m)
            | AppError::File(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        log::error!("sqlite: {e}");
        let msg = e.to_string();
        if msg.contains("historico imutavel") {
            AppError::Conflict(tr("Sessões concluídas fazem parte do histórico e não podem ser alteradas.").into())
        } else if msg.contains("UNIQUE constraint failed: exercise.name") {
            AppError::Conflict(tr("Já existe um exercício com esse nome.").into())
        } else if msg.contains("FOREIGN KEY constraint failed") {
            AppError::Conflict(tr("O registro está em uso e não pode ser removido.").into())
        } else {
            AppError::Storage(trf!("Erro no banco de dados: {}", msg))
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::File(trf!("Erro de arquivo: {}", e.to_string()))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Storage(trf!("Erro de serialização: {}", e.to_string()))
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn validation<T>(msg: impl Into<String>) -> AppResult<T> {
    Err(AppError::Validation(msg.into()))
}

// ---------------------------------------------------------------------------
// Entidades
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleType {
    Weekday,
    Sequence,
}

impl ScheduleType {
    pub fn as_str(self) -> &'static str {
        match self {
            ScheduleType::Weekday => "weekday",
            ScheduleType::Sequence => "sequence",
        }
    }
    pub fn parse(s: &str) -> ScheduleType {
        if s == "weekday" {
            ScheduleType::Weekday
        } else {
            ScheduleType::Sequence
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ScheduleType::Weekday => tr("Dias da semana"),
            ScheduleType::Sequence => tr("Sequência (A/B/C…)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Exercise {
    pub id: Id,
    pub name: String,
    pub muscle_focus: String,
    pub exercise_type: String,
    pub equipment: String,
    pub movement_pattern: String,
    pub archived: bool,
    /// Técnica de execução / descrição (4.5).
    pub description: String,
    /// Observações pessoais, ajustes de equipamento (4.5).
    pub notes: String,
    pub favorite: bool,
    /// Criado pelo usuário (não faz parte da biblioteca inicial).
    pub custom: bool,
    /// Id da ilustração própria (`exercise_media::ILLUSTRATIONS`); vazio = automática.
    #[serde(default)]
    pub illustration: String,
    /// Link externo opcional de vídeo demonstrativo (https), informado pelo usuário.
    #[serde(default)]
    pub video_url: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExerciseInput {
    pub name: String,
    pub muscle_focus: String,
    pub exercise_type: String,
    pub equipment: String,
    pub movement_pattern: String,
    pub description: String,
    pub notes: String,
    pub illustration: String,
    pub video_url: String,
}

impl ExerciseInput {
    pub fn validated(&self) -> AppResult<ExerciseInput> {
        let name = clean_text(&self.name, 80);
        if name.is_empty() {
            return validation(tr("Informe o nome do exercício."));
        }
        Ok(ExerciseInput {
            name,
            muscle_focus: clean_text(&self.muscle_focus, 40),
            exercise_type: clean_text(&self.exercise_type, 40),
            equipment: clean_text(&self.equipment, 40),
            movement_pattern: clean_text(&self.movement_pattern, 40),
            description: clean_notes(&self.description),
            notes: clean_notes(&self.notes),
            illustration: if crate::exercise_media::is_known(self.illustration.trim()) {
                self.illustration.trim().to_string()
            } else {
                String::new()
            },
            video_url: crate::exercise_media::validate_video_url(&self.video_url)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub schedule_type: ScheduleType,
    pub active: bool,
    pub favorite: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanDay {
    pub id: Id,
    pub plan_id: Id,
    pub name: String,
    /// Bitmask: bit 0 = segunda … bit 6 = domingo.
    pub weekdays: u8,
    pub sequence_order: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanExercise {
    pub id: Id,
    pub plan_id: Id,
    pub schedule_id: Id,
    pub exercise: Exercise,
    pub position: i64,
    pub target: Target,
    pub rest_seconds: i64,
    pub notes: String,
}

/// Parâmetros planejados de um exercício.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub sets: i64,
    pub reps_min: i64,
    pub reps_max: i64,
    pub load_g: i64,
}

impl Target {
    pub fn validated(self) -> AppResult<Target> {
        if !(1..=50).contains(&self.sets) {
            return validation(tr("Séries devem estar entre 1 e 50."));
        }
        if !(0..=1000).contains(&self.reps_min) || !(0..=1000).contains(&self.reps_max) {
            return validation(tr("Repetições devem estar entre 0 e 1000."));
        }
        let (a, b) = if self.reps_min <= self.reps_max {
            (self.reps_min, self.reps_max)
        } else {
            (self.reps_max, self.reps_min)
        };
        if !(0..=MAX_LOAD_G).contains(&self.load_g) {
            return validation(tr("Carga fora do intervalo permitido (0 a 2000 kg)."));
        }
        Ok(Target { sets: self.sets, reps_min: a, reps_max: b, load_g: self.load_g })
    }
}

pub const MAX_LOAD_G: i64 = 2_000_000;

pub fn validate_rest(rest: i64) -> AppResult<i64> {
    if !(0..=3600).contains(&rest) {
        return validation(tr("Descanso deve estar entre 0 e 3600 segundos."));
    }
    Ok(rest)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayFull {
    pub day: PlanDay,
    pub exercises: Vec<PlanExercise>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanFull {
    pub plan: Plan,
    pub days: Vec<DayFull>,
}

// ---------------------------------------------------------------------------
// Sessões
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    InProgress,
    Completed,
}

impl SessionStatus {
    pub fn parse(s: &str) -> SessionStatus {
        if s == "completed" {
            SessionStatus::Completed
        } else {
            SessionStatus::InProgress
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstitutionType {
    None,
    Temporary,
    Permanent,
}

impl SubstitutionType {
    pub fn as_str(self) -> &'static str {
        match self {
            SubstitutionType::None => "none",
            SubstitutionType::Temporary => "temporary",
            SubstitutionType::Permanent => "permanent",
        }
    }
    pub fn parse(s: &str) -> SubstitutionType {
        match s {
            "temporary" => SubstitutionType::Temporary,
            "permanent" => SubstitutionType::Permanent,
            _ => SubstitutionType::None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            SubstitutionType::None => "",
            SubstitutionType::Temporary => tr("Substituição só nesta sessão"),
            SubstitutionType::Permanent => tr("Substituição permanente na ficha"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: Id,
    pub plan_id: Option<Id>,
    pub schedule_id: Option<Id>,
    pub plan_name: String,
    pub schedule_name: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub status: SessionStatus,
    pub notes: String,
    pub plan_snapshot_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSet {
    pub id: Id,
    pub set_number: i64,
    pub planned_reps_min: i64,
    pub planned_reps_max: i64,
    pub planned_load_g: i64,
    pub actual_reps: Option<i64>,
    pub actual_load_g: Option<i64>,
    pub completed_at: Option<i64>,
    /// Descanso cumprido após esta série, medido pelo cronômetro (segundos).
    pub rest_taken_s: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionExercise {
    pub id: Id,
    pub session_id: Id,
    pub plan_exercise_id: Option<Id>,
    pub original_exercise_id: Option<Id>,
    pub performed_exercise_id: Option<Id>,
    pub original_name: String,
    pub performed_name: String,
    pub muscle_focus: String,
    pub exercise_type: String,
    pub equipment: String,
    pub substitution: SubstitutionType,
    pub position: i64,
    pub rest_seconds: i64,
    pub notes: String,
    pub completed: bool,
    pub sets: Vec<SessionSet>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionFull {
    pub session: Session,
    pub exercises: Vec<SessionExercise>,
}

/// Estado de um exercício na barra de progresso (3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseProgress {
    NotStarted,
    Partial,
    Done,
}

impl SessionExercise {
    /// Calculado só pelas séries registradas (o cronômetro não conclui exercício).
    pub fn progress(&self) -> ExerciseProgress {
        let done = self.sets.iter().filter(|s| s.completed_at.is_some()).count();
        if done == 0 {
            ExerciseProgress::NotStarted
        } else if done < self.sets.len() {
            ExerciseProgress::Partial
        } else {
            ExerciseProgress::Done
        }
    }
}

impl SessionFull {
    pub fn completed_sets(&self) -> usize {
        self.exercises.iter().flat_map(|e| &e.sets).filter(|s| s.completed_at.is_some()).count()
    }
    pub fn total_sets(&self) -> usize {
        self.exercises.iter().map(|e| e.sets.len()).sum()
    }
    /// Volume realizado em gramas × repetições.
    pub fn volume_g(&self) -> i64 {
        self.exercises
            .iter()
            .flat_map(|e| &e.sets)
            .filter(|s| s.completed_at.is_some())
            .map(|s| s.actual_load_g.unwrap_or(0) * s.actual_reps.unwrap_or(0))
            .sum()
    }
}

/// Cópia fiel da ficha no momento da sessão (RF-07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSnapshot {
    pub snapshot_version: u32,
    pub taken_at: i64,
    pub plan_id: Id,
    pub plan_name: String,
    pub plan_description: String,
    pub schedule_type: ScheduleType,
    pub day_id: Id,
    pub day_name: String,
    pub weekdays: u8,
    pub sequence_order: i64,
    pub exercises: Vec<SnapshotExercise>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotExercise {
    pub plan_exercise_id: Id,
    pub exercise_id: Id,
    pub name: String,
    pub muscle_focus: String,
    pub exercise_type: String,
    pub equipment: String,
    pub movement_pattern: String,
    pub position: i64,
    pub target: Target,
    pub rest_seconds: i64,
    pub notes: String,
    /// Técnica de execução no momento da sessão (preservada no snapshot — 4.5).
    #[serde(default)]
    pub description: String,
}

impl PlanSnapshot {
    pub fn build(plan: &Plan, day: &DayFull, taken_at: i64) -> PlanSnapshot {
        PlanSnapshot {
            snapshot_version: 1,
            taken_at,
            plan_id: plan.id,
            plan_name: plan.name.clone(),
            plan_description: plan.description.clone(),
            schedule_type: plan.schedule_type,
            day_id: day.day.id,
            day_name: day.day.name.clone(),
            weekdays: day.day.weekdays,
            sequence_order: day.day.sequence_order,
            exercises: day
                .exercises
                .iter()
                .map(|pe| SnapshotExercise {
                    plan_exercise_id: pe.id,
                    exercise_id: pe.exercise.id,
                    name: pe.exercise.name.clone(),
                    muscle_focus: pe.exercise.muscle_focus.clone(),
                    exercise_type: pe.exercise.exercise_type.clone(),
                    equipment: pe.exercise.equipment.clone(),
                    movement_pattern: pe.exercise.movement_pattern.clone(),
                    position: pe.position,
                    target: pe.target,
                    rest_seconds: pe.rest_seconds,
                    notes: pe.notes.clone(),
                    description: pe.exercise.description.clone(),
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Unidades e formatação
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadUnit {
    Kg,
    Lb,
}

pub const GRAMS_PER_LB: f64 = 453.592_37;

impl LoadUnit {
    pub fn parse(s: &str) -> LoadUnit {
        if s == "lb" {
            LoadUnit::Lb
        } else {
            LoadUnit::Kg
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            LoadUnit::Kg => "kg",
            LoadUnit::Lb => "lb",
        }
    }
}

/// Converte gramas para texto na unidade escolhida, com vírgula decimal (pt-BR).
pub fn format_load(g: i64, unit: LoadUnit) -> String {
    let value = match unit {
        LoadUnit::Kg => g as f64 / 1000.0,
        LoadUnit::Lb => g as f64 / GRAMS_PER_LB,
    };
    let mut s = format!("{:.2}", value);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s.replace('.', &crate::i18n::decimal_sep().to_string())
}

pub fn format_load_unit(g: i64, unit: LoadUnit) -> String {
    format!("{} {}", format_load(g, unit), unit.as_str())
}

/// Interpreta texto de carga ("40", "42,5", "20kg", "45 lb") e devolve gramas.
pub fn parse_load(text: &str, unit: LoadUnit) -> AppResult<i64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return Ok(0);
    }
    let (num, unit) = if let Some(n) = t.strip_suffix("kg") {
        (n.trim().to_string(), LoadUnit::Kg)
    } else if let Some(n) = t.strip_suffix("lbs").or_else(|| t.strip_suffix("lb")) {
        (n.trim().to_string(), LoadUnit::Lb)
    } else {
        (t.clone(), unit)
    };
    let num = num.replace(',', ".");
    let v: f64 = num.parse().map_err(|_| AppError::Validation(trf!("Carga inválida: \"{}\"", text)))?;
    if !v.is_finite() || v < 0.0 {
        return validation(trf!("Carga inválida: \"{}\"", text));
    }
    let g = match unit {
        LoadUnit::Kg => (v * 1000.0).round() as i64,
        LoadUnit::Lb => (v * GRAMS_PER_LB).round() as i64,
    };
    if g > MAX_LOAD_G {
        return validation(tr("Carga acima do limite (2000 kg)."));
    }
    Ok(g)
}

pub fn format_reps(min: i64, max: i64) -> String {
    if min == max {
        min.to_string()
    } else {
        format!("{}–{}", min, max)
    }
}

/// Interpreta repetições: "10", "8-12", "8 a 12", "12/10/8".
pub fn parse_reps(text: &str) -> AppResult<(i64, i64)> {
    let t = text.trim().to_lowercase().replace(['–', '—'], "-");
    if t.is_empty() {
        return validation(tr("Informe as repetições."));
    }
    let parts: Vec<i64> = t
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<i64>().unwrap_or(-1))
        .collect();
    if parts.is_empty() || parts.iter().any(|&p| !(0..=1000).contains(&p)) {
        return validation(trf!("Repetições inválidas: \"{}\"", text));
    }
    let min = *parts.iter().min().unwrap();
    let max = *parts.iter().max().unwrap();
    Ok((min, max))
}

pub fn format_duration_s(total: i64) -> String {
    let total = total.max(0);
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

/// Interpreta descanso: "90", "90s", "1:30", "1min30", "2 min".
pub fn parse_rest(text: &str) -> AppResult<i64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return Ok(0);
    }
    let secs = if let Some((m, s)) = t.split_once(':') {
        let m: i64 = m.trim().parse().map_err(|_| AppError::Validation(trf!("Descanso inválido: \"{}\"", text)))?;
        let s: i64 = s.trim().parse().map_err(|_| AppError::Validation(trf!("Descanso inválido: \"{}\"", text)))?;
        m * 60 + s
    } else if t.contains("min") || t.ends_with('m') {
        let nums: Vec<i64> = t
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect();
        match nums.as_slice() {
            [m] => m * 60,
            [m, s, ..] => m * 60 + s,
            _ => return validation(trf!("Descanso inválido: \"{}\"", text)),
        }
    } else {
        let n = t.trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace());
        n.trim().parse().map_err(|_| AppError::Validation(trf!("Descanso inválido: \"{}\"", text)))?
    };
    validate_rest(secs)
}

// ---------------------------------------------------------------------------
// Dias da semana
// ---------------------------------------------------------------------------

pub const WEEKDAY_SHORT: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
pub const WEEKDAY_LONG: [&str; 7] =
    ["Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado", "Domingo"];

/// Índice 0 = segunda.
pub fn weekday_bit(index_mon0: u32) -> u8 {
    1u8 << (index_mon0 % 7)
}

pub fn weekdays_text(mask: u8) -> String {
    if mask == 0 {
        return String::new();
    }
    (0..7)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| crate::i18n::tr(WEEKDAY_SHORT[i]))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reconhece nomes de dias da semana em português (e abreviações).
pub fn parse_weekdays(text: &str) -> u8 {
    let t = normalize(text);
    let keys: [(&[&str], u32); 7] = [
        (&["segunda", "seg", "2a", "2ª"], 0),
        (&["terca", "ter", "3a", "3ª"], 1),
        (&["quarta", "qua", "4a", "4ª"], 2),
        (&["quinta", "qui", "5a", "5ª"], 3),
        (&["sexta", "sex", "6a", "6ª"], 4),
        (&["sabado", "sab"], 5),
        (&["domingo", "dom"], 6),
    ];
    let mut mask = 0u8;
    for token in t.split(|c: char| !c.is_alphanumeric() && c != 'ª') {
        if token.is_empty() {
            continue;
        }
        for (names, idx) in keys.iter() {
            if names.iter().any(|n| token == *n) {
                mask |= weekday_bit(*idx);
            }
        }
    }
    mask
}

// ---------------------------------------------------------------------------
// Texto
// ---------------------------------------------------------------------------

/// Remove espaços extras e caracteres de controle; limita o tamanho.
pub fn clean_text(s: &str, max_chars: usize) -> String {
    let collapsed = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.chars().take(max_chars).collect()
}

/// Texto livre (observações): mantém quebras de linha, limita tamanho.
pub fn clean_notes(s: &str) -> String {
    s.chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .take(2000)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Minúsculas e sem acentos — usado em buscas e no reconhecimento de cabeçalhos.
pub fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Estimativa de 1RM (Epley). Apenas indicativa.
pub fn estimated_1rm_g(load_g: i64, reps: i64) -> i64 {
    if reps <= 0 || load_g <= 0 {
        return 0;
    }
    if reps == 1 {
        return load_g;
    }
    (load_g as f64 * (1.0 + reps as f64 / 30.0)).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_round_trip() {
        assert_eq!(parse_load("42,5", LoadUnit::Kg).unwrap(), 42_500);
        assert_eq!(parse_load("20kg", LoadUnit::Lb).unwrap(), 20_000);
        assert_eq!(format_load(42_500, LoadUnit::Kg), "42,5");
        assert_eq!(format_load(40_000, LoadUnit::Kg), "40");
        assert_eq!(parse_load("", LoadUnit::Kg).unwrap(), 0);
        assert!(parse_load("abc", LoadUnit::Kg).is_err());
        assert!(parse_load("-3", LoadUnit::Kg).is_err());
        let g = parse_load("100", LoadUnit::Lb).unwrap();
        assert_eq!(format_load(g, LoadUnit::Lb), "100");
    }

    #[test]
    fn reps_parsing() {
        assert_eq!(parse_reps("10").unwrap(), (10, 10));
        assert_eq!(parse_reps("8-12").unwrap(), (8, 12));
        assert_eq!(parse_reps("8 a 12").unwrap(), (8, 12));
        assert_eq!(parse_reps("12/10/8").unwrap(), (8, 12));
        assert!(parse_reps("x").is_err());
    }

    #[test]
    fn rest_parsing() {
        assert_eq!(parse_rest("90").unwrap(), 90);
        assert_eq!(parse_rest("90s").unwrap(), 90);
        assert_eq!(parse_rest("1:30").unwrap(), 90);
        assert_eq!(parse_rest("1min30").unwrap(), 90);
        assert_eq!(parse_rest("2 min").unwrap(), 120);
        assert!(parse_rest("9999").is_err());
    }

    #[test]
    fn weekdays() {
        assert_eq!(parse_weekdays("Segunda e Quinta"), 0b1001);
        assert_eq!(parse_weekdays("sáb/dom"), 0b110_0000);
        assert_eq!(parse_weekdays("Treino A"), 0);
        assert_eq!(weekdays_text(0b1001), "Seg, Qui");
    }

    #[test]
    fn target_validation() {
        let t = Target { sets: 3, reps_min: 12, reps_max: 8, load_g: 10_000 }.validated().unwrap();
        assert_eq!((t.reps_min, t.reps_max), (8, 12));
        assert!(Target { sets: 0, reps_min: 1, reps_max: 1, load_g: 0 }.validated().is_err());
    }
}
