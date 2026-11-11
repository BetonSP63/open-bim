//! Area reinforcement of a slab («раскладка»): uniform meshes, U-bars along
//! the contour and the openings, and frogs between the meshes. The slab type
//! carries the meshes; the cover lives on the concrete host. Layout runs
//! again when the host, the type or a referenced shape type changes.
use std::collections::{BTreeMap, BTreeSet};

use crate::concrete::{
    BarAlign, FrogArray, MeshLayer, REBAR_COVER, RebarCheck, RebarZone, SlabRebarType, ZoneRole,
};
use crate::{
    DetailKind, ElementShape, Frame, ModelError, ObjectId, Point3, Project, PropertyValue,
    RebarKind, RebarRule, RebarSet,
};

use super::shape::{self, resolve_shape};
use super::{Stress, anchorage, host_rbt, rebar_class};

/// Concrete cover of a host, mm. The part property wins; otherwise the
/// project default, otherwise 30.
pub fn host_cover(project: &Project, host: ObjectId) -> f64 {
    let own = project
        .elements
        .get(&host)
        .and_then(|e| match e.properties.get(REBAR_COVER) {
            Some(PropertyValue::Number(v)) if (0.0..=500.).contains(v) => Some(*v),
            _ => None,
        });
    own.or(project.concrete.default_cover).unwrap_or(30.)
}

/// Replace or create the area reinforcement of one slab from its type.
pub fn install_slab(
    project: &Project,
    host: ObjectId,
    type_id: &str,
) -> Result<crate::Concrete, String> {
    let mut concrete = project.concrete.clone();
    let number = concrete
        .zones
        .iter()
        .find(|z| z.host == host)
        .map(|z| z.number)
        .unwrap_or_else(|| concrete.zones.iter().map(|z| z.number).max().unwrap_or(0) + 1);
    let mut zone = concrete
        .zones
        .iter()
        .find(|z| z.number == number)
        .cloned()
        .unwrap_or(RebarZone {
            number,
            host,
            type_id: type_id.into(),
            edge_off: Vec::new(),
            opening_off: Vec::new(),
            mark: String::new(),
            mark_at: Point3([0.; 3]),
            checks: Vec::new(),
        });
    zone.host = host;
    zone.type_id = type_id.into();
    let old: Vec<_> = concrete
        .rebars
        .iter()
        .filter(|r| r.zone == Some(number))
        .cloned()
        .collect();
    let outside: Vec<_> = concrete
        .rebars
        .iter()
        .filter(|r| r.zone != Some(number))
        .cloned()
        .collect();
    let (zone, sets) = layout(project, &zone, &old, &outside)?;
    concrete.rebars.retain(|r| r.zone != Some(number));
    concrete.zones.retain(|z| z.number != number);
    concrete.rebars.extend(sets);
    concrete.zones.push(zone);
    concrete.rebars.sort_by_key(|r| r.number);
    concrete.zones.sort_by_key(|z| z.number);
    concrete.validate().map_err(|e| e.to_string())?;
    Ok(concrete)
}

/// Turn U-bars off on the given outer edges and opening sides. The meshes stay.
pub fn set_zone_framing(
    project: &Project,
    zone: u32,
    edge_off: Vec<u32>,
    opening_off: Vec<[u32; 2]>,
) -> Result<crate::Concrete, String> {
    let mut staged = project.clone();
    let found = staged
        .concrete
        .zones
        .iter_mut()
        .find(|z| z.number == zone)
        .ok_or("Нет зоны армирования")?;
    found.edge_off = edge_off;
    found.opening_off = opening_off;
    let host = found.host;
    let type_id = found.type_id.clone();
    install_slab(&staged, host, &type_id)
}

/// Notes for the model check: stretches where a U-bar or a frog was not placed.
pub fn model_checks(project: &Project) -> Vec<String> {
    let mut out = Vec::new();
    for z in &project.concrete.zones {
        let name = project
            .elements
            .get(&z.host)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| format!("#{}", z.host.0));
        for c in &z.checks {
            out.push(format!("{name}: {}", c.message));
        }
    }
    out
}

pub(super) fn rebuild_area(
    project: &Project,
    previous: &Project,
    zones: &mut Vec<RebarZone>,
    sets: &mut Vec<RebarSet>,
) -> Result<(), ModelError> {
    zones.retain(|z| project.elements.contains_key(&z.host));
    let alive: BTreeSet<u32> = zones.iter().map(|z| z.number).collect();
    sets.retain(|s| s.zone.is_none_or(|n| alive.contains(&n)));
    for zone in zones.iter_mut() {
        if !zone_changed(project, previous, zone) {
            continue;
        }
        let old: Vec<_> = sets
            .iter()
            .filter(|s| s.zone == Some(zone.number))
            .cloned()
            .collect();
        let outside: Vec<_> = sets
            .iter()
            .filter(|s| s.zone != Some(zone.number))
            .cloned()
            .collect();
        match layout(project, zone, &old, &outside) {
            Ok((next, laid)) => {
                sets.retain(|s| s.zone != Some(zone.number));
                sets.extend(laid);
                *zone = next;
            }
            Err(e) => {
                return Err(ModelError::InvalidValue(format!(
                    "Зона {}: {e}",
                    zone.number
                )));
            }
        }
    }
    sets.sort_by_key(|r| r.number);
    zones.sort_by_key(|z| z.number);
    Ok(())
}

