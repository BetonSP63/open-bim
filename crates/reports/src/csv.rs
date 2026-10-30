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
