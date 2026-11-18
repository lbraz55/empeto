//! Exportação de fichas e histórico em XLSX (RF-17). Formato documentado em docs/FORMATO_ARQUIVOS.md.
//! A aba "Fichas" usa o mesmo layout aceito pela importação (ida e volta).

use crate::calendar::{date_short, local_date_of, local_time_of};
use crate::domain::*;
use crate::persistence::Db;
use rust_xlsxwriter::{Format, Workbook, Worksheet, XlsxError};
use std::path::Path;

pub const PLAN_HEADERS: [&str; 13] = [
    "Ficha",
    "Dia",
    "Dias da semana",
    "Ordem",
    "Exercício",
    "Grupo muscular",
    "Tipo",
    "Equipamento",
    "Padrão de movimento",
    "Séries",
    "Repetições",
    "Carga (kg)",
    "Descanso (s)",
];

const PLAN_NOTES_HEADER: &str = "Observações";

pub const SESSION_HEADERS: [&str; 16] = [
    "Data",
    "Início",
    "Fim",
    "Ficha",
    "Dia",
    "Exercício planejado",
    "Exercício realizado",
    "Substituição",
    "Série",
    "Reps planejadas",
    "Carga planejada (kg)",
    "Reps realizadas",
    "Carga realizada (kg)",
    "Concluída às",
    "Obs. do exercício",
    "Obs. da sessão",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Plans,
    History,
    All,
}

fn xerr(e: XlsxError) -> AppError {
    AppError::File(format!("Falha ao gerar a planilha: {e}"))
}

fn kg(g: i64) -> f64 {
    g as f64 / 1000.0
}

fn header(ws: &mut Worksheet, cols: &[&str], bold: &Format) -> Result<(), XlsxError> {
    for (c, h) in cols.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, bold)?;
        ws.set_column_width(c as u16, (h.chars().count() as f64 + 4.0).max(12.0))?;
    }
    ws.set_freeze_panes(1, 0)?;
    Ok(())
}

fn write_plans(db: &Db, ws: &mut Worksheet, bold: &Format) -> AppResult<usize> {
    let mut cols: Vec<&str> = PLAN_HEADERS.to_vec();
    cols.push(PLAN_NOTES_HEADER);
    header(ws, &cols, bold).map_err(xerr)?;
    ws.set_name("Fichas").map_err(xerr)?;
    ws.set_column_width(4, 34).map_err(xerr)?;
    let mut row = 1u32;
    for p in db.list_plans_full()? {
        for d in &p.days {
            for (i, pe) in d.exercises.iter().enumerate() {
                let r = row;
                let w = |ws: &mut Worksheet, c: u16, s: &str| ws.write_string(r, c, s).map(|_| ()).map_err(xerr);
                w(ws, 0, &p.plan.name)?;
                w(ws, 1, &d.day.name)?;
                w(ws, 2, &weekdays_text(d.day.weekdays))?;
                ws.write_number(r, 3, (i + 1) as f64).map_err(xerr)?;
                w(ws, 4, &pe.exercise.name)?;
                w(ws, 5, &pe.exercise.muscle_focus)?;
                w(ws, 6, &pe.exercise.exercise_type)?;
                w(ws, 7, &pe.exercise.equipment)?;
                w(ws, 8, &pe.exercise.movement_pattern)?;
                ws.write_number(r, 9, pe.target.sets as f64).map_err(xerr)?;
                w(ws, 10, &format_reps(pe.target.reps_min, pe.target.reps_max).replace('–', "-"))?;
                ws.write_number(r, 11, kg(pe.target.load_g)).map_err(xerr)?;
                ws.write_number(r, 12, pe.rest_seconds as f64).map_err(xerr)?;
                w(ws, 13, &pe.notes)?;
                row += 1;
            }
        }
    }
    Ok(row as usize - 1)
}

