//! Formed views and sheet development regenerated from the same part parameters.
use super::*;
use open_bim_model::SweptPart;

pub(super) fn flat_project(
    project: &Project,
    drawing: &Drawing,
    view: &DrawingView,
) -> Result<Option<Project>, Error> {
    if !view.unfolded {
        return Ok(None);
    }
    let part = match drawing.kind {
        DrawingKind::Part { part } => part,
        DrawingKind::Multi { .. } => view.source.ok_or("Развёртка без детали")?,
        _ => return Err("Развёртка задаётся для отдельной детали".into()),
    };
    if !matches!(
        project.swept_parts.get(&part),
        Some(SweptPart::BentPlate { .. })
    ) {
        return Err("Развёртка доступна для гнутого листа".into());
    }
    let mut stock = project.clone();
    stock.elements.insert(
        part,
        project.fabrication_blank(part).map_err(|e| e.to_string())?,
    );
    stock.swept_parts.remove(&part);
    Ok(Some(stock))
}

pub(super) fn part_drawing(
    project: &Project,
    id: ObjectId,
    part: ObjectId,
    number: &str,
) -> Result<Drawing, Error> {
    let element = project.elements.get(&part).ok_or("Нет детали")?;
    let kind = DrawingKind::Part { part };
    let basis = drawing_basis(project, &kind)?;
    let formed = vec![PartMesh::new(project, part)?];
    let mut definitions = vec![
        (ViewDirection::Front, "Форма детали"),
        (ViewDirection::Top, "Вид сверху"),
        (ViewDirection::Left, "Вид слева"),
    ];
    let mut sizes: Vec<_> = definitions
        .iter()
        .map(|(d, _)| extents(&formed, &basis, *d))
        .collect();
    let flat = matches!(project.swept_parts[&part], SweptPart::BentPlate { .. });
    if flat {
        definitions.push((ViewDirection::Front, "Развёртка до гибки"));
        let SweptPart::BentPlate { width, .. } = project.swept_parts[&part] else {
            unreachable!()
        };
        sizes.push([
            project
                .part_developed_length(part)
                .map_err(|e| e.to_string())?,
            width,
        ]);
    }
    // Four independent cells above the inscription and part specification.
    // Captions, dimensions and bend data have a reserved strip in each cell.
    let cell = [175., 82.];
    let scale = open_bim_model::STANDARD_SCALES
        .iter()
        .copied()
        .filter(|s| *s >= 1.)
        .find(|s| {
            sizes
                .iter()
                .all(|a| a[0] / s <= cell[0] - 32. && a[1] / s <= cell[1] - 38.)
        })
        .ok_or("Деталь не помещается на листе A3")?;
    let positions = [[110., 245.], [295., 245.], [295., 175.], [110., 145.]];
    let mut views = views(&definitions, &positions);
    for (view, size) in views.iter_mut().zip(&sizes) {
        // Keep the top of the geometry fixed in its cell.
        view.position[1] -= size[1] / scale / 2.;
    }
    if flat {
        views[3].unfolded = true;
    }
    let drawing = Drawing {
        id,
        kind,
        number: number.into(),
        title: element.name.clone(),
        sheet: SheetSize::A3,
        scale,
        views,
        dimensions: Vec::new(),
        texts: Vec::new(),
        objects: Vec::new(),
        fingerprints: BTreeMap::new(),
        generated_revision: 0,
        issued_revision: None,
        multiple: 1,
        portrait: false,
    };
    Ok(updated(project, &drawing))
}

