//! Command-line steel release: assemblies by shop welds, numbering, drawing
//! set and CSV lists from a saved project.
//!
//! Usage: open-bim-release <project.obim> <output-dir> [--auto-assemblies] [--save <out.obim>]
use open_bim_commands::Document;
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("Укажите файл проекта .obim")?;
    let output = PathBuf::from(args.next().ok_or("Укажите папку выпуска")?);
    let mut auto = false;
    let mut save = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--auto-assemblies" => auto = true,
            "--save" => save = Some(args.next().ok_or("--save требует путь")?),
            _ => return Err(format!("Неизвестный аргумент: {arg}")),
        }
    }
    let project = open_bim_persistence::load(&input).map_err(|e| e.to_string())?;
    let mut document = Document::new(project).map_err(|e| e.to_string())?;
    if auto {
        let commands = open_bim_reports::auto_assemblies(document.project())?;
        if !commands.is_empty() {
            document
                .transact("Сборки по заводским швам", &commands)
                .map_err(|e| e.to_string())?;
        }
    }
    let commands = open_bim_reports::release::drawing_commands(document.project())?;
    if !commands.is_empty() {
        document
            .transact("Комплект чертежей", &commands)
            .map_err(|e| e.to_string())?;
    }
    let summary = open_bim_reports::release::export(document.project(), &output)?;
    if let Some(path) = save {
        open_bim_persistence::save_atomic(&path, document.project()).map_err(|e| e.to_string())?;
    }
    println!(
        "Выпущено: {} чертежей, {} файлов; масса деталей {:.3} кг; ревизия {}",
        summary.drawings.len(),
        summary.files.len(),
        summary.total_part_mass_kg,
        summary.revision
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Ошибка: {error}");
        std::process::exit(1);
    }
}