fn write_history(db: &Db, ws: &mut Worksheet, bold: &Format) -> AppResult<usize> {
    header(ws, &SESSION_HEADERS, bold).map_err(xerr)?;
    ws.set_name("Sessões").map_err(xerr)?;
    let mut row = 1u32;
    let mut sessions = db.list_session_summaries(None, None)?;
    sessions.reverse();
    for s in sessions {
        let full = db.get_session_full(s.id)?;
        for e in &full.exercises {
            for set in &e.sets {
                let r = row;
                let w = |ws: &mut Worksheet, c: u16, s: &str| ws.write_string(r, c, s).map(|_| ()).map_err(xerr);
                w(ws, 0, &date_short(local_date_of(full.session.started_at)))?;
                w(ws, 1, &local_time_of(full.session.started_at))?;
                w(ws, 2, &full.session.ended_at.map(local_time_of).unwrap_or_default())?;
                w(ws, 3, &full.session.plan_name)?;
                w(ws, 4, &full.session.schedule_name)?;
                w(ws, 5, &e.original_name)?;
                w(ws, 6, &e.performed_name)?;
                w(ws, 7, match e.substitution {
                    SubstitutionType::None => "",
                    SubstitutionType::Temporary => "Temporária",
                    SubstitutionType::Permanent => "Permanente",
                })?;
                ws.write_number(r, 8, set.set_number as f64).map_err(xerr)?;
                w(ws, 9, &format_reps(set.planned_reps_min, set.planned_reps_max).replace('–', "-"))?;
                ws.write_number(r, 10, kg(set.planned_load_g)).map_err(xerr)?;
                if let Some(v) = set.actual_reps {
                    ws.write_number(r, 11, v as f64).map_err(xerr)?;
                }
                if let Some(v) = set.actual_load_g {
                    ws.write_number(r, 12, kg(v)).map_err(xerr)?;
                }
                w(ws, 13, &set.completed_at.map(local_time_of).unwrap_or_default())?;
                w(ws, 14, &e.notes)?;
                w(ws, 15, &full.session.notes)?;
                row += 1;
            }
        }
    }
    Ok(row as usize - 1)
}

fn write_readme(ws: &mut Worksheet, bold: &Format) -> Result<(), XlsxError> {
    ws.set_name("Leia-me")?;
    ws.set_column_width(0, 110)?;
    let lines = [
        "WORKOUT — exportação de dados",
        "",
        "Aba \"Fichas\": uma linha por exercício planejado. Este mesmo formato pode ser importado pelo app",
        "(Configurações > Dados e armazenamento > Importar planilha).",
        "  • Ficha e Dia agrupam os exercícios. Linhas com Ficha/Dia vazios herdam o valor da linha anterior.",
        "  • Dias da semana: Seg, Ter, Qua, Qui, Sex, Sáb, Dom (separados por vírgula). Vazio = ficha em sequência (A/B/C).",
        "  • Repetições: número único (10) ou faixa (8-12). Carga em kg com vírgula ou ponto decimal. Descanso em segundos ou m:ss.",
        "Aba \"Sessões\": histórico de séries realizadas (somente leitura; não é reimportado).",
        "",
        "Backup completo (para restaurar o app) é outro arquivo (.zip), criado em Configurações > Dados e armazenamento.",
    ];
    for (i, l) in lines.iter().enumerate() {
        if i == 0 {
            ws.write_string_with_format(i as u32, 0, *l, bold)?;
        } else {
            ws.write_string(i as u32, 0, *l)?;
        }
    }
    Ok(())
}

/// Gera o arquivo XLSX. Devolve o número de linhas exportadas.
pub fn export_xlsx(db: &Db, kind: ExportKind, path: &Path) -> AppResult<usize> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let mut rows = 0;
    if matches!(kind, ExportKind::Plans | ExportKind::All) {
        let ws = wb.add_worksheet();
        rows += write_plans(db, ws, &bold)?;
    }
    if matches!(kind, ExportKind::History | ExportKind::All) {
        let ws = wb.add_worksheet();
        rows += write_history(db, ws, &bold)?;
    }
    let ws = wb.add_worksheet();
    write_readme(ws, &bold).map_err(xerr)?;
    wb.save(path).map_err(xerr)?;
    Ok(rows)
}

/// Modelo de importação com linhas de exemplo.
pub fn export_template(path: &Path) -> AppResult<()> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    {
        let ws = wb.add_worksheet();
        let mut cols: Vec<&str> = PLAN_HEADERS.to_vec();
        cols.push(PLAN_NOTES_HEADER);
        header(ws, &cols, &bold).map_err(xerr)?;
        ws.set_name("Fichas").map_err(xerr)?;
        ws.set_column_width(4, 34).map_err(xerr)?;
        let rows: [[&str; 14]; 4] = [
            ["Minha ficha", "Treino A", "Seg, Qui", "1", "Supino reto com barra", "Peito", "Composto", "Barra", "Empurrar horizontal", "4", "8-12", "40", "90", ""],
            ["", "", "", "2", "Crucifixo com halteres", "Peito", "Isolado", "Halteres", "Adução horizontal", "3", "12", "12,5", "60", ""],
            ["", "Treino B", "Ter, Sex", "1", "Agachamento livre com barra", "Quadríceps", "Composto", "Barra", "Agachar", "4", "6-8", "60", "120", "Aquecer antes"],
            ["", "", "", "2", "Mesa flexora", "Posterior", "Isolado", "Máquina", "Flexão de joelho", "3", "10-12", "30", "60", ""],
        ];
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                ws.write_string(r as u32 + 1, c as u16, *v).map_err(xerr)?;
            }
        }
    }
    let ws = wb.add_worksheet();
    write_readme(ws, &bold).map_err(xerr)?;
    wb.save(path).map_err(xerr)?;
    Ok(())
}
