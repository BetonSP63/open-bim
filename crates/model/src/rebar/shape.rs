//! Bar shape types («типы форм»): the centreline, the segment parameters
//! A/B/C…, the hooks and the mandrel. A set stores the type id. Lengths in
//! `shape_params` stay with the set; every other change of the type rebuilds
//! the set. The built-in catalogue is the common КЖ forms. ГОСТ does not
//! number these shapes; the bend rules are those of СП 63 already used for
//! hooks and mandrels, and the bar steel is ГОСТ 34028. The `code` field is
//! a stable mark for a later detail schedule and BVBS, not a GOST number.
use std::collections::BTreeMap;

use crate::concrete::{BarShapeType, ShapePlace, ShapeSegment};
use crate::{Concrete, Hook, Point3, Project};

use super::{hook_points, min_bend_diameter, rebar_class};

/// Whether `id` is a built-in form or a form stored on the project.
pub fn shape_known(concrete: &Concrete, id: &str) -> bool {
    concrete.shape_types.iter().any(|s| s.id == id) || builtin(id).is_some()
}

/// The project copy wins over the built-in form with the same id.
pub fn resolve_shape(project: &Project, id: &str) -> Option<BarShapeType> {
    project
        .concrete
        .shape_types
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .or_else(|| builtin(id))
}

/// Built-in forms, in the order shown in the editor.
pub fn builtin_shapes() -> Vec<BarShapeType> {
    [
        "gost.straight",
        "gost.l",
        "gost.u",
        "gost.stirrup",
        "gost.frog",
        "gost.crank",
        "gost.hooks",
    ]
    .into_iter()
    .filter_map(builtin)
    .collect()
}

fn seg(letter: &str, length: f64, bend_deg: f64) -> ShapeSegment {
    ShapeSegment {
        letter: letter.into(),
        length,
        bend_deg,
    }
}

fn form(
    id: &str,
    name: &str,
    code: &str,
    segments: Vec<ShapeSegment>,
    hooks: [Hook; 2],
) -> BarShapeType {
    BarShapeType {
        id: id.into(),
        name: name.into(),
        code: code.into(),
        segments,
        hooks,
        mandrel: None,
    }
}

fn builtin(id: &str) -> Option<BarShapeType> {
    let none = [Hook::default(); 2];
    Some(match id {
        "gost.straight" => form(
            "gost.straight",
            "Прямой",
            "ПР",
            vec![seg("A", 1000., 0.)],
            none,
        ),
        "gost.l" => form(
            "gost.l",
            "Г-образный",
            "Г",
            vec![seg("A", 1000., 90.), seg("B", 500., 0.)],
            none,
        ),
        // Legs A and C, vertical B. Layout fills A and C with l_an and B
        // with the height between the mesh layers.
        "gost.u" => form(
            "gost.u",
            "П-образный",
            "П",
            vec![seg("A", 400., 90.), seg("B", 200., 90.), seg("C", 400., 0.)],
            none,
        ),
        "gost.stirrup" => form(
            "gost.stirrup",
            "Хомут",
            "Х",
            vec![
                seg("A", 200., 90.),
                seg("B", 400., 90.),
                seg("A", 200., 90.),
                seg("B", 400., 0.),
            ],
            [Hook::new(super::HookKind::Deg135); 2],
        ),
        // Feet A, risers B (one parameter), seat C. Layout fills B from the
        // gap between the meshes.
        "gost.frog" => form(
            "gost.frog",
            "Лягушка",
            "Л",
            vec![
                seg("A", 150., 90.),
                seg("B", 100., -90.),
                seg("C", 200., -90.),
                seg("B", 100., 90.),
                seg("A", 150., 0.),
            ],
            none,
        ),
        "gost.crank" => form(
            "gost.crank",
            "С отгибом",
            "О",
            vec![
                seg("A", 500., 45.),
                seg("B", 200., -45.),
                seg("C", 500., 0.),
            ],
            none,
        ),
        "gost.hooks" => form(
            "gost.hooks",
            "Прямой с крюками",
            "К",
            vec![seg("A", 1000., 0.)],
            [Hook::new(super::HookKind::Deg180); 2],
        ),
        _ => return None,
    })
}

fn length_of(seg: &ShapeSegment, params: &BTreeMap<String, f64>) -> f64 {
    params.get(&seg.letter).copied().unwrap_or(seg.length)
}

/// Centreline corners in the shape plane, before hooks. The first segment
/// runs along +X from the origin; a positive angle turns left.
pub fn centerline(
    shape: &BarShapeType,
    params: &BTreeMap<String, f64>,
) -> Result<Vec<[f64; 2]>, String> {
    if shape.segments.is_empty() {
        return Err("У типа формы нет участков".into());
    }
    let mut pts = vec![[0., 0.]];
    let mut dir = [1., 0.];
    for seg in &shape.segments {
        let len = length_of(seg, params);
        if !(1.0..=100_000.).contains(&len) {
            return Err(format!("Длина участка {} вне диапазона", seg.letter));
        }
        let last = *pts.last().unwrap();
        pts.push([last[0] + dir[0] * len, last[1] + dir[1] * len]);
        if seg.bend_deg.abs() > 1e-9 {
            let a = seg.bend_deg.to_radians();
            let (c, s) = (a.cos(), a.sin());
            dir = [dir[0] * c - dir[1] * s, dir[0] * s + dir[1] * c];
        }
    }
    Ok(pts)
}