fn zone_changed(project: &Project, previous: &Project, zone: &RebarZone) -> bool {
    let Some(prev) = previous
        .concrete
        .zones
        .iter()
        .find(|z| z.number == zone.number)
    else {
        return true;
    };
    if prev.type_id != zone.type_id
        || prev.edge_off != zone.edge_off
        || prev.opening_off != zone.opening_off
    {
        return true;
    }
    if super::host_changed(project, previous, zone.host) {
        return true;
    }
    let ty = |p: &Project| {
        p.concrete
            .slab_types
            .iter()
            .find(|t| t.id == zone.type_id)
            .cloned()
    };
    if ty(project) != ty(previous) {
        return true;
    }
    let shapes = ty(project).map(|t| vec![t.edge_shape, t.frog_shape, "gost.straight".into()]);
    shapes
        .into_iter()
        .flatten()
        .any(|id| resolve_shape(project, &id) != resolve_shape(previous, &id))
}

struct Geom {
    frame: Frame,
    contour: Vec<[f64; 2]>,
    thickness: f64,
    openings: Vec<Ring>,
}

struct Ring {
    index: u32,
    pts: Vec<[f64; 2]>,
}

struct Side {
    opening: Option<u32>,
    edge: u32,
    a: [f64; 2],
    tangent: [f64; 2],
    inward: [f64; 2],
    length: f64,
}

struct Draft {
    role: ZoneRole,
    kind: RebarKind,
    diameter: f64,
    grade: String,
    bars: Vec<Vec<Point3>>,
    shape_id: String,
    params: BTreeMap<String, f64>,
    spacing: f64,
    remainders: Vec<f64>,
    array: Option<FrogArray>,
}

fn layout(
    project: &Project,
    zone: &RebarZone,
    old: &[RebarSet],
    outside: &[RebarSet],
) -> Result<(RebarZone, Vec<RebarSet>), String> {
    let ty = project
        .concrete
        .slab_types
        .iter()
        .find(|t| t.id == zone.type_id)
        .cloned()
        .ok_or("Нет типа плиты")?;
    if ty.bottom.is_none() && ty.top.is_none() {
        return Err("У типа плиты нет сеток".into());
    }
    let geom = geom_of(project, zone.host)?;
    let cover = host_cover(project, zone.host);
    let mut drafts = Vec::new();
    let mut checks = Vec::new();
    lay_meshes(project, zone.host, &ty, &geom, cover, &mut drafts)?;
    if ty.bottom.is_some() && ty.top.is_some() {
        if ty.edge_bars {
            lay_edges(project, &ty, &geom, cover, zone, &mut drafts, &mut checks)?;
        }
        if ty.frogs {
            lay_frogs(project, &ty, &geom, cover, &mut drafts, &mut checks)?;
        }
    }
    let (mark, mut sets) = number_sets(zone, old, outside, drafts);
    for set in &mut sets {
        if let Some(rule) = &mut set.rule {
            rule.cover = cover;
        }
    }
    let mut zone = zone.clone();
    zone.mark = mark;
    zone.mark_at = zone_centre(&geom);
    zone.checks = checks;
    Ok((zone, sets))
}

fn geom_of(project: &Project, host: ObjectId) -> Result<Geom, String> {
    let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
    let ElementShape::Plate {
        contour,
        thickness,
        frame,
    } = &e.shape
    else {
        return Err("Раскладка выполняется в плите".into());
    };
    if contour.len() < 3 {
        return Err("Контур плиты не замкнут".into());
    }
    let thickness = thickness.value();
    let mut openings = Vec::new();
    for d in project.details.values() {
        let DetailKind::PrismCut {
            part,
            frame: cut,
            contour: ring,
            length,
        } = &d.kind
        else {
            continue;
        };
        if *part != host || *length + 1. < thickness || ring.len() < 3 {
            continue;
        }
        let pts = ring
            .iter()
            .map(|p| {
                let w = cut.world(Point3([p[0], p[1], 0.]));
                let l = frame.local(w);
                [l.0[0], l.0[1]]
            })
            .collect();
        openings.push(Ring {
            index: openings.len() as u32,
            pts,
        });
    }
    Ok(Geom {
        frame: *frame,
        contour: contour.clone(),
        thickness,
        openings,
    })
}

fn lay_meshes(
    project: &Project,
    host: ObjectId,
    ty: &SlabRebarType,
    geom: &Geom,
    cover: f64,
    out: &mut Vec<Draft>,
) -> Result<(), String> {
    let mut runs = Vec::new();
    if let Some(layer) = &ty.bottom {
        runs.extend(face_runs(layer, false, geom.thickness, cover)?);
    }
    if let Some(layer) = &ty.top {
        runs.extend(face_runs(layer, true, geom.thickness, cover)?);
    }
    if let (Some(b), Some(t)) = (&ty.bottom, &ty.top) {
        let b = face_runs(b, false, geom.thickness, cover)?;
        let t = face_runs(t, true, geom.thickness, cover)?;
        let hi = b.iter().map(|r| r.z + r.diameter / 2.).fold(0., f64::max);
        let lo = t
            .iter()
            .map(|r| r.z - r.diameter / 2.)
            .fold(f64::MAX, f64::min);
        if hi >= lo - 1. {
            return Err("Толщина меньше защитного слоя двух сеток".into());
        }
    }
    let shape = resolve_shape(project, "gost.straight").ok_or("Нет типа формы «Прямой»")?;
    let class_r = |grade: &str, d: f64| {
        let class = rebar_class(grade).unwrap_or(&super::REBAR_CLASSES[2]);
        anchorage(class, d, host_rbt(project, host), 1., Stress::Tension).lap
    };
    let holes: Vec<Vec<[f64; 2]>> = if ty.cut_openings {
        geom.openings.iter().map(|o| o.pts.clone()).collect()
    } else {
        Vec::new()
    };
    for run in runs {
        let (v, u) = dir_axes(run.direction);
        let proj = |p: [f64; 2]| dot2(p, u);
        let (lo, hi) = geom.contour.iter().fold((f64::MAX, f64::MIN), |(l, h), p| {
            (l.min(proj(*p)), h.max(proj(*p)))
        });
        let inset = cover + run.diameter / 2.;
        let (coords, remainders) = stations(lo + inset, hi - inset, run.spacing, ty.align);
        let mut bars = Vec::new();
        let mandrel = shape::mandrel_of(&shape, &run.grade, run.diameter)?;
        let radius = (mandrel + run.diameter) / 2.;
        let lap = class_r(&run.grade, run.diameter);
        for c in coords {
            let rings = std::iter::once(geom.contour.clone()).chain(holes.clone());
            for (t0, t1) in segments(&rings.collect::<Vec<_>>(), u, v, c, inset) {
                let at = |t: f64, z: f64| {
                    let p = add2(mul2(u, c), mul2(v, t));
                    [p[0], p[1], z]
                };
                for (a, b, first, last) in
                    super::split(at(t0, run.z), at(t1, run.z), Some(11_700.), lap)
                {
                    let none = crate::Hook::default();
                    let hooks = [
                        if first { shape.hooks[0] } else { none },
                        if last { shape.hooks[1] } else { none },
                    ];
                    let local =
                        super::hooked(a, b, [0., 0., run.toward], hooks, run.diameter, radius);
                    bars.push(
                        local
                            .into_iter()
                            .map(|p| geom.frame.world(Point3(p)))
                            .collect(),
                    );
                }
            }
        }
        if bars.is_empty() {
            return Err("Контур меньше защитного слоя".into());
        }
        out.push(Draft {
            role: run.role,
            kind: RebarKind::Mesh,
            diameter: run.diameter,
            grade: run.grade,
            bars,
            shape_id: "gost.straight".into(),
            params: BTreeMap::new(),
            spacing: run.spacing,
            remainders,
            array: None,
        });
    }
    Ok(())
}

