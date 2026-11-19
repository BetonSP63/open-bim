//! Beam and column cages («каркасы»). A type stores the longitudinal bars
//! and the stirrups; placing it on a rectangular concrete member lays the
//! bars out inside the host cover (`rebar_cover`). The cage is built again
//! when the section, the length or the cover changes.
//!
//! A stirrup keeps references to the longitudinal bars it encloses. Its
//! centre-line size is the outside of those bars plus the stirrup diameter,
//! and the mandrel is large enough for the bar to sit in the bend (СП 63).
//! If the bars move so that the new size would be an invalid shape or would
//! leave the cover, the previous stirrup is kept and the model check says
//! the stirrup no longer wraps the bars.
use std::collections::BTreeMap;

use crate::concrete::{
    BarRef, BarShapeType, CageType, RebarCage, RebarCheck, RebarKind, ShapePlace,
};
use crate::{Frame, ModelError, ObjectId, Point3, Project, RebarRule, RebarSet};

use super::shape::{self, instantiate, resolve_shape};
use super::{host_changed, rect_member, set_lap, spread};

/// Longest delivered bar of a cage, mm. Longer bars are lapped.
const STOCK: f64 = 11_700.;

/// Replace or create the cage of one beam or column from its type.
pub fn install_cage(
    project: &Project,
    host: ObjectId,
    type_id: &str,
) -> Result<crate::Concrete, String> {
    if project.concrete.cage_types.iter().all(|t| t.id != type_id) {
        return Err("Нет типа каркаса".into());
    }
    rect_member(project, host)?;
    let concrete = project.concrete.clone();
    let number = concrete
        .cages
        .iter()
        .find(|c| c.host == host)
        .map(|c| c.number)
        .unwrap_or_else(|| concrete.cages.iter().map(|c| c.number).max().unwrap_or(0) + 1);
    let mut cage = concrete
        .cages
        .iter()
        .find(|c| c.number == number)
        .cloned()
        .unwrap_or(RebarCage {
            number,
            host,
            type_id: type_id.into(),
            checks: Vec::new(),
        });
    cage.host = host;
    cage.type_id = type_id.into();
    let mut staged = project.clone();
    staged.concrete = concrete;
    let mut rebars = std::mem::take(&mut staged.concrete.rebars);
    sync(&staged, None, &mut cage, &mut rebars)?;
    staged.concrete.rebars = rebars;
    staged.concrete.cages.retain(|c| c.number != number);
    staged.concrete.cages.push(cage);
    staged.concrete.rebars.sort_by_key(|r| r.number);
    staged.concrete.cages.sort_by_key(|c| c.number);
    super::schedule::write_positions(&mut staged);
    staged.concrete.validate().map_err(|e| e.to_string())?;
    Ok(staged.concrete)
}

pub(super) fn rebuild_cages(
    project: &Project,
    previous: &Project,
    cages: &mut Vec<RebarCage>,
    sets: &mut Vec<RebarSet>,
) -> Result<(), ModelError> {
    cages.retain(|c| project.elements.contains_key(&c.host));
    let alive: std::collections::BTreeSet<u32> = cages.iter().map(|c| c.number).collect();
    sets.retain(|s| s.cage.is_none_or(|n| alive.contains(&n)));
    for cage in cages.iter_mut() {
        if let Err(e) = sync(project, Some(previous), cage, sets) {
            return Err(ModelError::InvalidValue(format!(
                "Каркас {}: {e}",
                cage.number
            )));
        }
    }
    sets.sort_by_key(|r| r.number);
    cages.sort_by_key(|c| c.number);
    Ok(())
}

