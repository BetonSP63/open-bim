//! Starter bars («выпуски»): straight or L bars spaced along a host face,
//! a slab edge, or up from a column into the slab or foundation above.
//! Each leg is the lap length `l_l` of СП 63.13330 (the anchorage
//! calculation of `l_an`, with the lap factors). The set references its
//! host and is built again when that host changes.
use std::collections::BTreeMap;

use crate::concrete::{RebarCheck, RebarKind, ShapePlace, StarterLayout, StarterPath};
use crate::{ElementShape, Frame, ModelError, ObjectId, Point3, Project, RebarSet};

use super::shape::{instantiate, resolve_shape};
use super::{
    REBAR_CLASSES, Stress, anchorage, host_cover, host_rbt, rebar_class, rect_member, spread,
};

/// Place starter bars on `host`. A set already on the same host and path is
/// replaced.
pub fn install_starter(
    project: &Project,
    host: ObjectId,
    path: StarterPath,
    shape_id: &str,
    diameter: f64,
    grade: &str,
    spacing: f64,
) -> Result<crate::Concrete, String> {
    if !(3.0..=60.).contains(&diameter) {
        return Err("Диаметр арматуры от 3 до 60 мм".into());
    }
    if grade.trim().is_empty() {
        return Err("Класс арматуры не задан".into());
    }
    if !(diameter..=100_000.).contains(&spacing) {
        return Err("Шаг выпусков не меньше диаметра и не больше 100 000 мм".into());
    }
    let mut set = RebarSet::basic(RebarKind::Starter, diameter, grade);
    set.host = Some(host);
    set.shape_id = Some(shape_id.to_string());
    set.starter = Some(StarterLayout {
        path,
        spacing,
        into: None,
        checks: Vec::new(),
    });
    let laid = lay(project, &set)?;
    apply(&mut set, laid);
    let mut concrete = project.concrete.clone();
    let path = set.starter.as_ref().map(|s| s.path.clone());
    set.number = concrete
        .rebars
        .iter()
        .find(|r| {
            r.host == Some(host)
                && r.kind == RebarKind::Starter
                && r.starter.as_ref().map(|s| &s.path) == path.as_ref()
        })
        .map(|r| r.number)
        .unwrap_or_else(|| concrete.rebars.iter().map(|r| r.number).max().unwrap_or(0) + 1);
    concrete.rebars.retain(|r| r.number != set.number);
    concrete.rebars.push(set);
    concrete.rebars.sort_by_key(|r| r.number);
    let mut staged = project.clone();
    staged.concrete = concrete;
    super::schedule::write_positions(&mut staged);
    staged.concrete.validate().map_err(|e| e.to_string())?;
    Ok(staged.concrete)
}

pub(super) fn rebuild_starters(
    project: &Project,
    _previous: &Project,
    sets: &mut [RebarSet],
) -> Result<(), ModelError> {
    for set in sets.iter_mut() {
        if set.starter.is_none() {
            continue;
        }
        match lay(project, set) {
            Ok(laid) => apply(set, laid),
            Err(e) if !set.bars.is_empty() => {
                if let Some(starter) = &mut set.starter {
                    starter.checks = vec![RebarCheck {
                        message: e,
                        opening: None,
                        edge: None,
                        from: None,
                        to: None,
                    }];
                }
            }
            Err(e) => {
                return Err(ModelError::InvalidValue(format!(
                    "Выпуски №{}: {e}",
                    set.number
                )));
            }
        }
    }
    Ok(())
}

struct Laid {
    bars: Vec<Vec<Point3>>,
    hooks: [crate::Hook; 2],
    mandrel: Option<f64>,
    params: BTreeMap<String, f64>,
    into: Option<ObjectId>,
}

fn apply(set: &mut RebarSet, laid: Laid) {
    set.bars = laid.bars;
    set.hooks = laid.hooks;
    set.bend_diameter = laid.mandrel;
    set.shape_params = laid.params;
    if let Some(starter) = &mut set.starter {
        starter.into = laid.into;
        starter.checks.clear();
    }
}