struct Run {
    role: ZoneRole,
    diameter: f64,
    grade: String,
    spacing: f64,
    direction: f64,
    z: f64,
    toward: f64,
}

fn face_runs(layer: &MeshLayer, top: bool, t: f64, cover: f64) -> Result<Vec<Run>, String> {
    let d1 = layer.diameter;
    let d2 = layer.cross_diameter();
    let (z_main, toward, role_main, role_cross) = if top {
        (
            t - cover - d1 / 2.,
            -1.,
            ZoneRole::TopMain,
            ZoneRole::TopCross,
        )
    } else {
        (
            cover + d1 / 2.,
            1.,
            ZoneRole::BottomMain,
            ZoneRole::BottomCross,
        )
    };
    if z_main - d1 / 2. < -1. || z_main + d1 / 2. > t + 1. {
        return Err("Толщина меньше защитного слоя".into());
    }
    let mut runs = vec![Run {
        role: role_main,
        diameter: d1,
        grade: layer.grade.clone(),
        spacing: layer.spacing,
        direction: layer.direction_deg,
        z: z_main,
        toward,
    }];
    if let Some(step) = layer.cross_spacing {
        let z = if top {
            t - cover - d1 - d2 / 2.
        } else {
            cover + d1 + d2 / 2.
        };
        if z - d2 / 2. < -1. || z + d2 / 2. > t + 1. {
            return Err("Толщина меньше защитного слоя".into());
        }
        runs.push(Run {
            role: role_cross,
            diameter: d2,
            grade: layer.grade.clone(),
            spacing: step,
            direction: layer.direction_deg + 90.,
            z,
            toward,
        });
    }
    Ok(runs)
}

fn lay_edges(
    project: &Project,
    ty: &SlabRebarType,
    geom: &Geom,
    cover: f64,
    zone: &RebarZone,
    out: &mut Vec<Draft>,
    checks: &mut Vec<RebarCheck>,
) -> Result<(), String> {
    let (Some(bottom), Some(top)) = (&ty.bottom, &ty.top) else {
        return Ok(());
    };
    let shape = resolve_shape(project, &ty.edge_shape).ok_or("Нет типа формы П-стержня")?;
    let class = rebar_class(&ty.edge_grade).ok_or("Класс арматуры П-стержней не найден")?;
    let lan = anchorage(
        class,
        ty.edge_diameter,
        host_rbt(project, zone.host),
        1.,
        Stress::Tension,
    )
    .anchorage;
    let bot = face_runs(bottom, false, geom.thickness, cover)?;
    let top_runs = face_runs(top, true, geom.thickness, cover)?;
    let mut outer = Vec::new();
    let mut holes = Vec::new();
    for side in all_sides(geom) {
        if framing_off(zone, &side) {
            continue;
        }
        let along_main = along_primary(&side, bottom.direction_deg);
        let z_bot = run_z(&bot, along_main).max(cover + ty.edge_diameter / 2.);
        let z_top =
            run_z(&top_runs, along_main).min(geom.thickness - cover - ty.edge_diameter / 2.);
        let height = z_top - z_bot;
        let label = side_label(&side);
        if height < 20. {
            checks.push(RebarCheck {
                message: format!("{label}: П-стержни не поставлены — между сетками нет высоты"),
                opening: side.opening,
                edge: Some(side.edge),
                from: Some(0.),
                to: Some(side.length),
            });
            continue;
        }
        let mut params = BTreeMap::new();
        params.insert("A".into(), lan);
        params.insert("B".into(), height);
        params.insert("C".into(), lan);
        let inset = cover + ty.edge_diameter / 2.;
        let (ok, skipped) = place_side(&UPlace {
            ty,
            geom,
            shape: &shape,
            params: &params,
            side: &side,
            lan,
            z0: z_bot,
            inset,
        });
        if !skipped.is_empty() {
            for (from, to) in merge_spans(&skipped, ty.edge_spacing) {
                checks.push(RebarCheck {
                    message: format!(
                        "{label}, {from:.0}–{to:.0} мм: П-стержень не поставлен, длина анкеровки {lan:.0} мм не вмещается"
                    ),
                    opening: side.opening,
                    edge: Some(side.edge),
                    from: Some(from),
                    to: Some(to),
                });
            }
        }
        if side.opening.is_some() {
            holes.extend(ok);
        } else {
            outer.extend(ok);
        }
    }
    push_edge(out, ty, &shape.id, lan, ZoneRole::Edge, outer);
    push_edge(out, ty, &shape.id, lan, ZoneRole::Opening, holes);
    Ok(())
}

