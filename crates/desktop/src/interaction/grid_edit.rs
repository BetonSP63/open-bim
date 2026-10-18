//! Grid lines as selectable, editable objects (Tekla «Grids» / «Grid lines»
//! selection switches): screen-space picking, axis name rules of ГОСТ Р
//! 21.101-2026 5.3.2, reading the grid back into the dialog and rebuilding it
//! in one transaction.
use super::tools::{GridSpec, coordinates, elevations, labels};
use crate::tools::number;
use open_bim_commands::{Command, DeletePolicy};
use open_bim_model::{GridAxis, Level, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::Projector;

/// Letters never used for coordination axes (ГОСТ Р 21.101-2026, 5.3.2).
pub const FORBIDDEN_CYRILLIC: &str = "ЕЗЙОХЦЧЩЪЫЬ";
pub const FORBIDDEN_LATIN: &str = "IO";

/// Axis name: digits («12»), one or doubled letters («Б», «ББ»), or an
/// additional axis as a fraction of a main one («1/1», «Б/2»).
pub fn validate_axis_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Имя оси не может быть пустым".into());
    }
    for part in name.split('/') {
        if part.is_empty() {
            return Err(format!("«{name}»: пустая часть дробного обозначения"));
        }
        if part.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        for c in part.chars() {
            if FORBIDDEN_CYRILLIC.contains(c) || FORBIDDEN_LATIN.contains(c) {
                return Err(format!(
                    "Буква «{c}» не применяется для осей (ГОСТ Р 21.101, 5.3.2: исключены {FORBIDDEN_CYRILLIC}, латинские {FORBIDDEN_LATIN})"
                ));
            }
            if !(c.is_alphabetic() && c.is_uppercase()) {
                return Err(format!("«{name}»: допустимы цифры или прописные буквы"));
            }
        }
    }
    Ok(())
}

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
        labels(&spec.labels_z, zs.len())?,
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
    // Levels: replace by delete + create (they carry no dependants).
    for level in project.levels.values() {
        commands.push(Command::Delete {
            id: level.id,
            policy: DeletePolicy::Cascade,
        });
    }
    for (z, name) in zs.iter().zip(lz) {
        commands.push(Command::CreateLevel {
            level: Level {
                id: ObjectId(next),
                name,
                elevation_mm: *z,
            },
        });
        next += 1;
    }
    Ok(commands)
}
