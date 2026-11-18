//! Importação de planilhas (RF-16, RN-09, RN-10, seção 3.5).
//!
//! Fluxo: ler arquivo (com limites) → detectar cabeçalho/mapeamento → prévia com
//! avisos/erros e colunas não reconhecidas → usuário confirma (pode remapear) →
//! gravação em uma única transação.

use crate::domain::*;
use crate::persistence::Db;
use calamine::{open_workbook_auto, Data, Reader};
use rusqlite::{params, OptionalExtension};
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_SHEETS: usize = 30;
pub const MAX_ROWS: usize = 5_000;
pub const MAX_COLS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Plan,
    Day,
    Weekdays,
    Order,
    Exercise,
    Muscle,
    Type,
    Equipment,
    Pattern,
    Sets,
    Reps,
    Load,
    Rest,
    Notes,
}

impl Field {
    pub const ALL: [Field; 14] = [
        Field::Plan,
        Field::Day,
        Field::Weekdays,
        Field::Order,
        Field::Exercise,
        Field::Muscle,
        Field::Type,
        Field::Equipment,
        Field::Pattern,
        Field::Sets,
        Field::Reps,
        Field::Load,
        Field::Rest,
        Field::Notes,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Plan => "Ficha",
            Field::Day => "Dia",
            Field::Weekdays => "Dias da semana",
            Field::Order => "Ordem",
            Field::Exercise => "Exercício",
            Field::Muscle => "Grupo muscular",
            Field::Type => "Tipo",
            Field::Equipment => "Equipamento",
            Field::Pattern => "Padrão de movimento",
            Field::Sets => "Séries",
            Field::Reps => "Repetições",
            Field::Load => "Carga",
            Field::Rest => "Descanso",
            Field::Notes => "Observações",
        }
    }

    pub fn index(self) -> usize {
        Field::ALL.iter().position(|f| *f == self).unwrap()
    }

    pub fn from_index(i: i64) -> Option<Field> {
        if i < 0 {
            None
        } else {
            Field::ALL.get(i as usize).copied()
        }
    }

    fn exact(self) -> &'static [&'static str] {
        match self {
            Field::Plan => &["ficha", "plano", "programa", "rotina", "nome da ficha", "plan", "workout"],
            Field::Day => &["dia", "dia do treino", "divisao", "sessao", "treino", "letra", "sequencia", "day"],
            Field::Weekdays => &["dias da semana", "dia da semana", "semana", "weekday", "weekdays"],
            Field::Order => &["ordem", "#", "n", "no", "nº", "n°", "posicao", "order"],
            Field::Exercise => &["exercicio", "exercicios", "nome do exercicio", "movimento", "exercise"],
            Field::Muscle => &["grupo muscular", "musculo", "grupo", "musculo foco", "foco", "musculatura", "muscle"],
            Field::Type => &["tipo", "tipo de exercicio", "type"],
            Field::Equipment => &["equipamento", "aparelho", "maquina", "equipment"],
            Field::Pattern => &["padrao", "padrao de movimento", "pattern"],
            Field::Sets => &["series", "serie", "sets", "num series", "n series"],
            Field::Reps => &["repeticoes", "reps", "rep", "repeticao", "faixa de repeticoes", "repetitions"],
            Field::Load => &["carga", "peso", "kg", "carga alvo", "load", "weight"],
            Field::Rest => &["descanso", "intervalo", "pausa", "rest"],
            Field::Notes => &["observacoes", "obs", "observacao", "notas", "comentarios", "notes"],
        }
    }

    fn partial(self) -> &'static [&'static str] {
        match self {
            Field::Plan => &["ficha", "plano"],
            Field::Weekdays => &["semana"],
            Field::Exercise => &["exercic"],
            Field::Muscle => &["muscul"],
            Field::Equipment => &["equipament", "aparelho"],
            Field::Pattern => &["padra"],
            Field::Sets => &["serie"],
            Field::Reps => &["repet"],
            Field::Load => &["carga", "peso"],
            Field::Rest => &["descans", "interval"],
            Field::Notes => &["observ"],
            _ => &[],
        }
    }
}