fn push_edge(
    out: &mut Vec<Draft>,
    ty: &SlabRebarType,
    shape: &str,
    lan: f64,
    role: ZoneRole,
    bars: Vec<Vec<Point3>>,
) {
    if bars.is_empty() {
        return;
    }
    let mut params = BTreeMap::new();
    params.insert("A".into(), lan);
    params.insert("C".into(), lan);
    out.push(Draft {
        role,
        kind: RebarKind::Edge,
        diameter: ty.edge_diameter,
        grade: ty.edge_grade.clone(),
        bars,
        shape_id: shape.into(),
        params,
        spacing: ty.edge_spacing,
        remainders: Vec::new(),
        array: None,
    });
}

#[derive(Clone, Copy)]
struct UPlace<'a> {
    ty: &'a SlabRebarType,
    geom: &'a Geom,
    shape: &'a crate::concrete::BarShapeType,
    params: &'a BTreeMap<String, f64>,
    side: &'a Side,
    lan: f64,
    z0: f64,
    inset: f64,
}

fn place_side(job: &UPlace<'_>) -> (Vec<Vec<Point3>>, Vec<f64>) {
    let UPlace {
        ty,
        geom,
        shape,
        params,
        side,
        lan,
        z0,
        inset,
    } = *job;
    let spacing = ty.edge_spacing;
    let start = (spacing * 0.5).min(side.length * 0.5);
    let end = (side.length - spacing * 0.5).max(start);
    let (ss, _) = if side.length <= spacing + 1. {
        (vec![side.length / 2.], vec![])
    } else {
        stations(start, end, spacing, BarAlign::Edge)
    };
    let mut ok = Vec::new();
    let mut skipped = Vec::new();
    for s in ss {
        let base = add2(side.a, mul2(side.tangent, s));
        let base = add2(base, mul2(side.inward, inset));
        let origin_xy = add2(base, mul2(side.inward, lan));
        let place = crate::concrete::ShapePlace {
            origin: Point3([origin_xy[0], origin_xy[1], z0]),
            x: [-side.inward[0], -side.inward[1], 0.],
            y: [0., 0., 1.],
            local_to_host: false,
        };
        let Ok(made) = shape::instantiate(shape, params, ty.edge_diameter, &ty.edge_grade, &place)
        else {
            skipped.push(s);
            continue;
        };
        let bar = made.bar;
        if bar_in_concrete(geom, &bar) {
            ok.push(bar.into_iter().map(|p| geom.frame.world(p)).collect());
        } else {
            skipped.push(s);
        }
    }
    (ok, skipped)
}

fn lay_frogs(
    project: &Project,
    ty: &SlabRebarType,
    geom: &Geom,
    cover: f64,
    out: &mut Vec<Draft>,
    checks: &mut Vec<RebarCheck>,
) -> Result<(), String> {
    let (Some(bottom), Some(top)) = (&ty.bottom, &ty.top) else {
        return Ok(());
    };
    let shape = resolve_shape(project, &ty.frog_shape).ok_or("Нет типа формы лягушки")?;
    let bot = face_runs(bottom, false, geom.thickness, cover)?;
    let top_runs = face_runs(top, true, geom.thickness, cover)?;
    let (z_b, d_b) = upper_bottom(&bot);
    let (z_t, d_t) = lower_top(&top_runs);
    let d_f = ty.frog_diameter;
    let z_foot = (z_b + d_b / 2. + d_f / 2.).max(cover + d_f / 2.);
    let z_seat = (z_t - d_t / 2. - d_f / 2.).min(geom.thickness - cover - d_f / 2.);
    let height = z_seat - z_foot;
    if height < 20. {
        checks.push(RebarCheck {
            message: "Лягушки не поставлены: между сетками нет места по высоте".into(),
            opening: None,
            edge: None,
            from: None,
            to: None,
        });
        return Ok(());
    }
    let mut params = BTreeMap::new();
    params.insert("B".into(), height);
    let line = shape::centerline(&shape, &params)?;
    let (xmin, xmax) = line
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[0]), h.max(p[0])));
    let mid = (xmin + xmax) / 2.;
    let half = (xmax - xmin) / 2.;
    let (v, u) = dir_axes(bottom.direction_deg);
    let proj = |p: [f64; 2]| dot2(p, u);
    let (lo, hi) = geom.contour.iter().fold((f64::MAX, f64::MIN), |(l, h), p| {
        (l.min(proj(*p)), h.max(proj(*p)))
    });
    let inset = cover + d_f / 2.;
    let (coords, _) = stations(lo + inset, hi - inset, ty.frog_spacing, ty.align);
    let mut bars = Vec::new();
    let mut lines = Vec::new();
    let mut missed: Vec<(f64, f64)> = Vec::new();
    for c in coords {
        let holes: Vec<_> = geom.openings.iter().map(|o| o.pts.clone()).collect();
        let rings = std::iter::once(geom.contour.clone()).chain(holes);
        for (t0, t1) in segments(&rings.collect::<Vec<_>>(), u, v, c, inset) {
            let a = t0 + half;
            let b = t1 - half;
            if b < a + 1. {
                missed.push((t0, t1));
                continue;
            }
            let (centers, _) = stations(a, b, ty.frog_spacing, BarAlign::Edge);
            let mut placed = Vec::new();
            let mut skipped = Vec::new();
            for t in &centers {
                let center = add2(mul2(u, c), mul2(v, *t));
                let origin = add2(center, mul2(v, -mid));
                let place = crate::concrete::ShapePlace {
                    origin: Point3([origin[0], origin[1], z_foot]),
                    x: [v[0], v[1], 0.],
                    y: [0., 0., 1.],
                    local_to_host: false,
                };
                let Ok(made) = shape::instantiate(&shape, &params, d_f, &ty.frog_grade, &place)
                else {
                    skipped.push(*t);
                    continue;
                };
                let bar = made.bar;
                if bar_in_concrete(geom, &bar) {
                    placed.push(
                        bar.into_iter()
                            .map(|p| geom.frame.world(p))
                            .collect::<Vec<_>>(),
                    );
                } else {
                    skipped.push(*t);
                }
            }
            if placed.is_empty() {
                missed.push((t0, t1));
            } else {
                let s = add2(mul2(u, c), mul2(v, t0));
                let e = add2(mul2(u, c), mul2(v, t1));
                lines.push((s, e));
                bars.extend(placed);
                if !skipped.is_empty() {
                    for (from, to) in merge_spans(&skipped, ty.frog_spacing) {
                        missed.push((from, to));
                    }
                }
            }
        }
    }
    if bars.is_empty() {
        checks.push(RebarCheck {
            message: "Лягушки не поставлены: не вмещаются в контур без укорочения лапок".into(),
            opening: None,
            edge: None,
            from: None,
            to: None,
        });
        return Ok(());
    }
    if !missed.is_empty() {
        checks.push(RebarCheck {
            message: format!(
                "Лягушки пропущены на {} участках: стержень не вмещается без укорочения",
                missed.len()
            ),
            opening: None,
            edge: None,
            from: None,
            to: None,
        });
    }
    out.push(Draft {
        role: ZoneRole::Frog,
        kind: RebarKind::Frog,
        diameter: ty.frog_diameter,
        grade: ty.frog_grade.clone(),
        bars,
        shape_id: shape.id,
        params,
        spacing: ty.frog_spacing,
        remainders: Vec::new(),
        array: Some(FrogArray {
            lines,
            spacing: ty.frog_spacing,
        }),
    });
    Ok(())
}

