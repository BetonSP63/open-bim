//! ГОСТ tables as data: the rolled-steel specification of the КМ set
//! (ГОСТ 21.502) for the whole model and form 7 specifications of
//! assemblies (ГОСТ Р 21.101-2026, appendix И). The sheets draw the same
//! tables (`open_bim_drawings::tables`); here they feed the release CSV.
use crate::Error;
use open_bim_drawings::tables::{self, RolledSteel, Row};
use open_bim_model::{ObjectId, Project};

/// Rolled-steel specification of every part of the model.
pub fn rolled_steel(project: &Project) -> Result<RolledSteel, Error> {
    let parts: Vec<ObjectId> = project.elements.keys().copied().collect();
    tables::rolled_steel(project, &parts)
}

fn tonnes(value: f64) -> String {
    crate::csv::number(value)
}

/// CSV of the rolled-steel specification: lines, subtotals («Итого»), the
/// total and the totals per grade, masses in tonnes.
pub fn rolled_steel_csv(rs: &RolledSteel) -> String {
    let mut header = vec!["Вид проката", "Марка металла", "Размеры профиля"];
    header.extend(rs.types.iter().copied());
    header.push("Общая масса, т");
    let row = |l: &tables::SteelLine| {
        let mut cells = vec![l.assortment.clone(), l.grade.clone(), l.size.clone()];
        cells.extend(l.masses_t.iter().map(|m| tonnes(*m)));
        cells.push(tonnes(l.total_t));
        cells
    };
    let mut rows = Vec::new();
    for sub in &rs.subtotals {
        rows.extend(
            rs.lines
                .iter()
                .filter(|l| l.assortment == sub.assortment)
                .map(row),
        );
        rows.push(row(sub));
    }
    rows.push(row(&rs.total));
    rows.extend(rs.by_grade.iter().map(|g| {
        let mut cells = row(g);
        cells[0] = "В том числе".into();
        cells
    }));
    crate::csv::table(&header, rows.into_iter())
}

/// CSV of the form 7 specification of an assembly; section headings are
/// rows with the heading in «Наименование».
pub fn assembly_specification_csv(project: &Project, assembly: ObjectId) -> Result<String, Error> {
    let rows = tables::assembly_specification(project, assembly)?;
    Ok(crate::csv::table(
        &[
            "Поз.",
            "Обозначение",
            "Наименование",
            "Кол.",
            "Масса ед., кг",
            "Примечание",
        ],
        rows.into_iter().map(|r| match r {
            Row::Cells(cells) => cells,
            Row::Heading(h) => vec![String::new(), String::new(), h],
        }),
    ))
}