/// Normaliza cabeçalho: sem acento, minúsculo, sem parênteses/unidade.
fn header_key(s: &str) -> String {
    let n = normalize(s);
    let n = match n.find('(') {
        Some(i) => n[..i].to_string(),
        None => n,
    };
    n.replace(['_', '-', ':', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn recognize_header(s: &str) -> Option<Field> {
    let k = header_key(s);
    if k.is_empty() {
        return None;
    }
    for f in Field::ALL {
        if f.exact().contains(&k.as_str()) {
            return Some(f);
        }
    }
    // "Dia" e "Treino" são ambíguos com "Dias da semana" — testados acima.
    for f in Field::ALL {
        if f.partial().iter().any(|p| k.contains(p)) {
            return Some(f);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetData {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workbook {
    pub sheets: Vec<SheetData>,
}

fn cell_text(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                let mut s = format!("{f:.3}");
                while s.ends_with('0') {
                    s.pop();
                }
                s
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => if *b { "sim".into() } else { "não".into() },
        Data::DateTime(dt) => {
            let v = dt.as_f64();
            if v < 1.0 {
                // "1:30" digitado em Excel vira horário (01:30:00). Para descanso, a intenção
                // usual é minutos:segundos, então gera "1:30" (lido como 90 s).
                let secs = (v * 86_400.0).round() as i64;
                if secs % 60 == 0 {
                    format!("{}:{:02}", secs / 3600, (secs / 60) % 60)
                } else {
                    format!("{}:{:02}", secs / 60, secs % 60)
                }
            } else {
                dt.to_string()
            }
        }
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(_) => String::new(),
    }
}

/// Lê a pasta de trabalho validando extensão, tamanho e estrutura (seção 3.8).
pub fn read_workbook(path: &Path) -> AppResult<Workbook> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::File("Não foi possível ler o arquivo selecionado.".into()))?;
    if meta.len() == 0 {
        return Err(AppError::File("O arquivo está vazio.".into()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::File("Arquivo muito grande (limite de 10 MB).".into()));
    }
    let mut head = [0u8; 4];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(path)?;
        let _ = f.read(&mut head)?;
    }
    let is_zip = head == [0x50, 0x4b, 0x03, 0x04];
    let is_ole = head == [0xd0, 0xcf, 0x11, 0xe0];
    if !is_zip && !is_ole {
        return Err(AppError::File(
            "Formato não reconhecido. Selecione uma planilha .xlsx (Excel), .ods ou .xls.".into(),
        ));
    }
    let mut wb = open_workbook_auto(path).map_err(|e| {
        log::warn!("calamine: {e}");
        AppError::File("Não foi possível abrir a planilha: o arquivo parece corrompido ou não é uma planilha.".into())
    })?;
    let names = wb.sheet_names().to_vec();
    if names.is_empty() {
        return Err(AppError::File("A planilha não contém abas.".into()));
    }
    let mut sheets = Vec::new();
    for name in names.into_iter().take(MAX_SHEETS) {
        let range = match wb.worksheet_range(&name) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("aba {name}: {e}");
                continue;
            }
        };
        if range.height() > MAX_ROWS {
            return Err(AppError::File(format!(
                "A aba \"{name}\" tem mais de {MAX_ROWS} linhas. Divida a planilha em arquivos menores."
            )));
        }
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|r| r.iter().take(MAX_COLS).map(cell_text).collect::<Vec<_>>())
            .collect();
        sheets.push(SheetData { name, rows });
    }
    if sheets.is_empty() {
        return Err(AppError::File("Nenhuma aba pôde ser lida.".into()));
    }
    Ok(Workbook { sheets })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    pub header_row: usize,
    /// Campo associado a cada coluna (None = ignorar).
    pub columns: Vec<Option<Field>>,
}

impl Mapping {
    pub fn column_of(&self, f: Field) -> Option<usize> {
        self.columns.iter().position(|c| *c == Some(f))
    }
    /// Associa a coluna `col` ao campo `f` (removendo a associação anterior desse campo).
    pub fn set(&mut self, col: usize, f: Option<Field>) {
        if let Some(f) = f {
            for c in self.columns.iter_mut() {
                if *c == Some(f) {
                    *c = None;
                }
            }
        }
        if col < self.columns.len() {
            self.columns[col] = f;
        }
    }
}

/// Procura nas 15 primeiras linhas a que mais se parece com um cabeçalho.
pub fn detect_mapping(sheet: &SheetData) -> Mapping {
    let mut best = (0usize, 0usize, Vec::new());
    for (i, row) in sheet.rows.iter().take(15).enumerate() {
        let mut cols: Vec<Option<Field>> = vec![None; row.len()];
        let mut used = std::collections::HashSet::new();
        let mut score = 0;
        for (c, h) in row.iter().enumerate() {
            if let Some(f) = recognize_header(h) {
                if used.insert(f) {
                    cols[c] = Some(f);
                    score += if f == Field::Exercise { 3 } else { 1 };
                }
            }
        }
        if score > best.1 {
            best = (i, score, cols);
        }
    }
    let width = sheet.rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut columns = best.2;
    columns.resize(width, None);
    Mapping { header_row: best.0, columns }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportExercise {
    pub row: usize,
    pub name: String,
    pub muscle: String,
    pub exercise_type: String,
    pub equipment: String,
    pub pattern: String,
    pub target: Target,
    pub rest_seconds: i64,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDay {
    pub name: String,
    pub weekdays: u8,
    pub exercises: Vec<ImportExercise>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    pub name: String,
    pub schedule: ScheduleType,
    pub days: Vec<ImportDay>,
    /// Já existe ficha com esse nome no app.
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportPreview {
    pub sheet_name: String,
    pub plans: Vec<ImportPlan>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub unrecognized_columns: Vec<String>,
    pub data_rows: usize,
    pub valid_rows: usize,
}

impl ImportPreview {
    pub fn exercise_count(&self) -> usize {
        self.plans.iter().flat_map(|p| &p.days).map(|d| d.exercises.len()).sum()
    }
    pub fn can_import(&self) -> bool {
        self.errors.is_empty() && self.valid_rows > 0
    }
    pub fn has_conflicts(&self) -> bool {
        self.plans.iter().any(|p| p.exists)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImportDefaults {
    pub sets: i64,
    pub reps: (i64, i64),
    pub rest_seconds: i64,
    pub unit: LoadUnit,
}

impl Default for ImportDefaults {
    fn default() -> Self {
        ImportDefaults { sets: 3, reps: (10, 10), rest_seconds: 90, unit: LoadUnit::Kg }
    }
}

const MAX_MESSAGES: usize = 200;

fn push_limited(v: &mut Vec<String>, msg: String) {
    if v.len() < MAX_MESSAGES {
        v.push(msg);
    } else if v.len() == MAX_MESSAGES {
        v.push("… outras mensagens omitidas.".into());
    }
}

pub fn build_preview(db: &Db, sheet: &SheetData, mapping: &Mapping, defaults: ImportDefaults) -> ImportPreview {
    let mut p = ImportPreview { sheet_name: sheet.name.clone(), ..Default::default() };
    let header = sheet.rows.get(mapping.header_row).cloned().unwrap_or_default();
    for (c, h) in header.iter().enumerate() {
        if !h.trim().is_empty() && mapping.columns.get(c).copied().flatten().is_none() {
            p.unrecognized_columns.push(h.trim().to_string());
        }
    }
    let Some(ex_col) = mapping.column_of(Field::Exercise) else {
        p.errors.push(
            "Nenhuma coluna foi associada a \"Exercício\". Ajuste o mapeamento: essa coluna é obrigatória.".into(),
        );
        return p;
    };
    for (f, what) in [(Field::Sets, "séries"), (Field::Reps, "repetições")] {
        if mapping.column_of(f).is_none() {
            push_limited(
                &mut p.warnings,
                format!("Coluna de {what} não identificada: será usado o valor padrão para todos os exercícios."),
            );
        }
    }
    let get = |row: &Vec<String>, f: Field| -> String {
        mapping.column_of(f).and_then(|c| row.get(c)).map(|s| s.trim().to_string()).unwrap_or_default()
    };
    let default_plan = clean_text(&sheet.name, 80);
    let mut cur_plan = String::new();
    let mut cur_day = String::new();
    let mut plans: Vec<ImportPlan> = Vec::new();
    let mut rest_defaulted = 0usize;

    for (i, row) in sheet.rows.iter().enumerate().skip(mapping.header_row + 1) {
        let line = i + 1;
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        p.data_rows += 1;
        let plan_cell = get(row, Field::Plan);
        if !plan_cell.is_empty() {
            cur_plan = clean_text(&plan_cell, 80);
        }
        let day_cell = get(row, Field::Day);
        if !day_cell.is_empty() {
            cur_day = clean_text(&day_cell, 60);
        }
        let name = clean_text(row.get(ex_col).map(String::as_str).unwrap_or(""), 80);
        if name.is_empty() {
            push_limited(&mut p.warnings, format!("Linha {line}: sem nome de exercício — linha ignorada."));
            continue;
        }
        let plan_name = if cur_plan.is_empty() { default_plan.clone() } else { cur_plan.clone() };
        let day_name = if cur_day.is_empty() { "Treino A".to_string() } else { cur_day.clone() };

        let sets = match get(row, Field::Sets) {
            s if s.is_empty() => {
                if mapping.column_of(Field::Sets).is_some() {
                    push_limited(&mut p.warnings, format!("Linha {line}: séries não informadas — usado {}.", defaults.sets));
                }
                defaults.sets
            }
            s => match s.replace(',', ".").parse::<f64>() {
                Ok(v) if v >= 1.0 && v <= 50.0 => v.round() as i64,
                _ => match parse_reps(&s) {
                    // "3x10" em uma coluna só
                    Ok((a, _)) if (1..=50).contains(&a) => a,
                    _ => {
                        push_limited(&mut p.warnings, format!("Linha {line}: séries inválidas (\"{s}\") — usado {}.", defaults.sets));
                        defaults.sets
                    }
                },
            },
        };
        let reps = match get(row, Field::Reps) {
            s if s.is_empty() => {
                if mapping.column_of(Field::Reps).is_some() {
                    push_limited(&mut p.warnings, format!("Linha {line}: repetições não informadas — usado {}.", format_reps(defaults.reps.0, defaults.reps.1)));
                }
                defaults.reps
            }
            s => match parse_reps(&s) {
                Ok(r) => r,
                Err(_) => {
                    push_limited(&mut p.warnings, format!("Linha {line}: repetições inválidas (\"{s}\") — usado padrão."));
                    defaults.reps
                }
            },
        };
        let load_g = match get(row, Field::Load) {
            s if s.is_empty() || s == "-" => 0,
            s => match parse_load(&s, defaults.unit) {
                Ok(g) => g,
                Err(_) => {
                    push_limited(&mut p.warnings, format!("Linha {line}: carga inválida (\"{s}\") — usado 0."));
                    0
                }
            },
        };
        let rest = match get(row, Field::Rest) {
            s if s.is_empty() => {
                rest_defaulted += 1;
                defaults.rest_seconds
            }
            s => match parse_rest(&s) {
                Ok(r) => r,
                Err(_) => {
                    push_limited(&mut p.warnings, format!("Linha {line}: descanso inválido (\"{s}\") — usado {} s.", defaults.rest_seconds));
                    defaults.rest_seconds
                }
            },
        };
        let target = match (Target { sets, reps_min: reps.0, reps_max: reps.1, load_g }).validated() {
            Ok(t) => t,
            Err(e) => {
                push_limited(&mut p.warnings, format!("Linha {line}: {e} — linha ignorada."));
                continue;
            }
        };
        let weekdays_cell = get(row, Field::Weekdays);
        let weekdays = if !weekdays_cell.is_empty() { parse_weekdays(&weekdays_cell) } else { parse_weekdays(&day_name) };

        let ex = ImportExercise {
            row: line,
            name,
            muscle: clean_text(&get(row, Field::Muscle), 40),
            exercise_type: clean_text(&get(row, Field::Type), 40),
            equipment: clean_text(&get(row, Field::Equipment), 40),
            pattern: clean_text(&get(row, Field::Pattern), 40),
            target,
            rest_seconds: rest,
            notes: clean_notes(&get(row, Field::Notes)),
        };
        let plan = match plans.iter_mut().position(|x| normalize(&x.name) == normalize(&plan_name)) {
            Some(i) => &mut plans[i],
            None => {
                plans.push(ImportPlan { name: plan_name.clone(), schedule: ScheduleType::Sequence, days: vec![], exists: false });
                plans.last_mut().unwrap()
            }
        };
        let day = match plan.days.iter_mut().position(|d| normalize(&d.name) == normalize(&day_name)) {
            Some(i) => &mut plan.days[i],
            None => {
                plan.days.push(ImportDay { name: day_name.clone(), weekdays: 0, exercises: vec![] });
                plan.days.last_mut().unwrap()
            }
        };
        day.weekdays |= weekdays;
        day.exercises.push(ex);
        p.valid_rows += 1;
    }
    if rest_defaulted > 0 && mapping.column_of(Field::Rest).is_some() {
        push_limited(
            &mut p.warnings,
            format!("{rest_defaulted} linha(s) sem descanso informado — usado {} s.", defaults.rest_seconds),
        );
    }
    for plan in plans.iter_mut() {
        plan.schedule = if !plan.days.is_empty() && plan.days.iter().all(|d| d.weekdays != 0) {
            ScheduleType::Weekday
        } else {
            ScheduleType::Sequence
        };
        plan.exists = db.find_plan_by_name(&plan.name).ok().flatten().is_some();
    }
    if p.valid_rows == 0 && p.errors.is_empty() {
        p.errors.push("Nenhuma linha válida encontrada. Verifique a aba selecionada e o mapeamento.".into());
    }
    if !p.unrecognized_columns.is_empty() {
        let cols = p.unrecognized_columns.join(", ");
        p.warnings.insert(0, format!("Colunas não reconhecidas (serão ignoradas): {cols}."));
    }
    p.plans = plans;
    p
}

/// Junta as prévias de várias abas em uma só. Abas com erro (ex.: sem coluna de exercício)
/// são ignoradas com aviso; fichas e dias de mesmo nome são agrupados.
pub fn merge_previews(parts: Vec<ImportPreview>) -> ImportPreview {
    let mut out = ImportPreview { sheet_name: "todas as abas".into(), ..Default::default() };
    for p in parts {
        if !p.can_import() {
            let why = p.errors.first().cloned().unwrap_or_else(|| "sem linhas válidas".into());
            push_limited(&mut out.warnings, format!("Aba \"{}\" ignorada: {why}", p.sheet_name));
            continue;
        }
        out.data_rows += p.data_rows;
        out.valid_rows += p.valid_rows;
        for w in p.warnings {
            push_limited(&mut out.warnings, format!("[{}] {w}", p.sheet_name));
        }
        for c in p.unrecognized_columns {
            if !out.unrecognized_columns.contains(&c) {
                out.unrecognized_columns.push(c);
            }
        }
        for plan in p.plans {
            match out.plans.iter_mut().find(|x| normalize(&x.name) == normalize(&plan.name)) {
                None => out.plans.push(plan),
                Some(existing) => {
                    for day in plan.days {
                        match existing.days.iter_mut().find(|d| normalize(&d.name) == normalize(&day.name)) {
                            None => existing.days.push(day),
                            Some(d) => {
                                d.weekdays |= day.weekdays;
                                d.exercises.extend(day.exercises);
                            }
                        }
                    }
                    existing.schedule = if existing.days.iter().all(|d| d.weekdays != 0) {
                        ScheduleType::Weekday
                    } else {
                        ScheduleType::Sequence
                    };
                }
            }
        }
    }
    if out.valid_rows == 0 {
        out.errors.push("Nenhuma aba tem linhas válidas para importar.".into());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Cria uma nova ficha com nome diferente (padrão seguro).
    CreateNew,
    /// Substitui a ficha existente (somente com confirmação explícita — RN-09).
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportResult {
    pub plans: usize,
    pub days: usize,
    pub exercises: usize,
    pub new_catalog_exercises: usize,
    pub replaced: usize,
}

/// Grava a prévia confirmada em uma única transação (nada parcial em caso de erro — CT-09).
pub fn apply_import(db: &mut Db, preview: &ImportPreview, policy: ConflictPolicy) -> AppResult<ImportResult> {
    if !preview.can_import() {
        return Err(AppError::Validation("A prévia contém erros. Corrija o mapeamento antes de importar.".into()));
    }
    let now = now_ms();
    let mut res = ImportResult::default();
    let tx = db.conn.transaction()?;
    for plan in &preview.plans {
        let existing: Option<Id> = tx
            .query_row("SELECT id FROM workout_plan WHERE name = ?1 COLLATE NOCASE", params![plan.name], |r| r.get(0))
            .optional()?;
        let mut name = plan.name.clone();
        if let Some(old) = existing {
            match policy {
                ConflictPolicy::Replace => {
                    let busy: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM workout_session WHERE plan_id = ?1 AND status = 'in_progress'",
                        params![old],
                        |r| r.get(0),
                    )?;
                    if busy > 0 {
                        return Err(AppError::Conflict(format!(
                            "A ficha \"{}\" está em uma sessão em andamento e não pode ser substituída.",
                            plan.name
                        )));
                    }
                    tx.execute("DELETE FROM workout_plan WHERE id = ?1", params![old])?;
                    res.replaced += 1;
                }
                ConflictPolicy::CreateNew => {
                    let mut n = 1;
                    loop {
                        name = if n == 1 { format!("{} (importada)", plan.name) } else { format!("{} (importada {n})", plan.name) };
                        let taken: i64 = tx.query_row(
                            "SELECT COUNT(*) FROM workout_plan WHERE name = ?1 COLLATE NOCASE",
                            params![name],
                            |r| r.get(0),
                        )?;
                        if taken == 0 {
                            break;
                        }
                        n += 1;
                    }
                }
            }
        }
        tx.execute(
            "INSERT INTO workout_plan(name, description, schedule_type, active, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            params![name, format!("Importada da aba \"{}\"", preview.sheet_name), plan.schedule.as_str(), now],
        )?;
        let plan_id = tx.last_insert_rowid();
        res.plans += 1;
        for (di, day) in plan.days.iter().enumerate() {
            tx.execute(
                "INSERT INTO plan_schedule(plan_id, name, weekdays, sequence_order) VALUES (?1, ?2, ?3, ?4)",
                params![plan_id, day.name, day.weekdays as i64, di as i64],
            )?;
            let day_id = tx.last_insert_rowid();
            res.days += 1;
            for (pos, ex) in day.exercises.iter().enumerate() {
                let found: Option<(Id, i64)> = tx
                    .query_row(
                        "SELECT id, archived FROM exercise WHERE name = ?1 COLLATE NOCASE",
                        params![ex.name],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
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
                            "INSERT INTO exercise(name, muscle_focus, exercise_type, equipment, movement_pattern, created_at, updated_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                            params![ex.name, ex.muscle, ex.exercise_type, ex.equipment, ex.pattern, now],
                        )?;
                        res.new_catalog_exercises += 1;
                        tx.last_insert_rowid()
                    }
                };
                tx.execute(
                    "INSERT INTO plan_exercise(plan_id, schedule_id, exercise_id, position, target_sets, target_reps_min,
                       target_reps_max, target_load_g, rest_seconds, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        plan_id, day_id, ex_id, pos as i64, ex.target.sets, ex.target.reps_min, ex.target.reps_max,
                        ex.target.load_g, ex.rest_seconds, ex.notes
                    ],
                )?;
                res.exercises += 1;
            }
        }
    }
    tx.commit()?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_headers() {
        assert_eq!(recognize_header("Exercício"), Some(Field::Exercise));
        assert_eq!(recognize_header("Carga (kg)"), Some(Field::Load));
        assert_eq!(recognize_header("Nº de séries"), Some(Field::Sets));
        assert_eq!(recognize_header("Dias da semana"), Some(Field::Weekdays));
        assert_eq!(recognize_header("Dia"), Some(Field::Day));
        assert_eq!(recognize_header("Descanso (s)"), Some(Field::Rest));
        assert_eq!(recognize_header("Cor favorita"), None);
    }

    fn sheet(rows: &[&[&str]]) -> SheetData {
        SheetData {
            name: "Minha ficha".into(),
            rows: rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect(),
        }
    }

    #[test]
    fn preview_groups_days_and_reports_unknown_columns() {
        let db = Db::open_in_memory().unwrap();
        let s = sheet(&[
            &["Planilha de treino", "", "", "", "", ""],
            &["Dia", "Exercício", "Séries", "Reps", "Carga", "Cor"],
            &["Segunda", "Supino reto com barra", "4", "8-12", "40", "azul"],
            &["", "Crucifixo com halteres", "3", "12", "12,5", ""],
            &["Quarta", "Agachamento livre com barra", "4", "6 a 8", "60kg", ""],
            &["", "", "", "", "", ""],
            &["Quarta", "", "3", "10", "", ""],
        ]);
        let m = detect_mapping(&s);
        assert_eq!(m.header_row, 1);
        let p = build_preview(&db, &s, &m, ImportDefaults::default());
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(p.plans.len(), 1);
        assert_eq!(p.plans[0].name, "Minha ficha");
        assert_eq!(p.plans[0].schedule, ScheduleType::Weekday);
        assert_eq!(p.plans[0].days.len(), 2);
        assert_eq!(p.plans[0].days[0].exercises.len(), 2);
        assert_eq!(p.plans[0].days[0].exercises[1].target.load_g, 12_500);
        assert_eq!(p.unrecognized_columns, vec!["Cor".to_string()]);
        assert!(p.warnings.iter().any(|w| w.contains("Linha 7")));
        assert_eq!(p.valid_rows, 3);
    }

    #[test]
    fn merge_sheets() {
        let db = Db::open_in_memory().unwrap();
        let mut a = sheet(&[&["Ficha", "Dia", "Exercício"], &["F", "A", "Prancha"]]);
        a.name = "Aba1".into();
        let mut b = sheet(&[&["Ficha", "Dia", "Exercício"], &["F", "A", "Esteira"], &["F", "B", "Prancha"]]);
        b.name = "Aba2".into();
        let mut c = sheet(&[&["Nada"], &["x"]]);
        c.name = "Lixo".into();
        let parts = [a, b, c]
            .iter()
            .map(|s| build_preview(&db, s, &detect_mapping(s), ImportDefaults::default()))
            .collect();
        let m = merge_previews(parts);
        assert!(m.can_import(), "{:?}", m.errors);
        assert_eq!(m.plans.len(), 1);
        assert_eq!(m.plans[0].days.len(), 2);
        assert_eq!(m.plans[0].days[0].exercises.len(), 2);
        assert!(m.warnings.iter().any(|w| w.contains("Lixo")));
    }

    #[test]
    fn missing_exercise_column_is_error() {
        let db = Db::open_in_memory().unwrap();
        let s = sheet(&[&["Nome", "Séries"], &["Supino", "3"]]);
        let m = detect_mapping(&s);
        let p = build_preview(&db, &s, &m, ImportDefaults::default());
        assert!(!p.can_import());
        let mut m2 = m.clone();
        m2.set(0, Some(Field::Exercise));
        let p2 = build_preview(&db, &s, &m2, ImportDefaults::default());
        assert!(p2.can_import());
    }
}
