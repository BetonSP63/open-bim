//! Grid lines as selectable, editable objects («Grids» / «Grid lines»
//! selection switches): screen-space picking, axis name rules of ГОСТ Р
//! 21.101-2026 5.3.2, reading the grid back into the dialog and rebuilding it
//! in one transaction.
use super::tools::{GridSpec, coordinates, elevations, labels, level_labels};
use crate::tools::number;
use open_bim_commands::{Command, DeletePolicy};
use open_bim_model::{GridAxis, Level, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::Projector;

pub use open_bim_model::numbering_rules::{
    FORBIDDEN_CYRILLIC, FORBIDDEN_LATIN, validate_axis_name,
};

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

/// Pixel distance from `cursor` to the projected segment a–b.
fn pixels_to_segment(p: &Projector, a: DVec3, b: DVec3, cursor: [f64; 2]) -> Option<f64> {
    let (a, b) = (p.screen(a)?, p.screen(b)?);
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    let t = if len2 < 1e-12 {
        0.
    } else {
        (((cursor[0] - a[0]) * d[0] + (cursor[1] - a[1]) * d[1]) / len2).clamp(0., 1.)
    };
    let q = [a[0] + d[0] * t, a[1] + d[1] * t];
    Some((q[0] - cursor[0]).hypot(q[1] - cursor[1]))
}

/// The grid line nearest to the cursor within `tolerance` px (the line is
/// drawn at every level, so each copy counts).
pub fn pick_axis(
    project: &Project,
    p: &Projector,
    cursor: [f64; 2],
    tolerance: f64,
) -> Option<ObjectId> {
    project
        .axes
        .values()
        .flat_map(|axis| {
            let (a, b) = (v(axis.start), v(axis.end));
            let mut shifts = vec![0.];
            shifts.extend(project.levels.values().map(|l| l.elevation_mm - a.z));
            shifts.into_iter().filter_map(move |dz| {
                let dz = DVec3::Z * dz;
                pixels_to_segment(p, a + dz, b + dz, cursor).map(|px| (axis.id, px))
            })
        })
        .filter(|(_, px)| *px <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Level under the cursor: its label (drawn left of the level line's start)
/// or the level line itself, except a level at the grid's own elevation,
/// where the line is the grid axis.
pub fn pick_level(
    project: &Project,
    p: &Projector,
    cursor: [f64; 2],
    tolerance: f64,
) -> Option<ObjectId> {
    let grid_z = project.axes.values().next().map(|a| a.start.0[2])?;
    project
        .levels
        .values()
        .filter_map(|level| {
            let (a, b) = open_bim_renderer::overlay::level_line(project, level)?;
            let start = p.screen(a)?;
            let label_w = 8. * level.name.chars().count() as f64 + 8.;
            let on_label = cursor[0] >= start[0] - 30. - label_w
                && cursor[0] <= start[0] - 26.
                && (cursor[1] - start[1]).abs() <= 9.;
            if on_label {
                return Some((level.id, 0.));
            }
            let px = pixels_to_segment(p, a, b, cursor)?;
            ((level.elevation_mm - grid_z).abs() > 1. && px <= tolerance).then_some((level.id, px))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Construction line/circle or model dimension under the cursor.
pub fn pick_annotation(
    project: &Project,
    p: &Projector,
    cursor: [f64; 2],
    tolerance: f64,
) -> Option<ObjectId> {
    let construction = project.construction.values().flat_map(|c| {
        c.segments()
            .into_iter()
            .map(move |(a, b)| (c.id, DVec3::from_array(a), DVec3::from_array(b)))
    });
    let dimensions = project.dimensions.values().filter_map(|d| {
        Some((
            d.id,
            v(project.dim_point(&d.a)?),
            v(project.dim_point(&d.b)?),
        ))
    });
    construction
        .chain(dimensions)
        .filter_map(|(id, a, b)| pixels_to_segment(p, a, b, cursor).map(|px| (id, px)))
        .filter(|(_, px)| *px <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Relative coordinate string of sorted positions: «0 4*6000», «1500 3000 2*6000».
fn spacing_string(mut xs: Vec<f64>) -> String {
    xs.sort_by(f64::total_cmp);
    let fmt = |v: f64| {
        if (v - v.round()).abs() < 1e-6 {
            format!("{}", v.round() as i64)
        } else {
            format!("{v}")
        }
    };
    let mut out = vec![fmt(xs[0])];
    let steps: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    let mut i = 0;
    while i < steps.len() {
        let mut n = 1;
        while i + n < steps.len() && (steps[i + n] - steps[i]).abs() < 1e-6 {
            n += 1;
        }
        out.push(if n > 1 {
            format!("{n}*{}", fmt(steps[i]))
        } else {
            fmt(steps[i])
        });
        i += n;
    }
    out.join(" ")
}

/// X lines are parallel to Y (constant x), Y lines parallel to X.
fn split_axes(project: &Project) -> (Vec<&GridAxis>, Vec<&GridAxis>) {
    let mut xs: Vec<&GridAxis> = project
        .axes
        .values()
        .filter(|a| (a.start.0[0] - a.end.0[0]).abs() < 1e-6)
        .collect();
    let mut ys: Vec<&GridAxis> = project
        .axes
        .values()
        .filter(|a| (a.start.0[1] - a.end.0[1]).abs() < 1e-6)
        .collect();
    xs.sort_by(|a, b| a.start.0[0].total_cmp(&b.start.0[0]));
    ys.sort_by(|a, b| a.start.0[1].total_cmp(&b.start.0[1]));
    (xs, ys)
}

/// The dialog contents describing the existing rectangular grid.
pub fn spec_from_project(project: &Project) -> Option<GridSpec> {
    let (xs, ys) = split_axes(project);
    if xs.is_empty() || ys.is_empty() {
        return None;
    }
    let mut levels: Vec<&Level> = project.levels.values().collect();
    levels.sort_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm));
    let x0 = xs[0].start.0[0];
    let ext = (x0 - ys[0].start.0[0].min(ys[0].end.0[0])).max(0.);
    let fmt_z = |z: f64| format!("{}", z.round() as i64);
    Some(GridSpec {
        x: spacing_string(xs.iter().map(|a| a.start.0[0]).collect()),
        y: spacing_string(ys.iter().map(|a| a.start.0[1]).collect()),
        z: if levels.is_empty() {
            "0".into()
        } else {
            levels
                .iter()
                .map(|l| fmt_z(l.elevation_mm))
                .collect::<Vec<_>>()
                .join(" ")
        },
        labels_x: xs
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        labels_y: ys
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        labels_z: levels
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        extension: format!("{}", ext.round() as i64),
    })
}

/// Rebuild the existing grid from the dialog in one transaction: lines and
/// levels keep their IDs where they still exist, extra ones are removed, new
/// ones created. Parts are not touched.
pub fn update_commands(project: &Project, spec: &GridSpec) -> Result<Vec<Command>, String> {
    let (xs_new, ys_new, zs) = (
        coordinates(&spec.x)?,
        coordinates(&spec.y)?,
        elevations(&spec.z)?,
    );
    let (lx, ly, lz) = (
        labels(&spec.labels_x, xs_new.len())?,
        labels(&spec.labels_y, ys_new.len())?,
        level_labels(&spec.labels_z, &zs)?,
    );
    for name in lx.iter().chain(&ly) {
        validate_axis_name(name)?;
    }
    let ext = number(&spec.extension)?;
    let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let z0 = min(&zs);
    let (xs_old, ys_old) = split_axes(project);
    let mut next = project.next_id;
    let mut commands = Vec::new();
    let mut place = |old: &[&GridAxis], new: Vec<GridAxis>, commands: &mut Vec<Command>| {
        for (i, mut axis) in new.into_iter().enumerate() {
            match old.get(i) {
                Some(o) => {
                    axis.id = o.id;
                    if &axis != *o {
                        commands.push(Command::ReplaceAxis { axis });
                    }
                }
                None => {
                    axis.id = ObjectId(next);
                    next += 1;
                    commands.push(Command::CreateAxis { axis });
                }
            }
        }
    };
    let xlines = xs_new
        .iter()
        .zip(&lx)
        .map(|(x, name)| GridAxis {
            id: ObjectId(0),
            name: name.clone(),
            start: Point3([*x, min(&ys_new) - ext, z0]),
            end: Point3([*x, max(&ys_new) + ext, z0]),
        })
        .collect();
    let ylines = ys_new
        .iter()
        .zip(&ly)
        .map(|(y, name)| GridAxis {
            id: ObjectId(0),
            name: name.clone(),
            start: Point3([min(&xs_new) - ext, *y, z0]),
            end: Point3([max(&xs_new) + ext, *y, z0]),
        })
        .collect();
    place(&xs_old, xlines, &mut commands);
    place(&ys_old, ylines, &mut commands);
    for extra in xs_old
        .iter()
        .skip(xs_new.len())
        .chain(ys_old.iter().skip(ys_new.len()))
    {
        commands.push(Command::Delete {
            id: extra.id,
            policy: DeletePolicy::Cascade,
        });
    }
    // Levels keep their IDs from the bottom up, so columns bound to a level
    // follow its new elevation.
    let mut levels_old: Vec<&Level> = project.levels.values().collect();
    levels_old.sort_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm));
    let mut elevations = std::collections::BTreeMap::new();
    for (i, (z, name)) in zs.iter().zip(lz).enumerate() {
        match levels_old.get(i) {
            Some(old) => {
                let level = Level {
                    id: old.id,
                    name,
                    elevation_mm: *z,
                };
                if &level != *old {
                    elevations.insert(old.id, *z);
                    commands.push(Command::ReplaceLevel { level });
                }
            }
            None => {
                commands.push(Command::CreateLevel {
                    level: Level {
                        id: ObjectId(next),
                        name,
                        elevation_mm: *z,
                    },
                });
                next += 1;
            }
        }
    }
    for extra in levels_old.iter().skip(zs.len()) {
        commands.push(Command::Delete {
            id: extra.id,
            policy: DeletePolicy::Cascade,
        });
    }
    elevations.retain(|id, z| project.levels.get(id).is_some_and(|l| l.elevation_mm != *z));
    commands.extend(crate::tools::column_levels::follow(project, &elevations)?);
    Ok(commands)
}

/// Grid node typed as «Б/2», «2/Б», «Б2» or with a level: «Б/2/+3,600»,
/// «Б/2/3600» or «Б/2/Верх» (level name). `None` when the text is not a
/// grid reference (plain coordinates), `Some(Err)` for unknown axes.
pub fn grid_node(project: &Project, text: &str, plane_z: f64) -> Option<Result<DVec3, String>> {
    let text = text.trim();
    if !text.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    let mut parts: Vec<String> = text.split('/').map(|s| s.trim().to_string()).collect();
    // «Б2» or «Б2/+3,600»: letters directly followed by digits are two axes.
    let first = parts[0].clone();
    if let Some(split) = first.find(|c: char| c.is_ascii_digit())
        && split > 0
        && first[..split].chars().all(|c| c.is_alphabetic())
    {
        let (a, b) = first.split_at(split);
        parts.splice(0..1, [a.to_string(), b.to_string()]);
    }
    if parts.len() == 1 {
        return None;
    }
    if !(2..=3).contains(&parts.len()) {
        return Some(Err("Узел сетки: «Б/2» или «Б/2/+3,600»".into()));
    }
    let (xs, ys) = split_axes(project);
    fn find<'a>(name: &str, lines: &[&'a GridAxis]) -> Option<&'a GridAxis> {
        lines
            .iter()
            .find(|a| a.name.to_uppercase() == name.to_uppercase())
            .copied()
    }
    let (a, b) = (parts[0].as_str(), parts[1].as_str());
    let (x_line, y_line) = match (find(a, &xs), find(b, &ys), find(b, &xs), find(a, &ys)) {
        (Some(x), Some(y), _, _) | (_, _, Some(x), Some(y)) => (x, y),
        _ => {
            let missing = if find(a, &xs).or(find(a, &ys)).is_none() {
                a
            } else {
                b
            };
            return Some(Err(format!("Нет такой оси «{missing}»")));
        }
    };
    let z = match parts.get(2) {
        None => plane_z,
        Some(level) => {
            if let Some(l) = project.levels.values().find(|l| l.name == *level) {
                l.elevation_mm
            } else {
                // «+3,600» (metres, ГОСТ marks) or «3600» (mm).
                let t = level.replace(',', ".");
                match t.parse::<f64>() {
                    Ok(v) if t.contains('.') => v * 1000.,
                    Ok(v) => v,
                    Err(_) => return Some(Err(format!("Нет уровня «{level}»"))),
                }
            }
        }
    };
    Some(Ok(DVec3::new(x_line.start.0[0], y_line.start.0[1], z)))
}