fn unit2(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = v[0].hypot(v[1]);
    (l > 1e-9).then_some([v[0] / l, v[1] / l])
}

/// Hooks bend in the shape plane, to the left of the bar direction.
fn with_hooks(pts: &[[f64; 2]], hooks: [Hook; 2], diameter: f64, radius: f64) -> Vec<[f64; 3]> {
    let lift = |p: [f64; 2]| [p[0], p[1], 0.];
    if hooks.iter().all(Hook::is_none) || pts.len() < 2 {
        return pts.iter().copied().map(lift).collect();
    }
    let n = pts.len();
    let left = |d: [f64; 2]| [-d[1], d[0], 0.];
    let d0 = unit2([pts[1][0] - pts[0][0], pts[1][1] - pts[0][1]]).unwrap_or([1., 0.]);
    let d1 =
        unit2([pts[n - 1][0] - pts[n - 2][0], pts[n - 1][1] - pts[n - 2][1]]).unwrap_or([1., 0.]);
    let mut start = hook_points(
        lift(pts[0]),
        [-d0[0], -d0[1], 0.],
        left(d0),
        hooks[0],
        diameter,
        radius,
    );
    start.reverse();
    start.extend(pts[1..n - 1].iter().copied().map(lift));
    start.extend(hook_points(
        lift(pts[n - 1]),
        [d1[0], d1[1], 0.],
        left(d1),
        hooks[1],
        diameter,
        radius,
    ));
    start
}

fn place_point(place: &ShapePlace, p: [f64; 3]) -> Point3 {
    Point3(std::array::from_fn(|i| {
        place.origin.0[i] + place.x[i] * p[0] + place.y[i] * p[1]
    }))
}

/// Effective mandrel: the type value when it is at least the minimum of
/// СП 63, otherwise the minimum. `Err` when the type asks for a tighter bend.
pub fn mandrel_of(shape: &BarShapeType, grade: &str, diameter: f64) -> Result<f64, String> {
    let min = min_bend_diameter(rebar_class(grade), diameter);
    match shape.mandrel {
        Some(m) if m + 1e-6 < min => Err(format!(
            "Диаметр оправки {} мм меньше наименьшего {} мм по СП 63",
            m.round(),
            min.round()
        )),
        Some(m) => Ok(m),
        None => Ok(min),
    }
}

/// One bar built from a shape type.
pub struct Instantiated {
    pub bar: Vec<Point3>,
    pub hooks: [Hook; 2],
    /// Mandrel to store; `None` when it is the minimum of the class.
    pub mandrel: Option<f64>,
}

/// One bar of the shape at `place`.
pub fn instantiate(
    shape: &BarShapeType,
    params: &BTreeMap<String, f64>,
    diameter: f64,
    grade: &str,
    place: &ShapePlace,
) -> Result<Instantiated, String> {
    let line = centerline(shape, params)?;
    let mandrel = mandrel_of(shape, grade, diameter)?;
    let min = min_bend_diameter(rebar_class(grade), diameter);
    let radius = (mandrel + diameter) / 2.;
    let local = with_hooks(&line, shape.hooks, diameter, radius);
    let bar = local.into_iter().map(|p| place_point(place, p)).collect();
    let stored = ((mandrel - min).abs() > 1e-6).then_some(mandrel);
    Ok(Instantiated {
        bar,
        hooks: shape.hooks,
        mandrel: stored,
    })
}

/// World placement of a set. A host-local placement follows the host frame.
pub fn world_place(
    project: &Project,
    host: Option<crate::ObjectId>,
    place: &ShapePlace,
) -> Result<ShapePlace, String> {
    if !place.local_to_host {
        return Ok(place.clone());
    }
    let host = host.ok_or("Стержень по типу формы не привязан к детали")?;
    let frame = project
        .elements
        .get(&host)
        .ok_or("Деталь не найдена")?
        .shape
        .frame();
    let origin = frame.world(place.origin);
    let axis = |v: [f64; 3]| {
        let w = frame.world(Point3(v)).0;
        std::array::from_fn(|i| w[i] - frame.origin.0[i])
    };
    Ok(ShapePlace {
        origin,
        x: axis(place.x),
        y: axis(place.y),
        local_to_host: false,
    })
}