fn lay(project: &Project, set: &RebarSet) -> Result<Laid, String> {
    let host = set.host.ok_or("Выпуск не привязан к детали")?;
    let starter = set.starter.as_ref().ok_or("У выпуска нет пути")?;
    let shape_id = set.shape_id.as_deref().unwrap_or("gost.straight");
    let shape = resolve_shape(project, shape_id).ok_or("Неизвестный тип формы выпуска")?;
    let straight = shape.segments.len() == 1 && shape.segments[0].bend_deg.abs() < 1.;
    let elbow = shape.segments.len() == 2 && (shape.segments[0].bend_deg.abs() - 90.).abs() < 1.;
    if !straight && !elbow {
        return Err("Выпуск — прямой или Г-образный стержень".into());
    }
    let lap = lap_length(project, host, set.diameter, &set.grade);
    if lap < 1. {
        return Err("Длина нахлёстки выпуска не задана".into());
    }
    let mut params = BTreeMap::new();
    params.insert(shape.segments[0].letter.clone(), lap);
    if elbow {
        params.insert(shape.segments[1].letter.clone(), lap);
    }
    let cover = host_cover(project, host);
    let (frame, spots, axis_x, axis_y, into) = match &starter.path {
        StarterPath::Face { index } => {
            let (frame, width, height, length) = rect_member(project, host)?;
            let spots = face_spots(
                *index,
                width,
                height,
                length,
                cover,
                set.diameter,
                starter.spacing,
            )?;
            (frame, spots, face_out(*index)?, [0., 0., 1.], None)
        }
        StarterPath::ColumnTop => {
            let (frame, width, height, length) = rect_member(project, host)?;
            let spots = column_spots(width, height, length, cover, set.diameter, starter.spacing)?;
            let top = frame.world(Point3([0., 0., length]));
            (
                frame,
                spots,
                [0., 0., 1.],
                [1., 0., 0.],
                slab_above(project, top),
            )
        }
        StarterPath::SlabEdge { index } => {
            let (frame, contour, thickness) = plate(project, host)?;
            let spots = edge_spots(
                &contour,
                *index,
                thickness,
                cover,
                set.diameter,
                starter.spacing,
            )?;
            (
                frame,
                spots,
                edge_out(&contour, *index)?,
                [0., 0., 1.],
                None,
            )
        }
    };
    let bend_in = matches!(starter.path, StarterPath::ColumnTop) && elbow;
    let mut bars = Vec::new();
    let mut hooks = shape.hooks;
    let mut mandrel = None;
    for spot in spots {
        let y_axis = if bend_in {
            inward_xy(spot[0], spot[1])
        } else {
            axis_y
        };
        let place = ShapePlace {
            origin: frame.world(Point3(spot)),
            x: direction(&frame, axis_x),
            y: direction(&frame, y_axis),
            local_to_host: false,
        };
        let made = instantiate(&shape, &params, set.diameter, &set.grade, &place)?;
        hooks = made.hooks;
        mandrel = made.mandrel;
        bars.push(made.bar);
    }
    if bars.is_empty() {
        return Err("На пути нет места для выпуска".into());
    }
    Ok(Laid {
        bars,
        hooks,
        mandrel,
        params,
        into,
    })
}

/// Lap length `l_l` of a bar in tension in the concrete of `host`, mm.
pub fn lap_length(project: &Project, host: ObjectId, diameter: f64, grade: &str) -> f64 {
    let class = rebar_class(grade).unwrap_or(&REBAR_CLASSES[2]);
    anchorage(
        class,
        diameter,
        host_rbt(project, host),
        1.,
        Stress::Tension,
    )
    .lap
}

fn face_out(index: u32) -> Result<[f64; 3], String> {
    Ok(match index {
        0 => [0., -1., 0.],
        1 => [1., 0., 0.],
        2 => [0., 1., 0.],
        3 => [-1., 0., 0.],
        _ => return Err("Нет такой грани".into()),
    })
}

