//! Release set: one drawing per part and assembly position plus a general
//! arrangement, refreshed from the model, exported with the CSV lists.
use crate::{Error, Reports};
use open_bim_commands::Command;
use open_bim_drawings::Status;
use open_bim_model::{DrawingKind, ObjectId, Project, SheetSize};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

/// Commands creating missing drawings and refreshing outdated ones.
pub fn drawing_commands(project: &Project) -> Result<Vec<Command>, Error> {
    let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
    let mut next = project.next_id;
    let mut commands = Vec::new();
    let covered_parts: BTreeSet<String> = project
        .drawings
        .values()
        .filter_map(|d| match d.kind {
            DrawingKind::Part { part } => numbering.parts.get(&part).cloned(),
            _ => None,
        })
        .collect();
    let covered_assemblies: BTreeSet<String> = project
        .drawings
        .values()
        .filter_map(|d| match d.kind {
            DrawingKind::Assembly { assembly } => numbering.assemblies.get(&assembly).cloned(),
            _ => None,
        })
        .collect();
    let mut seen = BTreeSet::new();
    if !project
        .drawings
        .values()
        .any(|d| matches!(d.kind, DrawingKind::General { .. }))
        && !project.elements.is_empty()
    {
        commands.push(Command::CreateDrawing {
            drawing: open_bim_drawings::general_drawing(
                project,
                ObjectId(next),
                Vec::new(),
                "КМ-1",
                SheetSize::A1,
            )?,
        });
        next += 1;
    }
    for (id, position) in &numbering.assemblies {
        if covered_assemblies.contains(position) || !seen.insert(position.clone()) {
            continue;
        }
        commands.push(Command::CreateDrawing {
            drawing: open_bim_drawings::assembly_drawing(
                project,
                ObjectId(next),
                *id,
                &format!("КМД-{position}"),
            )?,
        });
        next += 1;
    }
    for (id, position) in &numbering.parts {
        if covered_parts.contains(position) || !seen.insert(position.clone()) {
            continue;
        }
        commands.push(Command::CreateDrawing {
            drawing: open_bim_drawings::part_drawing(
                project,
                ObjectId(next),
                *id,
                &format!("КМД-{position}"),
            )?,
        });
        next += 1;
    }
    for drawing in project.drawings.values() {
        if let Status::Outdated { .. } = open_bim_drawings::status(project, drawing) {
            commands.push(Command::ReplaceDrawing {
                drawing: open_bim_drawings::updated(project, drawing),
            });
        }
    }
    Ok(commands)
}

#[derive(Clone, Debug, Serialize)]
pub struct ReleaseSummary {
    pub revision: u64,
    pub drawings: Vec<(String, String)>,
    pub files: Vec<String>,
    pub total_part_mass_kg: f64,
    pub total_assembly_mass_kg: f64,
}

fn safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Export every drawing (PDF per sheet, one combined PDF) and the CSV lists.
/// Drawings with lost bindings stop the release; outdated ones must be refreshed first.
pub fn export(project: &Project, directory: &Path) -> Result<ReleaseSummary, Error> {
    export_with(project, directory, &|_, _| true)
}

/// As `export`, reporting progress per drawing; `progress` returning false
/// cancels. Files are written to a temporary sibling folder and moved into
/// `directory` only on success, so a cancelled release leaves no partial set.
pub fn export_with(
    project: &Project,
    directory: &Path,
    progress: &dyn Fn(usize, usize) -> bool,
) -> Result<ReleaseSummary, Error> {
    let name = directory
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "release".into());
    let staging = directory.with_file_name(format!(".{name}.partial"));
    let _ = std::fs::remove_dir_all(&staging);
    let result = write_set(project, &staging, progress);
    match result {
        Ok(summary) => {
            std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            for file in summary
                .files
                .iter()
                .chain(std::iter::once(&"release.json".to_string()))
            {
                std::fs::rename(staging.join(file), directory.join(file))
                    .map_err(|e| e.to_string())?;
            }
            let _ = std::fs::remove_dir_all(&staging);
            Ok(summary)
        }
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn write_set(
    project: &Project,
    directory: &Path,
    progress: &dyn Fn(usize, usize) -> bool,
) -> Result<ReleaseSummary, Error> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let reports: Reports = crate::generate(project)?;
    let mut sheets = Vec::new();
    let mut summary = ReleaseSummary {
        revision: project.revision,
        drawings: Vec::new(),
        files: Vec::new(),
        total_part_mass_kg: reports.total_part_mass_kg,
        total_assembly_mass_kg: reports.total_assembly_mass_kg,
    };
    let write = |name: String, bytes: &[u8], files: &mut Vec<String>| -> Result<(), Error> {
        std::fs::write(directory.join(&name), bytes).map_err(|e| e.to_string())?;
        files.push(name);
        Ok(())
    };
    let mut drawings: Vec<_> = project.drawings.values().collect();
    drawings.sort_by(|a, b| a.number.cmp(&b.number));
    let total = drawings.len() + 1;
    for (index, drawing) in drawings.into_iter().enumerate() {
        if !progress(index, total) {
            return Err("Выпуск отменён пользователем".into());
        }
        let status = open_bim_drawings::status(project, drawing);
        if status != Status::UpToDate {
            return Err(format!(
                "Комплект не выпущен: {} — {}",
                drawing.number,
                status.label()
            ));
        }
        let sheet = open_bim_drawings::generate(project, drawing)?;
        let pdf =
            open_bim_drawing_export::pdf::render(std::slice::from_ref(&sheet), &drawing.number)?;
        write(
            format!("{}.pdf", safe(&drawing.number)),
            &pdf,
            &mut summary.files,
        )?;
        summary
            .drawings
            .push((drawing.number.clone(), drawing.title.clone()));
        sheets.push(sheet);
    }
    if !progress(total - 1, total) {
        return Err("Выпуск отменён пользователем".into());
    }
    if !sheets.is_empty() {
        let pdf = open_bim_drawing_export::pdf::render(&sheets, &project.name)?;
        write("комплект.pdf".into(), &pdf, &mut summary.files)?;
    }
    write(
        "ведомость-деталей.csv".into(),
        crate::csv::parts(&reports).as_bytes(),
        &mut summary.files,
    )?;
    write(
        "ведомость-сборок.csv".into(),
        crate::csv::assemblies(&reports).as_bytes(),
        &mut summary.files,
    )?;
    if !project.elements.is_empty() {
        let steel = crate::gost_tables::rolled_steel(project)?;
        write(
            "спецификация-металлопроката.csv".into(),
            crate::gost_tables::rolled_steel_csv(&steel).as_bytes(),
            &mut summary.files,
        )?;
    }
    write(
        "крепёж-и-сварка.csv".into(),
        crate::csv::fasteners(&reports).as_bytes(),
        &mut summary.files,
    )?;
    let json = serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?;
    let mut files = summary.files.clone();
    write("release.json".into(), &json, &mut files)?;
    Ok(summary)
}