fn upper_bottom(runs: &[Run]) -> (f64, f64) {
    runs.iter()
        .max_by(|a, b| a.z.total_cmp(&b.z))
        .map(|r| (r.z, r.diameter))
        .unwrap_or((0., 0.))
}

fn lower_top(runs: &[Run]) -> (f64, f64) {
    runs.iter()
        .min_by(|a, b| a.z.total_cmp(&b.z))
        .map(|r| (r.z, r.diameter))
        .unwrap_or((0., 0.))
}

fn run_z(runs: &[Run], main: bool) -> f64 {
    runs.iter()
        .find(|r| matches!(r.role, ZoneRole::BottomMain | ZoneRole::TopMain) == main)
        .or_else(|| runs.first())
        .map(|r| r.z)
        .unwrap_or(0.)
}

fn along_primary(side: &Side, direction_deg: f64) -> bool {
    let (v, u) = dir_axes(direction_deg);
    dot2(side.inward, v).abs() >= dot2(side.inward, u).abs()
}

fn number_sets(
    zone: &RebarZone,
    old: &[RebarSet],
    outside: &[RebarSet],
    drafts: Vec<Draft>,
) -> (String, Vec<RebarSet>) {
    let mut used_num: BTreeSet<u32> = outside.iter().map(|r| r.number).collect();
    let mut used_pos: BTreeSet<u32> = outside.iter().filter_map(|r| r.position).collect();
    let alloc = |used: &mut BTreeSet<u32>| {
        let mut n = used.iter().next_back().copied().unwrap_or(0);
        loop {
            n += 1;
            if used.insert(n) {
                return n;
            }
        }
    };
    let mut sets = Vec::new();
    for d in drafts {
        let prev = old.iter().find(|s| s.role == Some(d.role));
        let number = prev
            .map(|s| s.number)
            .filter(|n| used_num.insert(*n))
            .unwrap_or_else(|| alloc(&mut used_num));
        let position = prev
            .and_then(|s| s.position)
            .filter(|n| used_pos.insert(*n))
            .unwrap_or_else(|| alloc(&mut used_pos));
        let mut set = RebarSet::basic(d.kind, d.diameter, d.grade);
        set.number = number;
        set.host = Some(zone.host);
        set.bars = d.bars;
        set.shape_id = Some(d.shape_id);
        set.shape_params = d.params;
        set.zone = Some(zone.number);
        set.position = Some(position);
        set.role = Some(d.role);
        set.remainders = d.remainders;
        set.array = d.array;
        set.rule = Some(RebarRule {
            cover: 0.,
            spacing: d.spacing,
            per_face: 2,
            stock_length: Some(11_700.),
        });
        sets.push(set);
    }
    // Cover is written below once the host value is known — the drafts do
    // not carry it. `layout` patches it after this call.
    let mut pos: Vec<u32> = sets.iter().filter_map(|s| s.position).collect();
    pos.sort();
    let mark = pos
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    (mark, sets)
}

fn zone_centre(geom: &Geom) -> Point3 {
    let c = centroid(&geom.contour);
    geom.frame.world(Point3([c[0], c[1], geom.thickness / 2.]))
}

fn centroid(ring: &[[f64; 2]]) -> [f64; 2] {
    let mut a = 0.;
    let mut cx = 0.;
    let mut cy = 0.;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        let cross = p[0] * q[1] - q[0] * p[1];
        a += cross;
        cx += (p[0] + q[0]) * cross;
        cy += (p[1] + q[1]) * cross;
    }
    if a.abs() < 1e-6 {
        let n = ring.len() as f64;
        return [
            ring.iter().map(|p| p[0]).sum::<f64>() / n,
            ring.iter().map(|p| p[1]).sum::<f64>() / n,
        ];
    }
    [cx / (3. * a), cy / (3. * a)]
}