fn face_spots(
    index: u32,
    width: f64,
    height: f64,
    length: f64,
    cover: f64,
    diameter: f64,
    spacing: f64,
) -> Result<Vec<[f64; 3]>, String> {
    let _ = face_out(index)?;
    if length <= 2. * cover + 1. {
        return Err("Длина меньше защитного слоя".into());
    }
    let (x, y) = face_point(index, width, height, diameter)?;
    Ok(spread(cover, length - cover, spacing)
        .into_iter()
        .map(|z| [x, y, z])
        .collect())
}

fn face_point(index: u32, width: f64, height: f64, diameter: f64) -> Result<(f64, f64), String> {
    let d = diameter / 2.;
    if width <= diameter || height <= diameter {
        return Err("Сечение меньше защитного слоя".into());
    }
    Ok(match index {
        0 => (0., -height / 2. + d),
        1 => (width / 2. - d, 0.),
        2 => (0., height / 2. - d),
        3 => (-width / 2. + d, 0.),
        _ => return Err("Нет такой грани".into()),
    })
}

fn column_spots(
    width: f64,
    height: f64,
    length: f64,
    cover: f64,
    diameter: f64,
    spacing: f64,
) -> Result<Vec<[f64; 3]>, String> {
    let inset = cover + diameter / 2.;
    let x0 = -width / 2. + inset;
    let x1 = width / 2. - inset;
    let y0 = -height / 2. + inset;
    let y1 = height / 2. - inset;
    if x1 - x0 < 1. || y1 - y0 < 1. {
        return Err("Сечение меньше защитного слоя".into());
    }
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut spots = Vec::new();
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let steps = (len / spacing).floor().max(1.) as usize;
        for step in 0..steps {
            let t = step as f64 * spacing;
            if t > len - 0.5 {
                break;
            }
            let u = t / len;
            spots.push([a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, length]);
        }
    }
    Ok(spots)
}

fn plate(project: &Project, host: ObjectId) -> Result<(Frame, Vec<[f64; 2]>, f64), String> {
    let element = project.elements.get(&host).ok_or("Деталь не найдена")?;
    match &element.shape {
        ElementShape::Plate {
            contour,
            thickness,
            frame,
        } => Ok((*frame, contour.clone(), thickness.value())),
        _ => Err("Выберите плиту".into()),
    }
}

fn edge_spots(
    contour: &[[f64; 2]],
    index: u32,
    thickness: f64,
    cover: f64,
    diameter: f64,
    spacing: f64,
) -> Result<Vec<[f64; 3]>, String> {
    let (a, b) = edge_ends(contour, index)?;
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    if len <= 2. * cover + 1. {
        return Err("Кромка короче защитного слоя".into());
    }
    let dir = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let outward = edge_normal(contour, dir);
    let inward = [-outward[0], -outward[1]];
    let z = thickness / 2.;
    Ok(spread(cover, len - cover, spacing)
        .into_iter()
        .map(|t| {
            [
                a[0] + dir[0] * t + inward[0] * diameter / 2.,
                a[1] + dir[1] * t + inward[1] * diameter / 2.,
                z,
            ]
        })
        .collect())
}

fn edge_out(contour: &[[f64; 2]], index: u32) -> Result<[f64; 3], String> {
    let (a, b) = edge_ends(contour, index)?;
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    let dir = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let n = edge_normal(contour, dir);
    Ok([n[0], n[1], 0.])
}

fn edge_ends(contour: &[[f64; 2]], index: u32) -> Result<([f64; 2], [f64; 2]), String> {
    if contour.is_empty() || index as usize >= contour.len() {
        return Err("Нет такой кромки".into());
    }
    let i = index as usize;
    Ok((contour[i], contour[(i + 1) % contour.len()]))
}

fn edge_normal(contour: &[[f64; 2]], dir: [f64; 2]) -> [f64; 2] {
    let right = [dir[1], -dir[0]];
    if signed_area(contour) >= 0. {
        right
    } else {
        [-right[0], -right[1]]
    }
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum()
}

fn inward_xy(x: f64, y: f64) -> [f64; 3] {
    let len = x.hypot(y);
    if len < 1. {
        [1., 0., 0.]
    } else {
        [-x / len, -y / len, 0.]
    }
}