/// Rebuild one shape instance from its type. Length overrides on the set
/// are kept; hooks, bends and the mandrel come from the type.
pub fn rebuild_instance(
    project: &Project,
    set: &crate::RebarSet,
) -> Result<crate::RebarSet, String> {
    let id = set.shape_id.as_deref().ok_or("У стержня нет типа формы")?;
    let shape = resolve_shape(project, id).ok_or("Неизвестный тип формы")?;
    let place = set.place.as_ref().ok_or("У стержня нет положения")?;
    let world = world_place(project, set.host, place)?;
    let made = instantiate(&shape, &set.shape_params, set.diameter, &set.grade, &world)?;
    let mut out = set.clone();
    out.bars = vec![made.bar];
    out.hooks = made.hooks;
    out.bend_diameter = made.mandrel;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HookKind, RebarKind, RebarSet};

    fn params(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| ((*k).into(), *v)).collect()
    }

    #[test]
    fn catalogue_covers_the_common_forms() {
        let ids: Vec<_> = builtin_shapes().into_iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            [
                "gost.straight",
                "gost.l",
                "gost.u",
                "gost.stirrup",
                "gost.frog",
                "gost.crank",
                "gost.hooks"
            ]
        );
        let u = builtin("gost.u").unwrap();
        assert_eq!(u.segments.len(), 3);
        let frog = builtin("gost.frog").unwrap();
        assert_eq!(frog.segments.iter().filter(|s| s.letter == "B").count(), 2);
        assert_eq!(
            builtin("gost.stirrup").unwrap().hooks[0].kind,
            HookKind::Deg135
        );
    }

    #[test]
    fn u_and_frog_centerlines_follow_the_parameters() {
        let u = builtin("gost.u").unwrap();
        let pts = centerline(&u, &params(&[("A", 500.), ("B", 140.), ("C", 500.)])).unwrap();
        assert_eq!(pts.len(), 4);
        assert!((pts[1][0] - 500.).abs() < 1e-6 && pts[1][1].abs() < 1e-6);
        assert!((pts[2][0] - 500.).abs() < 1e-6 && (pts[2][1] - 140.).abs() < 1e-6);
        assert!(pts[3][0].abs() < 1e-6 && (pts[3][1] - 140.).abs() < 1e-6);

        let frog = builtin("gost.frog").unwrap();
        let pts = centerline(&frog, &params(&[("B", 80.)])).unwrap();
        // Feet 150, seat 200, risers 80: plan length 2·150+200, seat at y = 80.
        assert!((pts.last().unwrap()[0] - 500.).abs() < 1e-6);
        assert!(pts.last().unwrap()[1].abs() < 1e-6);
        assert!((pts[2][1] - 80.).abs() < 1e-6 && (pts[3][1] - 80.).abs() < 1e-6);
        assert!((pts[3][0] - pts[2][0] - 200.).abs() < 1e-6);
    }

    #[test]
    fn an_instance_follows_the_type_and_keeps_its_own_length() {
        let mut project = Project::default();
        let mut shape = builtin("gost.l").unwrap();
        shape.id = "user.l".into();
        project.concrete.shape_types.push(shape);
        let place = ShapePlace {
            origin: Point3([0.; 3]),
            x: [1., 0., 0.],
            y: [0., 1., 0.],
            local_to_host: false,
        };
        let bar = instantiate(
            &project.concrete.shape_types[0],
            &BTreeMap::new(),
            12.,
            "А500С",
            &place,
        )
        .unwrap()
        .bar;
        let mut set = RebarSet::basic(RebarKind::Shape, 12., "А500С");
        set.shape_id = Some("user.l".into());
        set.place = Some(place);
        set.bars = vec![bar];
        project.concrete.rebars.push(set);
        let previous = project.clone();
        project.concrete.shape_types[0].segments[0].length = 800.;
        let rebuilt = rebuild_instance(&project, &project.concrete.rebars[0]).unwrap();
        let end = rebuilt.bars[0].last().unwrap().0;
        assert!(
            (end[0] - 800.).abs() < 1e-6 && (end[1] - 500.).abs() < 1e-6,
            "{end:?}"
        );
        // An override stays when the type default changes.
        project.concrete.rebars[0]
            .shape_params
            .insert("A".into(), 400.);
        let rebuilt = rebuild_instance(&project, &project.concrete.rebars[0]).unwrap();
        assert!((rebuilt.bars[0][1].0[0] - 400.).abs() < 1e-6);
        // The unchanged project still has the old length.
        assert!((previous.concrete.shape_types[0].segments[0].length - 1000.).abs() < 1e-9);
        // A stored instance is rebuilt with the project when the type changes.
        project.concrete.rebars[0].bars = vec![vec![Point3([0.; 3]), Point3([1., 0., 0.])]];
        project.concrete.rebars[0].shape_params.clear();
        let before = project.clone();
        project.concrete.shape_types[0].segments[0].length = 700.;
        crate::rebar::rebuild_rebars(&mut project, &before).unwrap();
        let end = project.concrete.rebars[0].bars[0].last().unwrap().0;
        assert!(
            (end[0] - 700.).abs() < 1e-6 && (end[1] - 500.).abs() < 1e-6,
            "{end:?}"
        );
    }
}