fn sync(
    project: &Project,
    previous: Option<&Project>,
    cage: &mut RebarCage,
    sets: &mut Vec<RebarSet>,
) -> Result<(), String> {
    let ty = project
        .concrete
        .cage_types
        .iter()
        .find(|t| t.id == cage.type_id)
        .cloned()
        .ok_or("Нет типа каркаса")?;
    let changed = previous.is_none_or(|prev| cage_changed(project, prev, cage, &ty));
    let old_long = sets
        .iter()
        .find(|s| s.cage == Some(cage.number) && s.kind == RebarKind::Longitudinal)
        .cloned();
    let old_stirrup = sets
        .iter()
        .find(|s| s.cage == Some(cage.number) && s.kind == RebarKind::Stirrup)
        .cloned();
    let mut long = if !changed && let Some(existing) = old_long.clone() {
        existing
    } else {
        longitudinal(project, cage, &ty, old_long.as_ref())?
    };
    long.number = nonzero(sets, long.number, &[]);
    let template = if changed {
        stirrup_template(cage, &ty, old_stirrup.as_ref())
    } else {
        old_stirrup
            .clone()
            .unwrap_or_else(|| stirrup_template(cage, &ty, None))
    };
    let kept_geometry = old_stirrup.as_ref().is_some_and(|s| !s.bars.is_empty());
    match transverse(project, cage, &ty, &long, &template) {
        Ok(mut stirrup) => {
            stirrup.number = nonzero(sets, stirrup.number, &[long.number]);
            cage.checks.clear();
            store(sets, long);
            store(sets, stirrup);
        }
        Err(e) if kept_geometry => {
            let Some(mut stirrup) = old_stirrup else {
                return Err(e);
            };
            stirrup.enclosed = links(&long);
            stirrup.host = Some(cage.host);
            stirrup.cage = Some(cage.number);
            cage.checks = vec![note(&e)];
            store(sets, long);
            store(sets, stirrup);
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

fn note(message: &str) -> RebarCheck {
    RebarCheck {
        message: message.to_string(),
        opening: None,
        edge: None,
        from: None,
        to: None,
    }
}

fn nonzero(sets: &[RebarSet], number: u32, extra: &[u32]) -> u32 {
    if number != 0 {
        return number;
    }
    sets.iter()
        .map(|s| s.number)
        .chain(extra.iter().copied())
        .max()
        .unwrap_or(0)
        + 1
}

fn cage_changed(project: &Project, previous: &Project, cage: &RebarCage, ty: &CageType) -> bool {
    let Some(prev) = previous
        .concrete
        .cages
        .iter()
        .find(|c| c.number == cage.number)
    else {
        return true;
    };
    if prev.type_id != cage.type_id || prev.host != cage.host {
        return true;
    }
    if host_changed(project, previous, cage.host) {
        return true;
    }
    let prev_ty = previous
        .concrete
        .cage_types
        .iter()
        .find(|t| t.id == cage.type_id);
    if prev_ty != Some(ty) {
        return true;
    }
    resolve_shape(project, &ty.stirrup_shape) != resolve_shape(previous, &ty.stirrup_shape)
}

fn store(sets: &mut Vec<RebarSet>, set: RebarSet) {
    if let Some(found) = sets.iter_mut().find(|s| s.number == set.number) {
        *found = set;
    } else {
        sets.push(set);
    }
}

fn links(long: &RebarSet) -> Vec<BarRef> {
    (0..long.bars.len() as u32)
        .map(|bar| BarRef {
            set: long.number,
            bar,
        })
        .collect()
}

fn rule(ty: &CageType, cover: f64) -> RebarRule {
    RebarRule {
        cover,
        spacing: ty
            .stirrup_spacing
            .max(ty.long_diameter)
            .max(ty.stirrup_diameter),
        per_face: ty.per_face,
        stock_length: Some(STOCK),
    }
}

fn longitudinal(
    project: &Project,
    cage: &RebarCage,
    ty: &CageType,
    old: Option<&RebarSet>,
) -> Result<RebarSet, String> {
    let (frame, width, height, length) = rect_member(project, cage.host)?;
    let cover = super::host_cover(project, cage.host);
    let (half_x, half_y) = halves(width, height, cover, ty.stirrup_diameter, ty.long_diameter)?;
    if length <= 2. * cover + 1. {
        return Err("Длина меньше защитного слоя".into());
    }
    let mut set = RebarSet::basic(
        RebarKind::Longitudinal,
        ty.long_diameter,
        ty.long_grade.clone(),
    );
    set.number = old.map(|s| s.number).unwrap_or(0);
    set.host = Some(cage.host);
    set.cage = Some(cage.number);
    set.shape_id = Some("gost.straight".into());
    set.rule = Some(rule(ty, cover));
    let lap = set_lap(project, &set);
    let mut bars = Vec::new();
    for [x, y] in spots(half_x, half_y, ty.per_face) {
        let a = [x, y, cover];
        let b = [x, y, length - cover];
        for (a, b, _, _) in super::split(a, b, Some(STOCK), lap) {
            bars.push(world(&frame, vec![a, b]));
        }
    }
    if bars.is_empty() {
        return Err("Сечение меньше защитного слоя".into());
    }
    let piece = RebarSet::bar_length(&bars[0]);
    set.shape_params.insert("A".into(), piece);
    set.bars = bars;
    Ok(set)
}

fn stirrup_template(cage: &RebarCage, ty: &CageType, old: Option<&RebarSet>) -> RebarSet {
    let mut set = RebarSet::basic(
        RebarKind::Stirrup,
        ty.stirrup_diameter,
        ty.stirrup_grade.clone(),
    );
    set.number = old.map(|s| s.number).unwrap_or(0);
    set.host = Some(cage.host);
    set.cage = Some(cage.number);
    set.shape_id = Some(ty.stirrup_shape.clone());
    set
}

/// Stirrup (or straight tie) sized from the longitudinal bars.
fn transverse(
    project: &Project,
    cage: &RebarCage,
    ty: &CageType,
    long: &RebarSet,
    template: &RebarSet,
) -> Result<RebarSet, String> {
    let mut shape = resolve_shape(
        project,
        template.shape_id.as_deref().unwrap_or(&ty.stirrup_shape),
    )
    .ok_or("Неизвестный тип формы хомута")?;
    let (frame, width, height, length) = rect_member(project, cage.host)?;
    let cover = super::host_cover(project, cage.host);
    let bars = section_bars(long, &frame)?;
    if bars.is_empty() {
        return Err("Хомут не охватывает стержни: нет продольного стержня".into());
    }
    let rect = rectangle(&bars, template.diameter);
    if !fits_cover(rect, width, height, cover, template.diameter) {
        return Err("Хомут не охватывает стержни: выходит за защитный слой".into());
    }
    let mandrel = stirrup_mandrel(&shape, &template.grade, template.diameter, &bars)?;
    let closed = closed_rectangle(&shape);
    let straight = straight_shape(&shape);
    // The bend, including the hooks, uses this mandrel: the code minimum
    // raised so the longitudinal bar fits inside the bend.
    shape.mandrel = Some(mandrel);
    if !closed && !straight {
        return Err(
            "Хомут каркаса — замкнутая прямоугольная форма (как «Хомут») или прямой стержень"
                .into(),
        );
    }
    if closed {
        let radius = (mandrel + template.diameter) / 2.;
        if rect.a < 2. * radius - 0.5 || rect.b < 2. * radius - 0.5 {
            return Err("Хомут не охватывает стержни: загиб не помещается между стержнями".into());
        }
    } else if rect.a < 1. {
        return Err("Хомут не охватывает стержни: слишком короткий стержень".into());
    }
    let stations = stations(
        cover,
        length - cover,
        ty.stirrup_spacing,
        ty.end_spacing,
        ty.end_length,
    );
    if stations.is_empty() {
        return Err("Длина меньше защитного слоя".into());
    }
    let mut params = BTreeMap::new();
    if closed {
        params.insert(shape.segments[0].letter.clone(), rect.a);
        params.insert(shape.segments[1].letter.clone(), rect.b);
    } else {
        params.insert(shape.segments[0].letter.clone(), rect.a);
    }
    let mut out = template.clone();
    out.bars.clear();
    out.shape_params = params.clone();
    out.enclosed = links(long);
    out.rule = Some(rule(ty, cover));
    for z in stations {
        let (origin, axis_x, axis_y) = if closed {
            ([rect.left, rect.bottom, z], frame.axes[0], frame.axes[1])
        } else {
            (
                [rect.left, (rect.bottom + rect.top) / 2., z],
                frame.axes[0],
                frame.axes[1],
            )
        };
        let place = ShapePlace {
            origin: frame.world(Point3(origin)),
            x: axis_x,
            y: axis_y,
            local_to_host: false,
        };
        let made = instantiate(&shape, &params, template.diameter, &template.grade, &place)?;
        out.hooks = made.hooks;
        out.bend_diameter = made.mandrel;
        out.bars.push(made.bar);
    }
    Ok(out)
}

struct Located {
    x: f64,
    y: f64,
    diameter: f64,
}

#[derive(Clone, Copy)]
struct Rect {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
    a: f64,
    b: f64,
}

fn section_bars(long: &RebarSet, frame: &Frame) -> Result<Vec<Located>, String> {
    let mut out = Vec::new();
    for bar in &long.bars {
        let (x, y) = bar_section(frame, bar)?;
        out.push(Located {
            x,
            y,
            diameter: long.diameter,
        });
    }
    Ok(out)
}

/// Section position of the long straight run. Hooks and short legs are ignored.
fn bar_section(frame: &Frame, bar: &[Point3]) -> Result<(f64, f64), String> {
    let local: Vec<[f64; 3]> = bar.iter().map(|p| frame.local(*p).0).collect();
    let mut best: Option<(f64, [f64; 3], [f64; 3])> = None;
    for w in local.windows(2) {
        let d = sub(w[1], w[0]);
        let len = len(d);
        if best.as_ref().is_none_or(|(have, _, _)| len > *have) {
            best = Some((len, w[0], w[1]));
        }
    }
    let Some((length, a, b)) = best else {
        return Err("Хомут не охватывает стержни: нет продольного стержня".into());
    };
    if length < 1. {
        return Err("Хомут не охватывает стержни: нет продольного стержня".into());
    }
    let dir = mul(sub(b, a), 1. / length);
    if dir[2].abs() < 0.99 {
        return Err("Хомут не охватывает стержни: стержень не идёт вдоль детали".into());
    }
    let mid = mul(add(a, b), 0.5);
    Ok((mid[0], mid[1]))
}

/// Centre line outside every enclosed bar by half a stirrup diameter.
fn rectangle(bars: &[Located], stirrup_d: f64) -> Rect {
    let left = extreme(bars, stirrup_d, -1., true);
    let right = extreme(bars, stirrup_d, 1., true);
    let bottom = extreme(bars, stirrup_d, -1., false);
    let top = extreme(bars, stirrup_d, 1., false);
    Rect {
        left,
        right,
        bottom,
        top,
        a: right - left,
        b: top - bottom,
    }
}

fn extreme(bars: &[Located], stirrup_d: f64, sign: f64, along_x: bool) -> f64 {
    bars.iter()
        .map(|b| {
            let coord = if along_x { b.x } else { b.y };
            coord + sign * (b.diameter / 2. + stirrup_d / 2.)
        })
        .fold(if sign < 0. { f64::MAX } else { f64::MIN }, |acc, v| {
            if sign < 0. { acc.min(v) } else { acc.max(v) }
        })
}

fn fits_cover(rect: Rect, width: f64, height: f64, cover: f64, stirrup_d: f64) -> bool {
    let need = cover + stirrup_d / 2. - 1.;
    rect.left + width / 2. >= need
        && width / 2. - rect.right >= need
        && rect.bottom + height / 2. >= need
        && height / 2. - rect.top >= need
}

fn stirrup_mandrel(
    shape: &BarShapeType,
    grade: &str,
    diameter: f64,
    bars: &[Located],
) -> Result<f64, String> {
    let code = shape::mandrel_of(shape, grade, diameter)?;
    let bar = bars.iter().map(|b| b.diameter).fold(0., f64::max);
    // The longitudinal bar has to sit in the bend: the inner radius is
    // half the mandrel, so the mandrel is at least the bar diameter.
    Ok(code.max(bar))
}

fn closed_rectangle(shape: &BarShapeType) -> bool {
    if shape.segments.len() != 4 {
        return false;
    }
    let bends_ok = shape
        .segments
        .iter()
        .take(3)
        .all(|s| (s.bend_deg - 90.).abs() < 1.);
    let paired = shape.segments[0].letter == shape.segments[2].letter
        && shape.segments[1].letter == shape.segments[3].letter
        && shape.segments[0].letter != shape.segments[1].letter;
    bends_ok && paired
}

fn straight_shape(shape: &BarShapeType) -> bool {
    shape.segments.len() == 1 && shape.segments[0].bend_deg.abs() < 1.
}

fn halves(
    width: f64,
    height: f64,
    cover: f64,
    stirrup_d: f64,
    long_d: f64,
) -> Result<(f64, f64), String> {
    let inset = cover + stirrup_d + long_d / 2.;
    let (x, y) = (width / 2. - inset, height / 2. - inset);
    if x <= 0. || y <= 0. {
        return Err("Сечение меньше защитного слоя".into());
    }
    Ok((x, y))
}

fn spots(half_x: f64, half_y: f64, per_face: u32) -> Vec<[f64; 2]> {
    let n = per_face.max(2) as usize - 1;
    let mut spots = Vec::new();
    for i in 0..=n {
        let t = i as f64 / n as f64;
        for p in [
            [-half_x + 2. * half_x * t, -half_y],
            [half_x, -half_y + 2. * half_y * t],
            [half_x - 2. * half_x * t, half_y],
            [-half_x, half_y - 2. * half_y * t],
        ] {
            if !spots
                .iter()
                .any(|q: &[f64; 2]| (q[0] - p[0]).abs() < 1e-6 && (q[1] - p[1]).abs() < 1e-6)
            {
                spots.push(p);
            }
        }
    }
    spots
}

fn stations(
    from: f64,
    to: f64,
    spacing: f64,
    end_spacing: Option<f64>,
    end_length: Option<f64>,
) -> Vec<f64> {
    let Some((end_spacing, end_length)) = end_spacing
        .zip(end_length)
        .filter(|(spacing, length)| *spacing > 1. && *length > 1.)
    else {
        return spread(from, to, spacing);
    };
    if from + 2. * end_length >= to - 1. {
        return spread(from, to, end_spacing);
    }
    let mut v = spread(from, from + end_length, end_spacing);
    for p in spread(from + end_length, to - end_length, spacing)
        .into_iter()
        .chain(spread(to - end_length, to, end_spacing))
    {
        if v.last().is_none_or(|q| (p - *q).abs() > 1.) {
            v.push(p);
        }
    }
    v
}

fn world(frame: &Frame, bar: Vec<[f64; 3]>) -> Vec<Point3> {
    bar.into_iter().map(|p| frame.world(Point3(p))).collect()
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn len(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::concrete::{CageType, REBAR_COVER};
    use crate::{
        Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId, Profile,
        ProfileShape, Project, PropertyValue, RebarKind,
    };

    fn host(width: f64, height: f64, length: f64) -> (Project, ObjectId) {
        let mut project = Project {
            name: "Каркас".into(),
            next_id: 20,
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
                name: "сечение".into(),
                shape: ProfileShape::Rectangle { height, width },
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
                name: "Балка".into(),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    profile: ObjectId(2),
                    length: LengthMm::new(length).unwrap(),
                    frame: Frame::default(),
                    position: Default::default(),
                },
                properties,
            },
        );
        (project, id)
    }

    fn ty() -> CageType {
        let mut ty = CageType::new("c", "Каркас балки");
        ty.long_diameter = 20.;
        ty.stirrup_diameter = 10.;
        ty.stirrup_spacing = 200.;
        ty
    }

    fn install(width: f64, height: f64) -> Project {
        let (mut project, id) = host(width, height, 3000.);
        project.concrete.cage_types.push(ty());
        project.concrete = install_cage(&project, id, "c").unwrap();
        project
    }

    fn kind(project: &Project, wanted: RebarKind) -> &RebarSet {
        project
            .concrete
            .rebars
            .iter()
            .find(|s| s.kind == wanted)
            .unwrap()
    }

    fn extent(bar: &[Point3]) -> (f64, f64) {
        let (mut min_x, mut max_x) = (f64::MAX, f64::MIN);
        let (mut min_y, mut max_y) = (f64::MAX, f64::MIN);
        for p in bar {
            min_x = min_x.min(p.0[0]);
            max_x = max_x.max(p.0[0]);
            min_y = min_y.min(p.0[1]);
            max_y = max_y.max(p.0[1]);
        }
        (max_x - min_x, max_y - min_y)
    }

    #[test]
    fn cage_places_bars_inside_the_cover_and_stirrups_on_them() {
        let project = install(400., 600.);
        let long = kind(&project, RebarKind::Longitudinal);
        assert_eq!(long.bars.len(), 4);
        assert_eq!(long.cage, Some(1));
        for bar in &long.bars {
            assert!((RebarSet::bar_length(bar) - 2940.).abs() < 1e-6);
            assert!((bar[0].0[0].abs() - 150.).abs() < 1e-6, "{}", bar[0].0[0]);
            assert!((bar[0].0[1].abs() - 250.).abs() < 1e-6, "{}", bar[0].0[1]);
        }
        let stirrup = kind(&project, RebarKind::Stirrup);
        assert_eq!(stirrup.bars.len(), 16);
        assert_eq!(stirrup.enclosed.len(), 4);
        assert!(stirrup.enclosed.iter().all(|r| r.set == long.number));
        let (a, b) = extent(&stirrup.bars[0]);
        // A = 400 − 2·30 − 10, B = 600 − 2·30 − 10.
        assert!((a - 330.).abs() < 1e-3, "{a}");
        assert!((b - 530.).abs() < 1e-3, "{b}");
        let z: Vec<f64> = stirrup.bars.iter().map(|bar| bar[0].0[2]).collect();
        assert!((z[0] - 30.).abs() < 1e-6);
        assert!((z[z.len() - 1] - 2970.).abs() < 1e-6);
        let rows = super::super::detail_schedule(&project);
        assert!(
            rows.iter()
                .any(|r| r.code == "ПР" && (r.diameter - 20.).abs() < 1e-6)
        );
        assert!(
            rows.iter()
                .any(|r| r.code == "Х" && (r.diameter - 10.).abs() < 1e-6)
        );
        let steel = super::super::steel_schedule(&project);
        assert!(
            steel
                .by_element
                .iter()
                .any(|r| r.element == "Балка" && r.bars > 0)
        );
        let bvbs = super::super::bvbs::file(&project);
        assert!(bvbs.contains("@d20@") && bvbs.contains("@d10@"));
        assert!(
            bvbs.lines()
                .filter(|line| !line.is_empty())
                .all(super::super::bvbs::checksum_matches)
        );
        let again = Project::from_json(&project.to_json().unwrap()).unwrap();
        assert_eq!(again.concrete.rebars.len(), project.concrete.rebars.len());
    }

    #[test]
    fn three_bars_per_face_keep_the_corners() {
        let (mut project, id) = host(400., 600., 3000.);
        let mut ty = ty();
        ty.per_face = 3;
        project.concrete.cage_types.push(ty);
        project.concrete = install_cage(&project, id, "c").unwrap();
        assert_eq!(kind(&project, RebarKind::Longitudinal).bars.len(), 8);
    }

    #[test]
    fn section_cover_and_length_rebuild_the_cage() {
        let mut project = install(400., 600.);
        let previous = project.clone();
        project
            .elements
            .get_mut(&ObjectId(10))
            .unwrap()
            .properties
            .insert(REBAR_COVER.into(), PropertyValue::Number(40.));
        super::super::rebuild_rebars(&mut project, &previous).unwrap();
        let (a, b) = extent(&kind(&project, RebarKind::Stirrup).bars[0]);
        assert!((a - 310.).abs() < 1e-3, "{a}");
        assert!((b - 510.).abs() < 1e-3, "{b}");
        assert!(project.concrete.cages[0].checks.is_empty());

        let previous = project.clone();
        if let ElementShape::Member { length, .. } =
            &mut project.elements.get_mut(&ObjectId(10)).unwrap().shape
        {
            *length = LengthMm::new(4000.).unwrap();
        }
        super::super::rebuild_rebars(&mut project, &previous).unwrap();
        assert_eq!(kind(&project, RebarKind::Stirrup).bars.len(), 21);
        // Cover is already 40 mm, so the bar runs 4000 − 2·40.
        assert!(
            (RebarSet::bar_length(&kind(&project, RebarKind::Longitudinal).bars[0]) - 3920.).abs()
                < 1e-6
        );
    }

    #[test]
    fn dense_end_zones_tighten_the_spacing() {
        let (mut project, id) = host(400., 600., 3000.);
        let mut ty = ty();
        ty.end_spacing = Some(100.);
        ty.end_length = Some(400.);
        project.concrete.cage_types.push(ty);
        project.concrete = install_cage(&project, id, "c").unwrap();
        let stirrup = kind(&project, RebarKind::Stirrup);
        assert_eq!(stirrup.bars.len(), 20);
        let z: Vec<f64> = stirrup.bars.iter().map(|bar| bar[0].0[2]).collect();
        assert!((z[1] - z[0] - 100.).abs() < 1e-6);
        assert!(z.windows(2).any(|w| (w[1] - w[0] - 200.).abs() < 1e-6));
    }

    #[test]
    fn a_stirrup_follows_a_bar_that_moves_inward() {
        let mut project = install(400., 600.);
        let previous = project.clone();
        let long = project
            .concrete
            .rebars
            .iter_mut()
            .find(|s| s.kind == RebarKind::Longitudinal)
            .unwrap();
        for bar in &mut long.bars {
            if bar[0].0[0] > 0. {
                for p in bar.iter_mut() {
                    p.0[0] -= 50.;
                }
            }
        }
        super::super::rebuild_rebars(&mut project, &previous).unwrap();
        let (a, b) = extent(&kind(&project, RebarKind::Stirrup).bars[0]);
        assert!((a - 280.).abs() < 1e-3, "{a}");
        assert!((b - 530.).abs() < 1e-3, "{b}");
        assert!(project.concrete.cages[0].checks.is_empty());
    }

    #[test]
    fn a_stirrup_is_not_resized_into_an_invalid_shape() {
        let mut project = install(400., 600.);
        let previous = project.clone();
        let long = project
            .concrete
            .rebars
            .iter_mut()
            .find(|s| s.kind == RebarKind::Longitudinal)
            .unwrap();
        let bar = long
            .bars
            .iter_mut()
            .max_by(|a, b| a[0].0[0].total_cmp(&b[0].0[0]))
            .unwrap();
        for p in bar.iter_mut() {
            p.0[0] += 40.;
        }
        super::super::rebuild_rebars(&mut project, &previous).unwrap();
        let (a, _) = extent(&kind(&project, RebarKind::Stirrup).bars[0]);
        assert!((a - 330.).abs() < 1e-3, "{a}");
        let note = &project.concrete.cages[0].checks[0].message;
        assert!(note.contains("не охватывает"), "{note}");

        let mut tight = install(400., 600.);
        let previous = tight.clone();
        if let ProfileShape::Rectangle { width, height } =
            &mut tight.profiles.get_mut(&ObjectId(2)).unwrap().shape
        {
            *width = 120.;
            *height = 120.;
        }
        super::super::rebuild_rebars(&mut tight, &previous).unwrap();
        let (a, _) = extent(&kind(&tight, RebarKind::Stirrup).bars[0]);
        assert!((a - 330.).abs() < 1e-3, "{a}");
        let note = &tight.concrete.cages[0].checks[0].message;
        assert!(note.contains("загиб"), "{note}");
        assert!(
            super::super::model_checks(&tight)
                .iter()
                .any(|n| n.contains("загиб"))
        );
    }

    #[test]
    fn a_bar_that_leaves_the_axis_is_reported() {
        let mut project = install(400., 600.);
        let previous = project.clone();
        let long = project
            .concrete
            .rebars
            .iter_mut()
            .find(|s| s.kind == RebarKind::Longitudinal)
            .unwrap();
        long.bars[0][1].0[0] += 800.;
        super::super::rebuild_rebars(&mut project, &previous).unwrap();
        let note = &project.concrete.cages[0].checks[0].message;
        assert!(note.contains("вдоль"), "{note}");
        let (a, _) = extent(&kind(&project, RebarKind::Stirrup).bars[0]);
        assert!((a - 330.).abs() < 1e-3, "{a}");
    }

    #[test]
    fn a_straight_tie_takes_its_length_from_the_bars() {
        let (mut project, id) = host(400., 600., 3000.);
        let mut ty = ty();
        ty.stirrup_shape = "gost.straight".into();
        project.concrete.cage_types.push(ty);
        project.concrete = install_cage(&project, id, "c").unwrap();
        let tie = kind(&project, RebarKind::Stirrup);
        assert_eq!(tie.enclosed.len(), 4);
        let (a, b) = extent(&tie.bars[0]);
        assert!((a - 330.).abs() < 1e-3, "{a}");
        assert!(b < 1., "{b}");
    }

    #[test]
    fn a_section_smaller_than_the_cover_is_refused() {
        let (mut project, id) = host(80., 80., 3000.);
        project.concrete.cage_types.push(ty());
        let err = install_cage(&project, id, "c").unwrap_err();
        assert!(err.contains("защитного слоя"), "{err}");
    }
}
