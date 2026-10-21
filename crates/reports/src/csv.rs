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