/// Bar coordinates from `lo` to `hi`, and the remainder steps.
fn stations(lo: f64, hi: f64, spacing: f64, align: BarAlign) -> (Vec<f64>, Vec<f64>) {
    let span = hi - lo;
    if span < 1. || spacing < 1. {
        return (Vec::new(), Vec::new());
    }
    let mut pos = Vec::new();
    let mut rems = Vec::new();
    match align {
        BarAlign::Edge => {
            let n = (span / spacing).floor().max(0.) as i64;
            for i in 0..=n {
                pos.push(lo + i as f64 * spacing);
            }
            let rem = hi - *pos.last().unwrap();
            if rem > 1. {
                pos.push(hi);
                rems.push(rem);
            }
        }
        BarAlign::Axis => {
            let mid = (lo + hi) / 2.;
            pos.push(mid);
            for sign in [1_f64, -1.] {
                let limit = if sign > 0. { hi } else { lo };
                let mut p = mid + sign * spacing;
                while (limit - p) * sign > 1. {
                    pos.push(p);
                    p += sign * spacing;
                }
                let last = pos
                    .iter()
                    .copied()
                    .filter(|x| (x - mid) * sign >= -1e-6)
                    .fold(mid, |a, b| if sign > 0. { a.max(b) } else { a.min(b) });
                let rem = (limit - last).abs();
                if rem > 1. {
                    pos.push(limit);
                    rems.push(rem);
                }
            }
            pos.sort_by(f64::total_cmp);
            pos.dedup_by(|a, b| (*a - *b).abs() < 1.);
        }
    }
    (pos, rems)
}

fn segments(
    rings: &[Vec<[f64; 2]>],
    u: [f64; 2],
    v: [f64; 2],
    c: f64,
    inset: f64,
) -> Vec<(f64, f64)> {
    let mut ts = Vec::new();
    for ring in rings {
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let (da, db) = (dot2(a, u), dot2(b, u));
            if (da <= c) != (db <= c) {
                let s = (c - da) / (db - da);
                let p = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s];
                ts.push(dot2(p, v));
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.chunks(2)
        .filter(|p| p.len() == 2 && p[1] - p[0] > 2. * inset + 1.)
        .map(|p| (p[0] + inset, p[1] - inset))
        .collect()
}

fn dir_axes(deg: f64) -> ([f64; 2], [f64; 2]) {
    let a = deg.to_radians();
    ([a.cos(), a.sin()], [-a.sin(), a.cos()])
}

fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn add2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul2(a: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] * k, a[1] * k]
}

fn all_sides(geom: &Geom) -> Vec<Side> {
    let mut sides = sides_of(&geom.contour, None, signed_area(&geom.contour) > 0.);
    for o in &geom.openings {
        // Concrete is outside the opening.
        sides.extend(sides_of(&o.pts, Some(o.index), signed_area(&o.pts) < 0.));
    }
    sides
}

fn sides_of(ring: &[[f64; 2]], opening: Option<u32>, concrete_left: bool) -> Vec<Side> {
    let mut out = Vec::new();
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0].hypot(d[1]);
        if len < 1. {
            continue;
        }
        let tangent = [d[0] / len, d[1] / len];
        let left = [-tangent[1], tangent[0]];
        let inward = if concrete_left {
            left
        } else {
            [-left[0], -left[1]]
        };
        out.push(Side {
            opening,
            edge: i as u32,
            a,
            tangent,
            inward,
            length: len,
        });
    }
    out
}

fn side_label(side: &Side) -> String {
    match side.opening {
        None => format!("Кромка {}", side.edge + 1),
        Some(i) => format!("Проём {}, сторона {}", i + 1, side.edge + 1),
    }
}

