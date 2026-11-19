//! CSV for spreadsheets with Russian regional settings: semicolon
//! separator and decimal comma, CRLF rows, fields with quotes, separators or
//! line breaks quoted and inner quotes doubled (RFC 4180); UTF-8 with BOM so
//! Cyrillic opens correctly.
use crate::{AssemblyRow, FastenerRow, PartRow, Reports, fmt};

pub fn field(value: &str) -> String {
    if value.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.into()
    }
}

pub(crate) fn table(header: &[&str], rows: impl Iterator<Item = Vec<String>>) -> String {
    let mut out = String::from("\u{feff}");
    let line =
        |cells: Vec<String>| cells.iter().map(|c| field(c)).collect::<Vec<_>>().join(";") + "\r\n";
    out.push_str(&line(header.iter().map(|h| (*h).to_string()).collect()));
    for row in rows {
        out.push_str(&line(row));
    }
    out
}

/// A number cell with a decimal comma.
pub(crate) fn number(value: f64) -> String {
    format!("{:.3}", value).replace('.', ",")
}

/// A length cell: whole millimetres, else one decimal, with a comma.
fn length(value: f64) -> String {
    fmt(value).replace('.', ",")
}

pub fn parts(r: &Reports) -> String {
    let total = vec![
        "Итого".into(),
        String::new(),
        String::new(),
        String::new(),
        r.parts
            .iter()
            .map(|p| p.quantity)
            .sum::<usize>()
            .to_string(),
        String::new(),
        String::new(),
        number(r.total_part_mass_kg),
    ];
    table(
        &[
            "Позиция",
            "Наименование",
            "Сечение",
            "Материал",
            "Кол-во",
            "Длина, мм",
            "Масса ед., кг",
            "Масса общ., кг",
        ],
        r.parts
            .iter()
            .map(|p: &PartRow| {
                vec![
                    p.position.clone(),
                    p.name.clone(),
                    p.section.clone(),
                    p.material.clone(),
                    p.quantity.to_string(),
                    length(p.length_mm),
                    number(p.unit_mass_kg),
                    number(p.total_mass_kg),
                ]
            })
            .chain(std::iter::once(total)),
    )
}

pub fn assemblies(r: &Reports) -> String {
    let total = vec![
        "Итого".into(),
        String::new(),
        String::new(),
        r.assemblies
            .iter()
            .map(|a| a.quantity)
            .sum::<usize>()
            .to_string(),
        String::new(),
        String::new(),
        number(r.total_assembly_mass_kg),
    ];
    table(
        &[
            "Марка",
            "Наименование",
            "Главная деталь",
            "Кол-во",
            "Деталей в сборке",
            "Масса ед., кг",
            "Масса общ., кг",
        ],
        r.assemblies
            .iter()
            .map(|a: &AssemblyRow| {
                vec![
                    a.position.clone(),
                    a.name.clone(),
                    a.main_part.clone(),
                    a.quantity.to_string(),
                    a.parts_per_assembly.to_string(),
                    number(a.unit_mass_kg),
                    number(a.total_mass_kg),
                ]
            })
            .chain(std::iter::once(total)),
    )
}

pub fn fasteners(r: &Reports) -> String {
    table(
        &["Тип", "Наименование", "Кол-во", "Длина швов, мм"],
        r.fasteners.iter().map(|f: &FastenerRow| {
            vec![
                f.kind.clone(),
                f.description.clone(),
                f.quantity.to_string(),
                length(f.length_mm),
            ]
        }),
    )
}

pub fn coatings(r: &Reports) -> String {
    table(
        &["Обработка поверхности", "Кол-во деталей", "Площадь, м²"],
        r.coatings.iter().map(|c| {
            vec![
                c.treatment.clone(),
                c.quantity.to_string(),
                format!("{:.3}", c.area_m2),
            ]
        }),
    )
}

pub fn welds(r: &Reports) -> String {
    table(
        &["№ шва", "Обозначение", "Длина, мм", "Детали", "Шов"],
        r.welds.iter().map(|w| {
            vec![
                w.number.clone(),
                w.designation.clone(),
                length(w.length_mm),
                w.parts.clone(),
                if w.shop {
                    "заводской"
                } else {
                    "монтажный"
                }
                .into(),
            ]
        }),
    )
}