fn measurement(value: f64) -> String {
    format!("{value:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .replace('.', ",")
}

fn caption(g: &mut ViewGraphic, at: [f64; 2], text: String) {
    g.labels.push(Label {
        position: at,
        text,
        height: TEXT_HEIGHT,
        align: Align::Middle,
        rotation: 0.,
        object: None,
        lost: false,
    });
}

/// Analytic dimension value, with paper geometry; regenerated on every model update.
fn overall(g: &mut ViewGraphic, a: [f64; 2], b: [f64; 2], value: f64, vertical: bool) {
    let original = [a, b];
    let (a, b) = if vertical {
        ([a[0] - 10., a[1]], [a[0] - 10., b[1]])
    } else {
        ([a[0], a[1] + 10.], [b[0], a[1] + 10.])
    };
    for (start, end) in original.into_iter().zip([a, b]) {
        g.lines.push(Polyline {
            points: vec![start, end],
            style: LineStyle::Thin,
            object: None,
        });
    }
    g.lines.push(Polyline {
        points: vec![a, b],
        style: LineStyle::Thin,
        object: None,
    });
    let dir = if vertical { [0., 1.] } else { [1., 0.] };
    g.lines.push(arrow(a, [-dir[0], -dir[1]]));
    g.lines.push(arrow(b, dir));
    caption(
        g,
        [
            (a[0] + b[0]) / 2. - if vertical { 4. } else { 0. },
            (a[1] + b[1]) / 2. + 2.,
        ],
        measurement(value),
    );
}

pub(super) fn annotate(
    project: &Project,
    drawing: &Drawing,
    view: &DrawingView,
    ctx: &ViewContext,
    g: &mut ViewGraphic,
) -> Result<(), Error> {
    let part = match drawing.kind {
        DrawingKind::Part { part } => part,
        _ => return Ok(()),
    };
    let Some(swept) = project.swept_parts.get(&part) else {
        return Ok(());
    };
    let [lo, hi] = ctx.bounds;
    if !view.appearance.hidden_objects.contains(&part) {
        let a = ctx.paper([lo[0], hi[1]]);
        let b = ctx.paper([hi[0], hi[1]]);
        overall(g, a, b, hi[0] - lo[0], false);
        overall(g, ctx.paper(lo), a, hi[1] - lo[1], true);
    }
    if view.unfolded {
        let SweptPart::BentPlate {
            first_length,
            radius,
            angle,
            ..
        } = swept
        else {
            return Err("Нет развёртки".into());
        };
        let ElementShape::Plate { thickness, .. } = project.elements[&part].shape else {
            return Err("Нет листа".into());
        };
        let allowance = (radius + thickness.value() / 2.) * angle.abs();
        if !view.appearance.hidden_objects.contains(&part) {
            for (x, style) in [
                (first_length, LineStyle::Thin),
                (&(first_length + allowance), LineStyle::Thin),
                (&(first_length + allowance / 2.), LineStyle::DashDot),
            ] {
                g.lines.push(Polyline {
                    points: vec![ctx.paper([*x, lo[1]]), ctx.paper([*x, hi[1]])],
                    style,
                    object: Some(part),
                });
            }
        }
        let y = g.bounds[0][1] - 18.;
        caption(
            g,
            [view.position[0], y],
            format!(
                "Ось сгиба от торца: {} мм; K = 0,5",
                measurement(first_length + allowance / 2.)
            ),
        );
        caption(
            g,
            [view.position[0], y - 5.],
            format!(
                "Зона сгиба: {}…{} мм",
                measurement(*first_length),
                measurement(first_length + allowance)
            ),
        );
    } else if view.id == drawing.views[0].id {
        let text = match swept {
            SweptPart::CurvedBeam {
                radius,
                sweep_angle,
                ..
            } => format!(
                "R оси {}; угол {}°; L оси {} мм",
                measurement(*radius),
                measurement(sweep_angle.to_degrees()),
                measurement(
                    project
                        .part_developed_length(part)
                        .map_err(|e| e.to_string())?
                )
            ),
            SweptPart::PolyBeam {
                bend_radius,
                points,
            } => format!(
                "R оси {}; точек {}; L оси {} мм",
                measurement(*bend_radius),
                points.len(),
                measurement(
                    project
                        .part_developed_length(part)
                        .map_err(|e| e.to_string())?
                )
            ),
            SweptPart::BentPlate {
                radius,
                angle,
                first_length,
                second_length,
                ..
            } => format!(
                "R внутр. {}; угол {}°; прямые {} / {} мм",
                measurement(*radius),
                measurement(angle.to_degrees()),
                measurement(*first_length),
                measurement(*second_length)
            ),
        };
        caption(g, [view.position[0], g.bounds[0][1] - 19.], text);
        let element = &project.elements[&part];
        let section = match element.shape {
            ElementShape::Member { profile, .. } => project.profiles[&profile].name.clone(),
            ElementShape::Plate { thickness, .. } => {
                format!("Лист {} мм", measurement(thickness.value()))
            }
        };
        let mass = project.element_mass_kg(part).map_err(|e| e.to_string())?;
        caption(
            g,
            [view.position[0], g.bounds[0][1] - 24.],
            format!(
                "{}; {}; масса {} кг",
                section,
                project.materials[&element.material].name,
                measurement(mass)
            ),
        );
    }
    Ok(())
}
