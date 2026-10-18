//! Synthetic benchmark corpus: N simple steel members (columns and beams on a
//! storey grid, four catalogue sections, three lengths), no connections.
//!
//! Usage: open-bim-synthetic <count> <out.obim>

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let count: usize = args
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|v| (1..=1_000_000).contains(v))
        .ok_or("Укажите число объектов 1…1 000 000")?;
    let path = args.next().ok_or("Укажите файл .obim")?;
    let project = open_bim_desktop::synthetic::corpus(count)?;
    project.validate().map_err(|e| e.to_string())?;
    open_bim_persistence::save_atomic(&path, &project).map_err(|e| e.to_string())?;
    println!("{count} объектов → {path}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Ошибка: {e}");
        std::process::exit(1);
    }
}
