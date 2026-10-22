//! `open-bim-batch macro.json` — run a JSON macro without the interface and
//! print the step report as JSON. Exit code 1 with the failing step on error.
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("Использование: open-bim-batch <макрос.json>");
        return ExitCode::from(2);
    };
    let path = Path::new(path);
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let base = path.parent().unwrap_or(Path::new("."));
    let result = open_bim_api::batch::parse(&text)
        .and_then(|script| open_bim_api::batch::run(&script, base));
    match result {
        Ok((_, report)) => {
            match serde_json::to_string_pretty(&report) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::FAILURE;
                }
            }
            if report.staged > 0 {
                eprintln!(
                    "Внимание: {} изменений не зафиксировано и не сохранено",
                    report.staged
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