fn direction(frame: &Frame, local: [f64; 3]) -> [f64; 3] {
    let world = frame.world(Point3(local)).0;
    std::array::from_fn(|i| world[i] - frame.origin.0[i])
}

/// A slab or raft whose body contains the column top, preferring the one
/// whose bottom is closest to that point.
fn slab_above(project: &Project, top: Point3) -> Option<ObjectId> {
    let mut best: Option<(f64, ObjectId)> = None;
    for (id, element) in &project.elements {
        let ElementShape::Plate {
            contour,
            thickness,
            frame,
        } = &element.shape
        else {
            continue;
        };
        let local = frame.local(top).0;
        let t = thickness.value();
        if !(-20.0..=t + 20.).contains(&local[2]) || !point_in(contour, [local[0], local[1]]) {
            continue;
        }
        let gap = local[2].abs();
        if best.as_ref().is_none_or(|(have, _)| gap < *have) {
            best = Some((gap, *id));
        }
    }
    best.map(|(_, id)| id)
}

fn point_in(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut n = 0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]);
            if x > p[0] {
                n += 1;
            }
        }
    }
    n % 2 == 1
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::concrete::REBAR_COVER;
    use crate::{
        Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId, Profile,
        ProfileShape, Project, PropertyValue,
    };

    fn column() -> (Project, ObjectId) {
        let mut project = Project {
            name: "Выпуски".into(),
            next_id: 30,
            ..Project::default()
        };
        project.materials.insert(
            ObjectId(1),
            Material {
                id: ObjectId(1),
                name: "Бетон В25".into(),
                density_kg_m3: 2500.,
            },
        );
        project.profiles.insert(
            ObjectId(2),
            Profile {
                id: ObjectId(2),
                name: "400×400".into(),
                shape: ProfileShape::Rectangle {
                    height: 400.,
                    width: 400.,
                },
                source: "тест".into(),
                standard: None,
                area_mm2: None,
            },
        );
        let id = ObjectId(10);
        let mut properties = BTreeMap::new();
        properties.insert(REBAR_COVER.into(), PropertyValue::Number(30.));
        project.elements.insert(
            id,
            Element {
                id,
                name: "Колонна".into(),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: MemberKind::Column,
                    profile: ObjectId(2),
                    length: LengthMm::new(3000.).unwrap(),
                    frame: Frame::default(),
                    position: Default::default(),
                },
                properties,
            },
        );
        (project, id)
    }

    fn dist(a: &Point3, b: &Point3) -> f64 {
        (0..3)
            .map(|k| (a.0[k] - b.0[k]).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    #[test]
    fn straight_and_l_starters_use_the_lap_length() {
        let (mut project, id) = column();
        let lap = lap_length(&project, id, 12., "А500С");
        assert!(lap >= 250.);
        project.concrete = install_starter(
            &project,
            id,
            StarterPath::ColumnTop,
            "gost.straight",
            12.,
            "А500С",
            200.,
        )
        .unwrap();
        let set = &project.concrete.rebars[0];
        assert_eq!(set.host, Some(id));
        assert_eq!(set.bars.len(), 4);
        assert!(set.starter.as_ref().unwrap().into.is_none());
        for bar in &set.bars {
            assert!((dist(&bar[0], &bar[1]) - lap).abs() < 1e-3);
            assert!((bar[0].0[2] - 3000.).abs() < 1e-6);
            assert!(bar[1].0[2] > bar[0].0[2]);
        }
        project.concrete = install_starter(
            &project,
            id,
            StarterPath::Face { index: 2 },
            "gost.l",
            12.,
            "А500С",
            200.,
        )
        .unwrap();
        let set = project
            .concrete
            .rebars
            .iter()
            .find(|s| s.shape_id.as_deref() == Some("gost.l"))
            .unwrap();
        assert_eq!(set.bars.len(), 16);
        for bar in &set.bars {
            assert!(
                (dist(&bar[0], &bar[1]) - lap).abs() < 1e-3,
                "{}",
                dist(&bar[0], &bar[1])
            );
            assert!((dist(&bar[1], &bar[2]) - lap).abs() < 1e-3);
        }
        let rows = super::super::detail_schedule(&project);
        assert!(rows.iter().any(|r| r.code == "ПР"));
        assert!(rows.iter().any(|r| r.code == "Г"));
        let steel = super::super::steel_schedule(&project);
        assert!(steel.by_element.iter().any(|r| r.element == "Колонна"));
    }

    #[test]
    fn column_starters_record_the_slab_above() {
        let (mut project, id) = column();
        let slab = ObjectId(11);
        project.elements.insert(
            slab,
            Element {
                id: slab,
                name: "Плита".into(),
                material: ObjectId(1),
                shape: ElementShape::Plate {
                    contour: vec![
                        [-2000., -2000.],
                        [2000., -2000.],
                        [2000., 2000.],
                        [-2000., 2000.],
                    ],
                    thickness: LengthMm::new(200.).unwrap(),
                    frame: Frame {
                        origin: Point3([0., 0., 3000.]),
                        ..Frame::default()
                    },
                },
                properties: BTreeMap::new(),
            },
        );
        project.concrete = install_starter(
            &project,
            id,
            StarterPath::ColumnTop,
            "gost.straight",
            12.,
            "А500С",
            200.,
        )
        .unwrap();
        assert_eq!(
            project.concrete.rebars[0].starter.as_ref().unwrap().into,
            Some(slab)
        );
    }

    #[test]
    fn face_and_slab_edge_follow_the_path() {
        let (mut project, id) = column();
        let lap = lap_length(&project, id, 12., "А500С");
        project.concrete = install_starter(
            &project,
            id,
            StarterPath::Face { index: 2 },
            "gost.straight",
            12.,
            "А500С",
            200.,
        )
        .unwrap();
        let set = &project.concrete.rebars[0];
        // 30..2970 at 200 → 16 bars, sticking out of the top face.
        assert_eq!(set.bars.len(), 16);
        let bar = &set.bars[0];
        assert!((bar[0].0[1] - 194.).abs() < 1e-6, "{}", bar[0].0[1]);
        assert!((dist(&bar[0], &bar[1]) - lap).abs() < 1e-3);
        assert!(bar[1].0[1] > bar[0].0[1]);

        let slab = ObjectId(12);
        project.elements.insert(
            slab,
            Element {
                id: slab,
                name: "Перекрытие".into(),
                material: ObjectId(1),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [4000., 0.], [4000., 1000.], [0., 1000.]],
                    thickness: LengthMm::new(200.).unwrap(),
                    frame: Frame::default(),
                },
                properties: BTreeMap::from([(REBAR_COVER.into(), PropertyValue::Number(30.))]),
            },
        );
        project.concrete = install_starter(
            &project,
            slab,
            StarterPath::SlabEdge { index: 0 },
            "gost.straight",
            12.,
            "А500С",
            200.,
        )
        .unwrap();
        let edge = project
            .concrete
            .rebars
            .iter()
            .find(|s| s.host == Some(slab))
            .unwrap();
        assert_eq!(edge.bars.len(), 21);
        assert!(
            (dist(&edge.bars[0][0], &edge.bars[0][1]) - lap_length(&project, slab, 12., "А500С"))
                .abs()
                < 1e-3
        );
        assert!(edge.bars[0][1].0[1] < edge.bars[0][0].0[1]);
    }

    #[test]
    fn a_longer_column_moves_the_starters() {
        let (mut project, id) = column();
        project.concrete = install_starter(
            &project,
            id,
            StarterPath::ColumnTop,
            "gost.straight",
            12.,
            "А500С",
            200.,
        )
        .unwrap();
        let previous = project.clone();
        if let ElementShape::Member { length, .. } =
            &mut project.elements.get_mut(&id).unwrap().shape
        {
            *length = LengthMm::new(3600.).unwrap();
        }
        super::super::rebuild_rebars(&mut project, &previous).unwrap();
        let bar = &project.concrete.rebars[0].bars[0];
        assert!((bar[0].0[2] - 3600.).abs() < 1e-6, "{}", bar[0].0[2]);
    }
}