fn framing_off(zone: &RebarZone, side: &Side) -> bool {
    match side.opening {
        None => zone.edge_off.contains(&side.edge),
        Some(i) => zone
            .opening_off
            .iter()
            .any(|p| p[0] == i && p[1] == side.edge),
    }
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    ring.iter()
        .enumerate()
        .map(|(i, p)| {
            let q = ring[(i + 1) % ring.len()];
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        * 0.5
}

fn merge_spans(spots: &[f64], spacing: f64) -> Vec<(f64, f64)> {
    if spots.is_empty() {
        return Vec::new();
    }
    let mut s = spots.to_vec();
    s.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    let (mut a, mut b) = (s[0], s[0]);
    for x in s.iter().skip(1) {
        if *x - b <= spacing + 1. {
            b = *x;
        } else {
            out.push((a, b));
            (a, b) = (*x, *x);
        }
    }
    out.push((a, b));
    out
}

fn bar_in_concrete(geom: &Geom, bar: &[Point3]) -> bool {
    let xy = |p: &Point3| [p.0[0], p.0[1]];
    bar.windows(2)
        .all(|w| segment_inside(geom, xy(&w[0]), xy(&w[1])))
}

fn segment_inside(geom: &Geom, a: [f64; 2], b: [f64; 2]) -> bool {
    if !inside(geom, a) || !inside(geom, b) {
        return false;
    }
    let mut rings = vec![geom.contour.clone()];
    rings.extend(geom.openings.iter().map(|o| o.pts.clone()));
    for ring in &rings {
        for i in 0..ring.len() {
            if crosses(a, b, ring[i], ring[(i + 1) % ring.len()]) {
                return false;
            }
        }
    }
    true
}

fn inside(geom: &Geom, p: [f64; 2]) -> bool {
    point_in(&geom.contour, p) && geom.openings.iter().all(|o| !point_in(&o.pts, p))
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

fn crosses(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    orient(a, b, c) * orient(a, b, d) < -1e-6 && orient(c, d, a) * orient(c, d, b) < -1e-6
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concrete::{MeshLayer, SlabRebarType};
    use crate::{Element, ElementShape, Frame, LengthMm, Material, Point3};

    fn project_with(width: f64, depth: f64, thick: f64) -> (Project, ObjectId) {
        let mut p = Project::default();
        p.materials.insert(
            ObjectId(1),
            Material {
                id: ObjectId(1),
                name: "Бетон В25".into(),
                density_kg_m3: 2500.,
            },
        );
        p.next_id = 20;
        let id = ObjectId(10);
        p.elements.insert(
            id,
            Element {
                id,
                name: "Перекрытие".into(),
                material: ObjectId(1),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [width, 0.], [width, depth], [0., depth]],
                    thickness: LengthMm::new(thick).unwrap(),
                    frame: Frame {
                        origin: Point3([0.; 3]),
                        ..Frame::default()
                    },
                },
                properties: BTreeMap::new(),
            },
        );
        (p, id)
    }

    fn slab_type(align: BarAlign, spacing: f64, cut: bool) -> SlabRebarType {
        let mut ty = SlabRebarType::new("s", "Плита с сетками");
        let mut layer = MeshLayer::new(10., spacing, 0.);
        layer.grade = "А500С".into();
        ty.bottom = Some(layer.clone());
        ty.top = Some(layer);
        ty.align = align;
        ty.cut_openings = cut;
        ty.edge_diameter = 12.;
        ty.edge_grade = "А500С".into();
        ty.edge_spacing = 200.;
        ty.frog_diameter = 8.;
        ty.frog_grade = "А500С".into();
        ty.frog_spacing = 600.;
        ty
    }

    fn install(p: &mut Project, host: ObjectId, ty: SlabRebarType) {
        p.concrete.slab_types.retain(|t| t.id != ty.id);
        p.concrete.slab_types.push(ty);
        p.concrete = install_slab(p, host, "s").unwrap();
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn mesh_fills_the_slab_and_follows_the_host() {
        let (mut p, host) = project_with(2000., 1000., 200.);
        install(&mut p, host, slab_type(BarAlign::Edge, 300., false));
        let main = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::BottomMain))
            .unwrap();
        // Y from 35 to 965 at 300: four steps and a 30 mm remainder.
        assert_eq!(main.remainders, vec![30.]);
        let ys: Vec<f64> = main.bars.iter().map(|b| b[0].0[1]).collect();
        assert_eq!(ys.len(), 5, "{ys:?}");
        assert!(
            near(ys[0], 35.) && near(*ys.last().unwrap(), 965.),
            "{ys:?}"
        );
        assert!(near(ys[4] - ys[3], 30.), "{ys:?}");
        let cross = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::BottomCross))
            .unwrap();
        assert!(
            cross.remainders.len() == 1 && near(cross.remainders[0], 130.),
            "{:?}",
            cross.remainders
        );
        // The mark lists every position and sits at the centre.
        let zone = &p.concrete.zones[0];
        assert_eq!(zone.mark, "1, 2, 3, 4, 5, 6");
        assert!(near(zone.mark_at.0[0], 1000.) && near(zone.mark_at.0[1], 500.));
        assert!(near(zone.mark_at.0[2], 100.));
        let top_z = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::TopMain))
            .unwrap()
            .bars[0][0]
            .0[2];
        assert!(near(top_z, 165.), "{top_z}");
        // A thicker slab lifts the top mesh; the bars are built again.
        let previous = p.clone();
        if let ElementShape::Plate { thickness, .. } = &mut p.elements.get_mut(&host).unwrap().shape
        {
            *thickness = LengthMm::new(300.).unwrap();
        }
        crate::rebar::rebuild_rebars(&mut p, &previous).unwrap();
        let top_z = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::TopMain))
            .unwrap()
            .bars[0][0]
            .0[2];
        assert!(near(top_z, 265.), "{top_z}");
        assert_eq!(p.concrete.zones[0].mark, "1, 2, 3, 4, 5, 6");
    }

    #[test]
    fn axis_alignment_leaves_remainder_steps_at_both_edges() {
        let (mut p, host) = project_with(2000., 1000., 200.);
        let mut ty = slab_type(BarAlign::Axis, 300., false);
        ty.edge_bars = false;
        ty.frogs = false;
        install(&mut p, host, ty);
        let main = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::BottomMain))
            .unwrap();
        let mut ys: Vec<f64> = main.bars.iter().map(|b| b[0].0[1]).collect();
        ys.sort_by(f64::total_cmp);
        assert!(
            ys.iter()
                .zip([35., 200., 500., 800., 965.])
                .all(|(a, b)| near(*a, b)),
            "{ys:?}"
        );
        assert_eq!(main.remainders.len(), 2);
        assert!(
            main.remainders.iter().all(|r| near(*r, 165.)),
            "{:?}",
            main.remainders
        );
    }

    #[test]
    fn u_bar_leg_is_anchorage_and_a_tight_edge_is_skipped() {
        let (mut p, host) = project_with(6000., 4000., 200.);
        install(&mut p, host, slab_type(BarAlign::Edge, 200., false));
        let class = rebar_class("А500С").unwrap();
        let lan = anchorage(class, 12., 1.05, 1., Stress::Tension).anchorage;
        let edge = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::Edge))
            .unwrap();
        assert!(edge.bars.len() > 4);
        for bar in &edge.bars {
            let leg =
                |i: usize, j: usize| (bar[i].0[0] - bar[j].0[0]).hypot(bar[i].0[1] - bar[j].0[1]);
            assert!(near(leg(0, 1), lan), "нога {} при lan {lan}", leg(0, 1));
            assert!(near(leg(2, 3), lan), "нога {}", leg(2, 3));
        }
        // Adjacent sides sit in different mesh layers, so no two bars share a point.
        let mut pts: Vec<[f64; 3]> = edge.bars.iter().flatten().map(|p| p.0).collect();
        pts.sort_by(|a, b| {
            a[0].total_cmp(&b[0])
                .then(a[1].total_cmp(&b[1]))
                .then(a[2].total_cmp(&b[2]))
        });
        for w in pts.windows(2) {
            let d = (0..3)
                .map(|k| (w[0][k] - w[1][k]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(d > 1., "совпали {w:?}");
        }
        // A 300 mm slab cannot take l_an; the meshes stay, the U-bars do not.
        let (mut q, host) = project_with(300., 300., 200.);
        install(&mut q, host, slab_type(BarAlign::Edge, 200., false));
        assert!(
            q.concrete
                .rebars
                .iter()
                .any(|r| r.role == Some(ZoneRole::BottomMain))
        );
        assert!(
            q.concrete
                .rebars
                .iter()
                .all(|r| r.role != Some(ZoneRole::Edge))
        );
        let notes = model_checks(&q);
        let bars: Vec<_> = notes.iter().filter(|n| n.contains("П-стержень")).collect();
        assert!(!bars.is_empty(), "{notes:?}");
        assert!(
            bars.iter()
                .all(|n| n.contains("не вмещается") && !n.contains("укороч")),
            "{bars:?}"
        );
    }

    #[test]
    fn frogs_appear_only_when_both_meshes_exist() {
        let (mut p, host) = project_with(6000., 4000., 200.);
        install(&mut p, host, slab_type(BarAlign::Edge, 200., false));
        let frog = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::Frog))
            .unwrap();
        assert!(frog.array.as_ref().unwrap().lines.len() > 1);
        assert!(frog.bars.len() > frog.array.as_ref().unwrap().lines.len());
        let z: Vec<f64> = frog.bars[0].iter().map(|p| p.0[2]).collect();
        let (lo, hi) = (
            z.iter().copied().fold(f64::MAX, f64::min),
            z.iter().copied().fold(f64::MIN, f64::max),
        );
        // Upper bottom bar Ø10 at 45, frog Ø8: foot 45+5+4, seat 155−5−4.
        assert!(near(hi - lo, 92.), "{lo} {hi}");
        // Bottom mesh alone: no frogs.
        let (mut q, host) = project_with(6000., 4000., 200.);
        let mut ty = slab_type(BarAlign::Edge, 200., false);
        ty.top = None;
        install(&mut q, host, ty);
        assert!(q.concrete.rebars.iter().all(|r| r.kind != RebarKind::Frog));
        // Meshes fit, the gap does not: the check says so and nothing is shortened.
        let (mut thin, host) = project_with(6000., 4000., 90.);
        let mut ty = slab_type(BarAlign::Edge, 200., false);
        for layer in [&mut ty.bottom, &mut ty.top] {
            let layer = layer.as_mut().unwrap();
            layer.diameter = 8.;
            layer.cross_diameter = Some(8.);
        }
        thin.elements
            .get_mut(&host)
            .unwrap()
            .properties
            .insert(REBAR_COVER.into(), PropertyValue::Number(20.));
        ty.frog_diameter = 8.;
        install(&mut thin, host, ty);
        assert!(
            thin.concrete
                .rebars
                .iter()
                .all(|r| r.kind != RebarKind::Frog)
        );
        assert!(
            model_checks(&thin)
                .iter()
                .any(|n| n.contains("нет места по высоте")),
            "{:?}",
            model_checks(&thin)
        );
    }

    #[test]
    fn openings_can_stay_continuous_and_a_side_can_be_turned_off() {
        let (mut p, host) = project_with(6000., 4000., 200.);
        p.details.insert(
            ObjectId(30),
            crate::Detail {
                id: ObjectId(30),
                kind: DetailKind::PrismCut {
                    part: host,
                    frame: Frame {
                        origin: Point3([200., 1600., -20.]),
                        ..Frame::default()
                    },
                    contour: vec![[0., 0.], [800., 0.], [800., 800.], [0., 800.]],
                    length: 240.,
                },
            },
        );
        let mut ty = slab_type(BarAlign::Edge, 200., false);
        ty.edge_bars = false;
        ty.frogs = false;
        install(&mut p, host, ty.clone());
        let continuous = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::BottomCross))
            .unwrap()
            .bars
            .len();
        ty.cut_openings = true;
        install(&mut p, host, ty);
        let cut = p
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::BottomCross))
            .unwrap();
        // Cross bars run along Y and meet the opening, so cutting adds bars.
        assert!(
            cut.bars.len() > continuous,
            "{} {}",
            cut.bars.len(),
            continuous
        );
        // Framing off on the first edge removes those U-bars and keeps the mesh.
        let (mut q, host) = project_with(6000., 4000., 200.);
        install(&mut q, host, slab_type(BarAlign::Edge, 200., false));
        let mesh = q
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::BottomMain))
            .unwrap()
            .bars
            .len();
        let zone = q.concrete.zones[0].number;
        q.concrete = set_zone_framing(&q, zone, vec![0], Vec::new()).unwrap();
        assert_eq!(
            q.concrete
                .rebars
                .iter()
                .find(|r| r.role == Some(ZoneRole::BottomMain))
                .unwrap()
                .bars
                .len(),
            mesh
        );
        let edge = q
            .concrete
            .rebars
            .iter()
            .find(|r| r.role == Some(ZoneRole::Edge))
            .unwrap();
        // The bottom edge (y ≈ cover) is off; the other sides stay.
        assert!(
            edge.bars
                .iter()
                .all(|bar| { bar.iter().map(|p| p.0[1]).fold(f64::MAX, f64::min) > 80. })
        );
    }
}
