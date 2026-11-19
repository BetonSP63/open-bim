use crate::{
    Align, Error, Label, LineStyle, Polyline, ViewContext, ViewGraphic, conventions, tables,
};
use open_bim_model::{
    Drawing, DrawingKind, DrawingView, ObjectId, Project, SheetSize, ViewDirection,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn units(project: &Project, objects: &[ObjectId]) -> BTreeSet<ObjectId> {
    let selected: BTreeSet<_> = objects.iter().copied().collect();
    project
        .assemblies
        .iter()
        .filter(|(id, unit)| unit.is_precast() && (selected.is_empty() || selected.contains(id)))
        .map(|(&id, _)| id)
        .collect()
}

pub(crate) fn marks(
    project: &Project,
    objects: &[ObjectId],
) -> Result<BTreeMap<ObjectId, String>, Error> {
    let units = units(project, objects);
    let mut marks = open_bim_numbering::number(project)
        .map_err(|error| error.to_string())?
        .assemblies;
    marks.retain(|id, _| units.contains(id));
    Ok(marks)
}

pub(crate) fn specification(
    project: &Project,
    objects: &[ObjectId],
) -> Result<tables::Table, Error> {
    let rows = project
        .precast_rows(&marks(project, objects)?)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|row| {
            let note = row.note();
            tables::Row::Cells(vec![
                row.mark,
                row.designation,
                row.name,
                row.count.to_string(),
                tables::mass(row.unit_mass_kg),
                note,
            ])
        })
        .collect();
    Ok(tables::form7(Some("Спецификация сборных элементов"), rows))
}

pub fn precast_drawing(
    project: &Project,
    id: ObjectId,
    mut objects: Vec<ObjectId>,
    number: &str,
) -> Result<Drawing, Error> {
    objects.sort_unstable();
    objects.dedup();
    let table = specification(project, &objects)?;
    if table.rows.is_empty() {
        return Err("В модели нет выбранных сборных элементов".into());
    }
    let kind = DrawingKind::PrecastLayout { objects };
    let sheet = SheetSize::A1;
    let direction = ViewDirection::Top;
    let (scale, positions) = crate::layout(project, &kind, sheet, &[direction])?;
    let views = crate::views(
        &[(direction, "Схема расположения сборных элементов")],
        &positions,
    );
    Ok(crate::updated(
        project,
        &Drawing {
            id,
            kind,
            number: number.into(),
            title: "Схема расположения сборных элементов".into(),
            sheet,
            scale,
            views,
            dimensions: Vec::new(),
            texts: vec![open_bim_model::TextNote {
                id: 1,
                position: [crate::MARGIN_LEFT + 5., crate::MARGIN + 8.],
                text: "Схема по ГОСТ 21.501-2018".into(),
                height: 3.5,
            }],
            objects: Vec::new(),
            fingerprints: BTreeMap::new(),
            generated_revision: 0,
            issued_revision: None,
            multiple: 1,
            portrait: false,
        },
    ))
}

pub(crate) fn annotate(
    project: &Project,
    view: &DrawingView,
    ctx: &ViewContext,
    graphic: &mut ViewGraphic,
    marks: &BTreeMap<ObjectId, String>,
) {
    let low = graphic.bounds[0];
    conventions::axes(
        project,
        view,
        ctx,
        graphic,
        [low[0] - 24., low[1] - 24.],
        true,
    );
    for (along, across) in [(0, 1), (1, 0)] {
        let mut points = Vec::new();
        for axis in project.axes.values() {
            let a = ctx.basis.project(axis.start.0);
            let b = ctx.basis.project(axis.end.0);
            let p = ctx.paper([a[0], a[1]]);
            if (a[along] - b[along]).abs() < 0.01
                && (a[across] - b[across]).abs() > 0.01
                && p[along] >= low[along] - 1.
                && p[along] <= graphic.bounds[1][along] + 1.
            {
                points.push((p[along], a[along]));
            }
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        points.dedup_by(|a, b| (a.1 - b.1).abs() < 0.01);
        let on_chain = |x: f64| {
            let mut p = low;
            p[along] = x;
            p[across] -= 18.;
            p
        };
        for pair in points.windows(2) {
            let [(first, a), (last, b)] = pair else {
                continue;
            };
            let start = on_chain(*first);
            let end = on_chain(*last);
            let mut label = on_chain((first + last) / 2.);
            label[across] += 1.;
            graphic.lines.push(Polyline {
                points: vec![start, end],
                style: LineStyle::Thin,
                object: None,
            });
            for point in [start, end] {
                graphic.lines.push(Polyline {
                    points: vec![
                        [point[0] - 1., point[1] - 1.],
                        [point[0] + 1., point[1] + 1.],
                    ],
                    style: LineStyle::Thin,
                    object: None,
                });
            }
            graphic.labels.push(Label {
                position: label,
                text: format!("{:.0}", (a - b).abs()),
                height: 2.5,
                align: Align::Middle,
                rotation: if along == 0 { 0. } else { 90. },
                object: None,
                lost: false,
            });
        }
    }
    for (id, mark) in marks {
        let part = project.assemblies[id].main_part;
        let Ok([lo, hi]) = project.element_bounds(part) else {
            continue;
        };
        let centre = std::array::from_fn(|axis| (lo[axis] + hi[axis]) / 2.);
        let projected = ctx.basis.project(centre);
        let p = ctx.paper([projected[0], projected[1]]);
        let knee = [p[0] + 5., p[1] + 6.];
        let width = (mark.chars().count() as f64 * conventions::LEVEL_TEXT).max(10.);
        graphic.lines.push(Polyline {
            points: vec![p, knee, [knee[0] + width, knee[1]]],
            style: LineStyle::Thin,
            object: Some(part),
        });
        graphic.labels.push(Label {
            position: [knee[0], knee[1] + 1.],
            text: mark.clone(),
            height: conventions::LEVEL_TEXT,
            align: Align::Start,
            rotation: 0.,
            object: Some(part),
            lost: false,
        });
    }
}