/// «Ведомость объёмов бетона по захваткам»: one line per element kind of a
/// zone, the zone total, then the totals by concrete class.
pub fn pours(project: &open_bim_model::Project) -> String {
    let zones = project.pour_zones();
    let mut rows = Vec::new();
    for z in &zones {
        for l in project.pour_rows(std::slice::from_ref(z)) {
            rows.push(vec![
                l.zone.to_string(),
                l.grade,
                l.element,
                l.count.to_string(),
                number(l.volume_m3),
            ]);
        }
        rows.push(vec![
            z.number.to_string(),
            z.grade.clone(),
            "Итого по захватке".into(),
            String::new(),
            number(z.volume_m3),
        ]);
    }
    for (grade, v) in open_bim_model::pours::volumes_by_grade(&zones) {
        rows.push(vec![
            String::new(),
            grade,
            "Всего".into(),
            String::new(),
            number(v),
        ]);
    }
    table(
        &["Захватка", "Класс бетона", "Элемент", "Кол-во", "Объём, м³"],
        rows.into_iter(),
    )
}

/// «Ведомость деталей» of reinforcing bars.
pub fn rebar_details(project: &open_bim_model::Project) -> String {
    let rows = open_bim_model::rebar::detail_schedule(project);
    let total_mass: f64 = rows.iter().map(|r| r.total_mass_kg).sum();
    let total_count: usize = rows.iter().map(|r| r.count).sum();
    let mut body: Vec<Vec<String>> = rows.iter().map(detail_cells).collect();
    body.push(vec![
        "Итого".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        total_count.to_string(),
        String::new(),
        number(total_mass),
    ]);
    table(
        &[
            "Позиция",
            "Форма",
            "Диаметр, мм",
            "Класс",
            "Стандарт",
            "Участки",
            "Крюки",
            "Длина, мм",
            "Кол-во",
            "Масса ед., кг",
            "Масса, кг",
        ],
        body.into_iter(),
    )
}

/// «Ведомость расхода стали» by element, then totals by class.
pub fn rebar_steel(project: &open_bim_model::Project) -> String {
    let steel = open_bim_model::rebar::steel_schedule(project);
    let mut rows: Vec<Vec<String>> = steel.by_element.iter().map(steel_cells).collect();
    for row in &steel.by_class {
        let mut cells = steel_cells(row);
        cells[0] = "Всего".into();
        rows.push(cells);
    }
    rows.push(vec![
        "Итого".into(),
        String::new(),
        String::new(),
        steel.total_bars.to_string(),
        number(steel.total_length_m),
        number(steel.total_kg),
    ]);
    table(
        &[
            "Элемент",
            "Класс",
            "Диаметр, мм",
            "Стержней",
            "Длина, м",
            "Масса, кг",
        ],
        rows.into_iter(),
    )
}

/// Writes both rebar schedules. Empty when the model has no bars.
pub fn write_rebar(
    project: &open_bim_model::Project,
    dir: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, String> {
    if project.concrete.rebars.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for (name, text) in [
        ("ведомость-деталей-арматуры.csv", rebar_details(project)),
        ("ведомость-расхода-стали.csv", rebar_steel(project)),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        files.push(path);
    }
    Ok(files)
}

fn detail_cells(row: &open_bim_model::rebar::DetailRow) -> Vec<String> {
    vec![
        row.mark.to_string(),
        row.code.clone(),
        fmt(row.diameter),
        row.grade.clone(),
        row.standard.clone(),
        segment_text(row),
        hook_text(&row.hooks),
        fmt(row.length_mm),
        row.count.to_string(),
        number(row.unit_mass_kg),
        number(row.total_mass_kg),
    ]
}

fn steel_cells(row: &open_bim_model::rebar::SteelRow) -> Vec<String> {
    vec![
        row.element.clone(),
        row.grade.clone(),
        fmt(row.diameter),
        row.bars.to_string(),
        number(row.length_m),
        number(row.mass_kg),
    ]
}

fn segment_text(row: &open_bim_model::rebar::DetailRow) -> String {
    let mut out: Vec<(String, f64)> = Vec::new();
    for seg in &row.segments {
        if let Some((letter, value)) = out.last()
            && letter == &seg.letter
            && (*value - seg.length_mm).abs() < 0.5
        {
            continue;
        }
        out.push((seg.letter.clone(), seg.length_mm));
    }
    out.iter()
        .map(|(letter, value)| format!("{letter}={}", fmt(*value)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn hook_text(hooks: &[open_bim_model::Hook; 2]) -> String {
    use open_bim_model::rebar::HookKind;
    if hooks.iter().all(open_bim_model::Hook::is_none) {
        return "—".into();
    }
    hooks
        .iter()
        .map(|h| match h.kind {
            HookKind::None => "—",
            HookKind::Deg90 => "90°",
            HookKind::Deg135 => "135°",
            HookKind::Deg180 => "180°",
        })
        .collect::<Vec<_>>()
        .join("/")
}
