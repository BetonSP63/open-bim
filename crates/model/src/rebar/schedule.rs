//! Reinforcement positions, the detail schedule («ведомость деталей») and
//! the steel-consumption schedule («ведомость расхода стали»).
//!
//! Identical bars share one position: the same shape type, diameter, steel
//! class, segment lengths and hooks. Lengths are measured on the centreline
//! stored in the model. The sketch is built from that centreline — segments,
//! bend angles, hooks and the mandrel — not from a drawn picture.
use std::collections::{BTreeMap, BTreeSet};

use crate::concrete::{BarShapeType, ShapeSegment};
use crate::{ObjectId, Point3, Project, RebarSet};

use super::shape::{self, mandrel_of, resolve_shape};
use super::{Hook, HookKind, mass_per_m, min_bend_diameter, rebar_class};

/// One straight portion between bend tangents.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentLen {
    pub letter: String,
    /// Tangent length, mm, rounded to 1 mm.
    pub length_mm: f64,
    /// Turn at the end of the portion, degrees. 0 — the bar ends.
    pub bend_deg: f64,
}

/// A text mark on the shape sketch (a segment letter or a bend angle).
#[derive(Clone, Debug, PartialEq)]
pub struct SketchMark {
    pub text: String,
    pub at: [f64; 2],
}

/// GOST-style bar sketch in the shape plane, mm. +X is the first segment,
/// a positive angle turns left. `points` is the centreline with hooks and
/// with a fillet of the mandrel at each bend.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeSketch {
    pub points: Vec<[f64; 2]>,
    pub marks: Vec<SketchMark>,
    /// min x, min y, max x, max y, including the marks.
    pub bounds: [f64; 4],
}

impl Default for ShapeSketch {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            marks: Vec::new(),
            bounds: [0., 0., 1., 1.],
        }
    }
}

/// One row of the detail schedule («ведомость деталей»).
#[derive(Clone, Debug, PartialEq)]
pub struct DetailRow {
    pub mark: u32,
    pub shape_id: String,
    /// Short designation («ПР», «П», «Л»…). Not a GOST number.
    pub code: String,
    pub diameter: f64,
    pub grade: String,
    /// «ГОСТ 34028-2016» for a known class.
    pub standard: String,
    pub segments: Vec<SegmentLen>,
    pub hooks: [Hook; 2],
    /// Cut length of one bar, mm (arcs and hooks included).
    pub length_mm: f64,
    pub count: usize,
    pub unit_mass_kg: f64,
    pub total_mass_kg: f64,
    pub sets: Vec<u32>,
    pub hosts: Vec<ObjectId>,
    pub sketch: ShapeSketch,
}

impl DetailRow {
    /// Lengths of every portion with this letter, mm.
    pub fn lengths_of(&self, letter: &str) -> Vec<f64> {
        let mut out = Vec::new();
        for seg in self.segments.iter().filter(|s| s.letter == letter) {
            if out
                .last()
                .is_none_or(|v: &f64| (*v - seg.length_mm).abs() > 0.5)
            {
                out.push(seg.length_mm);
            }
        }
        out
    }
}

/// One row of the steel-consumption schedule.
#[derive(Clone, Debug, PartialEq)]
pub struct SteelRow {
    /// Host part name. «—» when the bar has no host, and on a class total.
    pub element: String,
    pub grade: String,
    pub diameter: f64,
    pub bars: usize,
    pub length_m: f64,
    pub mass_kg: f64,
}

/// Steel consumption by element and rolled up by class and diameter.
#[derive(Clone, Debug, PartialEq)]
pub struct SteelSchedule {
    pub by_element: Vec<SteelRow>,
    pub by_class: Vec<SteelRow>,
    pub total_bars: usize,
    pub total_length_m: f64,
    pub total_kg: f64,
}

/// Detail schedule, live from the model. Position 1 is the first distinct bar.
pub fn detail_schedule(project: &Project) -> Vec<DetailRow> {
    assemble(project, true)
}

/// «Ведомость расхода стали».
pub fn steel_schedule(project: &Project) -> SteelSchedule {
    let mut by_element: BTreeMap<(String, String, i64), Acc> = BTreeMap::new();
    let mut by_class: BTreeMap<(String, i64), Acc> = BTreeMap::new();
    for set in &project.concrete.rebars {
        let grade = grade_of(&set.grade);
        let d_key = (set.diameter * 1000.).round() as i64;
        let element = host_name(project, set.host);
        let radius = (mandrel_diameter(project, set) + set.diameter) / 2.;
        for bar in &set.bars {
            let length = cut_length(bar, radius);
            let mass = mass_per_m(set.diameter) * length / 1000.;
            bump(
                by_element.entry((element.clone(), grade.clone(), d_key)),
                set.diameter,
                &grade,
                length,
                mass,
            );
            bump(
                by_class.entry((grade.clone(), d_key)),
                set.diameter,
                &grade,
                length,
                mass,
            );
        }
    }
    let by_element = by_element
        .into_iter()
        .map(|((element, _, _), acc)| acc.row(element))
        .collect::<Vec<_>>();
    let by_class = by_class
        .into_values()
        .map(|acc| acc.row("—".into()))
        .collect::<Vec<_>>();
    let total_bars = by_class.iter().map(|r| r.bars).sum();
    let total_length_m = by_class.iter().map(|r| r.length_m).sum();
    let total_kg = by_class.iter().map(|r| r.mass_kg).sum();
    SteelSchedule {
        by_element,
        by_class,
        total_bars,
        total_length_m,
        total_kg,
    }
}

/// Position numbers listed on each zone mark, in order.
pub fn zone_marks(project: &Project) -> BTreeMap<u32, String> {
    labels_of(project, &assemble(project, false))
}

/// Write position numbers onto sets and zone marks.
pub fn write_positions(project: &mut Project) {
    let rows = assemble(project, false);
    let mut by_set: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for row in &rows {
        for n in &row.sets {
            by_set.entry(*n).or_default().insert(row.mark);
        }
    }
    for set in &mut project.concrete.rebars {
        let marks: Vec<u32> = by_set
            .get(&set.number)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        set.position = (marks.len() == 1).then_some(marks[0]);
    }
    let labels = labels_of(project, &rows);
    for zone in &mut project.concrete.zones {
        zone.mark = labels.get(&zone.number).cloned().unwrap_or_default();
    }
}

/// One leg of a bending shape, in bar order. `angle_deg` is the turn after
/// the leg (positive to the left); the last leg ends at 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BendLeg {
    /// Length between bend tangents, or the straight part of a hook, mm.
    pub length_mm: i64,
    pub angle_deg: i32,
}

/// Legs of one schedule position: a start hook, the segments A, B, C…, then
/// an end hook. This is the bending shape shared by BVBS and IFC.
pub fn bend_legs(row: &DetailRow) -> Vec<BendLeg> {
    let mut legs = Vec::new();
    if let Some(deg) = hook_degrees(row.hooks[0].kind) {
        legs.push(BendLeg {
            length_mm: whole_mm(row.hooks[0].extension(row.diameter)),
            angle_deg: deg,
        });
    }
    let n = row.segments.len();
    for (i, seg) in row.segments.iter().enumerate() {
        let mut angle = seg.bend_deg.round() as i32;
        if i + 1 == n {
            if let Some(deg) = hook_degrees(row.hooks[1].kind) {
                if angle == 0 {
                    angle = deg;
                }
            } else {
                angle = 0;
            }
        }
        legs.push(BendLeg {
            length_mm: whole_mm(seg.length_mm),
            angle_deg: angle,
        });
    }
    if hook_degrees(row.hooks[1].kind).is_some() {
        legs.push(BendLeg {
            length_mm: whole_mm(row.hooks[1].extension(row.diameter)),
            angle_deg: 0,
        });
    }
    if legs.is_empty() {
        legs.push(BendLeg {
            length_mm: whole_mm(row.length_mm),
            angle_deg: 0,
        });
    }
    legs
}

fn hook_degrees(kind: HookKind) -> Option<i32> {
    match kind {
        HookKind::None => None,
        HookKind::Deg90 => Some(90),
        HookKind::Deg135 => Some(135),
        HookKind::Deg180 => Some(180),
    }
}

fn whole_mm(value: f64) -> i64 {
    let n = value.round() as i64;
    if n < 1 { 1 } else { n }
}

/// Where one bar sits in the detail schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarPlace {
    pub set_number: u32,
    pub bar_index: usize,
    pub mark: u32,
}

/// Position of every bar. The mark matches [`detail_schedule`].
pub fn bar_places(project: &Project) -> Vec<BarPlace> {
    let items = items_of(project);
    let mut out = Vec::new();
    for (n, ix) in grouped(&items).into_iter().enumerate() {
        let mark = n as u32 + 1;
        for i in ix {
            out.push(BarPlace {
                set_number: items[i].set_number,
                bar_index: items[i].bar_index,
                mark,
            });
        }
    }
    out
}

/// Letters used by the schedule, in the order they first appear.
pub fn column_letters(rows: &[DetailRow]) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows {
        for seg in &row.segments {
            if !out.contains(&seg.letter) {
                out.push(seg.letter.clone());
            }
        }
    }
    out
}

/// Sketch of a shape type. Lengths come from `params` when set.
pub fn shape_sketch(
    shape: &BarShapeType,
    params: &BTreeMap<String, f64>,
    diameter: f64,
    grade: &str,
) -> Result<ShapeSketch, String> {
    let mandrel = mandrel_of(shape, grade, diameter)?;
    let radius = (mandrel + diameter) / 2.;
    let line = shape::centerline(shape, params)?;
    let drawn = fillet(
        &shape::plane_points(shape, params, diameter, radius)?,
        radius,
    );
    let mut marks = Vec::new();
    let offset = (diameter * 4.).clamp(36., 80.);
    for (i, seg) in shape.segments.iter().enumerate() {
        if i + 1 >= line.len() {
            break;
        }
        let (a, b) = (line[i], line[i + 1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0].hypot(d[1]);
        if len < 1. {
            continue;
        }
        let dir = [d[0] / len, d[1] / len];
        let left = [-dir[1], dir[0]];
        let letter = if seg.letter.is_empty() {
            letter_of(i)
        } else {
            seg.letter.clone()
        };
        marks.push(SketchMark {
            text: letter,
            at: [
                (a[0] + b[0]) / 2. + left[0] * offset,
                (a[1] + b[1]) / 2. + left[1] * offset,
            ],
        });
        if seg.bend_deg.abs() > 1. && i + 2 < line.len() {
            let c = line[i + 2];
            let v = [c[0] - b[0], c[1] - b[1]];
            let l = v[0].hypot(v[1]);
            let dout = if l < 1. { dir } else { [v[0] / l, v[1] / l] };
            let bis = unit2([dir[0] + dout[0], dir[1] + dout[1]]).unwrap_or(left);
            marks.push(SketchMark {
                text: angle_text(seg.bend_deg),
                at: [b[0] + bis[0] * offset, b[1] + bis[1] * offset],
            });
        }
    }
    if line.len() >= 2 {
        let dir = unit2([line[1][0] - line[0][0], line[1][1] - line[0][1]]).unwrap_or([1., 0.]);
        if let Some(text) = hook_angle(shape.hooks[0].kind) {
            marks.push(SketchMark {
                text,
                at: [line[0][0] - dir[0] * offset, line[0][1] - dir[1] * offset],
            });
        }
        let n = line.len();
        let dir = unit2([
            line[n - 1][0] - line[n - 2][0],
            line[n - 1][1] - line[n - 2][1],
        ])
        .unwrap_or([1., 0.]);
        if let Some(text) = hook_angle(shape.hooks[1].kind) {
            marks.push(SketchMark {
                text,
                at: [
                    line[n - 1][0] + dir[0] * offset,
                    line[n - 1][1] + dir[1] * offset,
                ],
            });
        }
    }
    Ok(finish_sketch(drawn, marks))
}

fn finish_sketch(points: Vec<[f64; 2]>, marks: Vec<SketchMark>) -> ShapeSketch {
    let mut min = [f64::MAX; 2];
    let mut max = [f64::MIN; 2];
    for p in points.iter().copied().chain(marks.iter().map(|m| m.at)) {
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    }
    let bounds = if min[0] <= max[0] {
        [min[0], min[1], max[0], max[1]]
    } else {
        [0., 0., 1., 1.]
    };
    ShapeSketch {
        points,
        marks,
        bounds,
    }
}

fn angle_text(deg: f64) -> String {
    let r = deg.round();
    if (deg - r).abs() < 1e-6 {
        format!("{r:.0}°")
    } else {
        format!("{deg:.1}°")
    }
}

fn hook_angle(kind: HookKind) -> Option<String> {
    match kind {
        HookKind::None => None,
        HookKind::Deg90 => Some("90°".into()),
        HookKind::Deg135 => Some("135°".into()),
        HookKind::Deg180 => Some("180°".into()),
    }
}

struct Acc {
    grade: String,
    diameter: f64,
    bars: usize,
    length_mm: f64,
    mass_kg: f64,
}

fn bump(
    entry: std::collections::btree_map::Entry<'_, impl Ord, Acc>,
    diameter: f64,
    grade: &str,
    length_mm: f64,
    mass: f64,
) {
    let acc = entry.or_insert_with(|| Acc {
        grade: grade.to_string(),
        diameter,
        bars: 0,
        length_mm: 0.,
        mass_kg: 0.,
    });
    acc.bars += 1;
    acc.length_mm += length_mm;
    acc.mass_kg += mass;
}

impl Acc {
    fn row(self, element: String) -> SteelRow {
        SteelRow {
            element,
            grade: self.grade,
            diameter: self.diameter,
            bars: self.bars,
            length_m: self.length_mm / 1000.,
            mass_kg: self.mass_kg,
        }
    }
}

struct BarItem {
    set_number: u32,
    bar_index: usize,
    key: Key,
    shape_id: String,
    code: String,
    diameter: f64,
    grade: String,
    standard: String,
    segments: Vec<SegmentLen>,
    hooks: [Hook; 2],
    mandrel: f64,
    length_mm: f64,
    host: Option<ObjectId>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    shape_id: String,
    diameter_um: i64,
    grade: String,
    segments: Vec<(String, i64, i32)>,
    hooks: [(u8, i64); 2],
    mandrel_mm: i64,
}

fn assemble(project: &Project, sketches: bool) -> Vec<DetailRow> {
    let items = items_of(project);
    grouped(&items)
        .into_iter()
        .enumerate()
        .map(|(n, ix)| row_of(&items, ix, n as u32 + 1, sketches))
        .collect()
}

fn items_of(project: &Project) -> Vec<BarItem> {
    let mut items = Vec::new();
    for set in &project.concrete.rebars {
        let mandrel = mandrel_diameter(project, set);
        let radius = (mandrel + set.diameter) / 2.;
        for (bar_index, bar) in set.bars.iter().enumerate() {
            if bar.len() < 2 {
                continue;
            }
            let measured = measure(project, set, bar);
            items.push(BarItem {
                set_number: set.number,
                bar_index,
                key: key_of(set, &measured, mandrel),
                shape_id: measured.shape_id,
                code: measured.code,
                diameter: set.diameter,
                grade: grade_of(&set.grade),
                standard: standard_of(&set.grade),
                segments: measured.segments,
                hooks: measured.hooks,
                mandrel,
                length_mm: cut_length(bar, radius),
                host: set.host,
            });
        }
    }
    items
}

fn grouped(items: &[BarItem]) -> Vec<Vec<usize>> {
    let mut groups: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        groups.entry(item.key.clone()).or_default().push(i);
    }
    let mut order: Vec<Vec<usize>> = groups.into_values().collect();
    order.sort_by(|a, b| {
        let first = |ix: &[usize]| {
            ix.iter()
                .map(|i| (items[*i].set_number, items[*i].bar_index))
                .min()
                .unwrap_or((u32::MAX, usize::MAX))
        };
        first(a)
            .cmp(&first(b))
            .then_with(|| items[a[0]].key.cmp(&items[b[0]].key))
    });
    order
}

fn row_of(items: &[BarItem], ix: Vec<usize>, mark: u32, sketches: bool) -> DetailRow {
    let first = &items[ix[0]];
    let mut sets = Vec::new();
    let mut hosts = Vec::new();
    let mut length = 0.;
    for i in &ix {
        let item = &items[*i];
        length += item.length_mm;
        if !sets.contains(&item.set_number) {
            sets.push(item.set_number);
        }
        if let Some(host) = item.host
            && !hosts.contains(&host)
        {
            hosts.push(host);
        }
    }
    sets.sort_unstable();
    let count = ix.len();
    let mass = mass_per_m(first.diameter) * length / 1000.;
    DetailRow {
        mark,
        shape_id: first.shape_id.clone(),
        code: first.code.clone(),
        diameter: first.diameter,
        grade: first.grade.clone(),
        standard: first.standard.clone(),
        segments: first.segments.clone(),
        hooks: first.hooks,
        length_mm: length / count as f64,
        count,
        unit_mass_kg: mass / count as f64,
        total_mass_kg: mass,
        sets,
        hosts,
        sketch: if sketches {
            sketch_of(first)
        } else {
            ShapeSketch::default()
        },
    }
}

fn sketch_of(item: &BarItem) -> ShapeSketch {
    let shape = BarShapeType {
        id: item.shape_id.clone(),
        name: String::new(),
        code: item.code.clone(),
        segments: item
            .segments
            .iter()
            .map(|s| ShapeSegment {
                letter: s.letter.clone(),
                length: s.length_mm.max(1.),
                bend_deg: s.bend_deg,
            })
            .collect(),
        hooks: item.hooks,
        mandrel: Some(item.mandrel),
    };
    if shape.segments.is_empty() {
        return ShapeSketch::default();
    }
    shape_sketch(&shape, &BTreeMap::new(), item.diameter, &item.grade).unwrap_or_default()
}

struct Measured {
    shape_id: String,
    code: String,
    segments: Vec<SegmentLen>,
    hooks: [Hook; 2],
}

fn measure(project: &Project, set: &RebarSet, bar: &[Point3]) -> Measured {
    let resolved = set
        .shape_id
        .as_deref()
        .and_then(|id| resolve_shape(project, id));
    let (pts, hooks) = centerline_of(bar, resolved.as_ref().map(|s| s.segments.len()), set.hooks);
    let matched = resolved
        .as_ref()
        .is_some_and(|s| pts.len() == s.segments.len() + 1);
    let turns = signed_turns(&pts);
    let mut segments = Vec::new();
    let n = pts.len().saturating_sub(1);
    for i in 0..n {
        let length = dist3(&pts[i], &pts[i + 1]).round();
        let (letter, bend) = if matched {
            let seg = &resolved.as_ref().unwrap().segments[i];
            (seg.letter.clone(), seg.bend_deg)
        } else {
            let bend = if i + 1 < n {
                turns.get(i).copied().unwrap_or(0.).round()
            } else {
                0.
            };
            (letter_of(i), bend)
        };
        segments.push(SegmentLen {
            letter,
            length_mm: length,
            bend_deg: bend,
        });
    }
    if segments.is_empty() {
        segments.push(SegmentLen {
            letter: "A".into(),
            length_mm: 0.,
            bend_deg: 0.,
        });
    }
    let derived = derived_code(&segments, &hooks);
    let (shape_id, code) = if let Some(shape) = &resolved {
        (shape.id.clone(), shape.code.clone())
    } else if let Some(id) = catalogue_id(&derived) {
        (id.into(), derived)
    } else {
        (String::new(), derived)
    };
    Measured {
        shape_id,
        code,
        segments,
        hooks,
    }
}

fn key_of(set: &RebarSet, measured: &Measured, mandrel: f64) -> Key {
    Key {
        shape_id: measured.shape_id.clone(),
        diameter_um: (set.diameter * 1000.).round() as i64,
        grade: grade_of(&set.grade),
        segments: measured
            .segments
            .iter()
            .map(|s| {
                (
                    s.letter.clone(),
                    s.length_mm.round() as i64,
                    s.bend_deg.round() as i32,
                )
            })
            .collect(),
        hooks: [
            hook_key(measured.hooks[0], set.diameter),
            hook_key(measured.hooks[1], set.diameter),
        ],
        mandrel_mm: mandrel.round() as i64,
    }
}

fn hook_key(hook: Hook, diameter: f64) -> (u8, i64) {
    let kind = match hook.kind {
        HookKind::None => 0,
        HookKind::Deg90 => 1,
        HookKind::Deg135 => 2,
        HookKind::Deg180 => 3,
    };
    let ext = if hook.is_none() {
        0
    } else {
        hook.extension(diameter).round() as i64
    };
    (kind, ext)
}

fn grade_of(grade: &str) -> String {
    rebar_class(grade)
        .map(|c| c.name.to_string())
        .unwrap_or_else(|| grade.trim().to_string())
}

fn standard_of(grade: &str) -> String {
    rebar_class(grade)
        .map(|c| c.standard.to_string())
        .unwrap_or_else(|| "ГОСТ 34028-2016".into())
}

fn host_name(project: &Project, host: Option<ObjectId>) -> String {
    host.and_then(|id| project.elements.get(&id))
        .map(|e| e.name.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "—".into())
}

/// Mandrel diameter of a set, mm: the set, else the shape type, else СП 63.
pub fn mandrel_diameter(project: &Project, set: &RebarSet) -> f64 {
    if let Some(d) = set.bend_diameter {
        return d;
    }
    if let Some(id) = set.shape_id.as_deref()
        && let Some(shape) = resolve_shape(project, id)
        && let Ok(m) = mandrel_of(&shape, &set.grade, set.diameter)
    {
        return m;
    }
    min_bend_diameter(rebar_class(&set.grade), set.diameter)
}

fn catalogue_id(code: &str) -> Option<&'static str> {
    Some(match code {
        "ПР" => "gost.straight",
        "Г" => "gost.l",
        "П" => "gost.u",
        "Х" => "gost.stirrup",
        "Л" => "gost.frog",
        "К" => "gost.hooks",
        _ => return None,
    })
}

fn derived_code(segments: &[SegmentLen], hooks: &[Hook; 2]) -> String {
    let bends: Vec<i32> = segments.iter().map(|s| s.bend_deg.round() as i32).collect();
    let straight = bends.iter().all(|b| b.abs() < 1);
    if segments.len() == 1 && straight && hooks.iter().all(Hook::is_none) {
        return "ПР".into();
    }
    if segments.len() == 1 && straight && hooks.iter().all(|h| h.kind == HookKind::Deg180) {
        return "К".into();
    }
    if segments.len() == 2 && bends.first().is_some_and(|b| b.abs() == 90) {
        return "Г".into();
    }
    if segments.len() == 3 && bends.len() >= 2 && bends[0].abs() == 90 && bends[1].abs() == 90 {
        return "П".into();
    }
    if segments.len() == 4 && hooks.iter().all(|h| h.kind == HookKind::Deg135) {
        return "Х".into();
    }
    if bends == [90, -90, -90, 90, 0] {
        return "Л".into();
    }
    "С".into()
}

fn letter_of(i: usize) -> String {
    if i < 26 {
        char::from(b'A' + i as u8).to_string()
    } else {
        format!("A{}", i + 1)
    }
}

fn hook_extra(kind: HookKind) -> usize {
    match kind {
        HookKind::None => 0,
        HookKind::Deg90 | HookKind::Deg135 => 1,
        HookKind::Deg180 => 2,
    }
}

fn centerline_of(
    bar: &[Point3],
    n_seg: Option<usize>,
    hooks: [Hook; 2],
) -> (Vec<Point3>, [Hook; 2]) {
    let none = Hook::default();
    if let Some(n) = n_seg {
        let need = n + 1;
        for pair in [
            [hooks[0], hooks[1]],
            [hooks[0], none],
            [none, hooks[1]],
            [none, none],
        ] {
            let e0 = hook_extra(pair[0].kind);
            let e1 = hook_extra(pair[1].kind);
            if e0 + e1 < bar.len() && bar.len() == need + e0 + e1 {
                return (bar[e0..bar.len() - e1].to_vec(), pair);
            }
        }
    }
    if bar.len() == 2 {
        return (bar.to_vec(), [none, none]);
    }
    let e0 = hook_extra(hooks[0].kind);
    let e1 = hook_extra(hooks[1].kind);
    if e0 + e1 > 0 && bar.len() > 2 + e0 + e1 {
        return (bar[e0..bar.len() - e1].to_vec(), hooks);
    }
    (bar.to_vec(), [none, none])
}

fn signed_turns(pts: &[Point3]) -> Vec<f64> {
    if pts.len() < 3 {
        return Vec::new();
    }
    let mut normal = None;
    for w in pts.windows(3) {
        let c = cross3(sub3(w[1], w[0]), sub3(w[2], w[1]));
        if len3(c) > 1e-4 {
            normal = norm3(c);
            break;
        }
    }
    let Some(n) = normal else {
        return vec![0.; pts.len() - 2];
    };
    pts.windows(3)
        .map(|w| {
            let (Some(u), Some(v)) = (norm3(sub3(w[1], w[0])), norm3(sub3(w[2], w[1]))) else {
                return 0.;
            };
            let sign = dot3(cross3(u, v), n).signum();
            let sign = if sign == 0. { 1. } else { sign };
            dot3(u, v).clamp(-1., 1.).acos().to_degrees() * sign
        })
        .collect()
}

fn cut_length(bar: &[Point3], radius: f64) -> f64 {
    let mut length = bar.windows(2).map(|w| dist3(&w[0], &w[1])).sum::<f64>();
    for w in bar.windows(3) {
        let (Some(u), Some(v)) = (norm3(sub3(w[1], w[0])), norm3(sub3(w[2], w[1]))) else {
            continue;
        };
        let theta = dot3(u, v).clamp(-1., 1.).acos().min(179f64.to_radians());
        if theta > 1e-6 {
            length -= 2. * radius * (theta / 2.).tan() - radius * theta;
        }
    }
    length
}

fn fillet(pts: &[[f64; 2]], radius: f64) -> Vec<[f64; 2]> {
    if pts.len() < 3 || radius < 1. {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let (a, b, c) = (pts[i - 1], pts[i], pts[i + 1]);
        let vin = [b[0] - a[0], b[1] - a[1]];
        let vout = [c[0] - b[0], c[1] - b[1]];
        let (lin, lout) = (vin[0].hypot(vin[1]), vout[0].hypot(vout[1]));
        let (Some(din), Some(dout)) = (unit2(vin), unit2(vout)) else {
            out.push(b);
            continue;
        };
        let cross = din[0] * dout[1] - din[1] * dout[0];
        let dot = (din[0] * dout[0] + din[1] * dout[1]).clamp(-1., 1.);
        let theta = dot.acos();
        if !(1f64.to_radians()..170f64.to_radians()).contains(&theta) {
            out.push(b);
            continue;
        }
        let setback = (radius * (theta / 2.).tan()).min(0.45 * lin.min(lout));
        if setback < 0.5 {
            out.push(b);
            continue;
        }
        let r = setback / (theta / 2.).tan();
        let p1 = [b[0] - din[0] * setback, b[1] - din[1] * setback];
        let p2 = [b[0] + dout[0] * setback, b[1] + dout[1] * setback];
        let n = if cross > 0. {
            [-din[1], din[0]]
        } else {
            [din[1], -din[0]]
        };
        let center = [p1[0] + n[0] * r, p1[1] + n[1] * r];
        let a0 = (p1[1] - center[1]).atan2(p1[0] - center[0]);
        let a1 = (p2[1] - center[1]).atan2(p2[0] - center[0]);
        let mut sweep = a1 - a0;
        if cross > 0. && sweep < 0. {
            sweep += std::f64::consts::TAU;
        }
        if cross < 0. && sweep > 0. {
            sweep -= std::f64::consts::TAU;
        }
        let steps = ((sweep.abs().to_degrees() / 18.).ceil() as usize).clamp(2, 8);
        out.push(p1);
        for s in 1..steps {
            let t = a0 + sweep * s as f64 / steps as f64;
            out.push([center[0] + r * t.cos(), center[1] + r * t.sin()]);
        }
        out.push(p2);
    }
    out.push(*pts.last().unwrap());
    out
}

fn labels_of(project: &Project, rows: &[DetailRow]) -> BTreeMap<u32, String> {
    let set_zone: BTreeMap<u32, u32> = project
        .concrete
        .rebars
        .iter()
        .filter_map(|s| s.zone.map(|z| (s.number, z)))
        .collect();
    let mut marks: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for row in rows {
        let mut zones = BTreeSet::new();
        for n in &row.sets {
            if let Some(z) = set_zone.get(n) {
                zones.insert(*z);
            }
        }
        for z in zones {
            marks.entry(z).or_default().insert(row.mark);
        }
    }
    marks
        .into_iter()
        .map(|(z, ms)| {
            (
                z,
                ms.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })
        .collect()
}

fn sub3(a: Point3, b: Point3) -> [f64; 3] {
    [a.0[0] - b.0[0], a.0[1] - b.0[1], a.0[2] - b.0[2]]
}
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}
fn norm3(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = len3(a);
    (l > 1e-9).then_some([a[0] / l, a[1] / l, a[2] / l])
}
fn dist3(a: &Point3, b: &Point3) -> f64 {
    len3(sub3(*b, *a))
}
fn unit2(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = v[0].hypot(v[1]);
    (l > 1e-9).then_some([v[0] / l, v[1] / l])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concrete::{MeshLayer, ShapePlace, SlabRebarType};
    use crate::rebar::shape;
    use crate::{DetailKind, Element, ElementShape, Frame, LengthMm, Material, Point3, RebarKind};

    fn place() -> ShapePlace {
        ShapePlace {
            origin: Point3([0.; 3]),
            x: [1., 0., 0.],
            y: [0., 1., 0.],
            local_to_host: false,
        }
    }

    fn bar_of(id: &str, params: &[(&str, f64)]) -> Vec<Point3> {
        let shape = shape::builtin_shapes()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap();
        let map = params.iter().map(|(k, v)| ((*k).into(), *v)).collect();
        shape::instantiate(&shape, &map, 16., "А500С", &place())
            .unwrap()
            .bar
    }

    fn push(
        project: &mut Project,
        kind: RebarKind,
        diameter: f64,
        grade: &str,
        bars: Vec<Vec<Point3>>,
        shape_id: Option<&str>,
        hooks: [Hook; 2],
    ) {
        let mut set = RebarSet::basic(kind, diameter, grade);
        set.number = project
            .concrete
            .rebars
            .iter()
            .map(|r| r.number)
            .max()
            .unwrap_or(0)
            + 1;
        set.bars = bars;
        set.shape_id = shape_id.map(str::to_string);
        set.hooks = hooks;
        project.concrete.rebars.push(set);
    }

    #[test]
    fn identical_bars_share_a_position_hooks_and_lengths_do_not() {
        let mut project = Project::default();
        let straight = bar_of("gost.straight", &[("A", 1000.)]);
        push(
            &mut project,
            RebarKind::Single,
            16.,
            "А500С",
            vec![straight.clone(), straight],
            Some("gost.straight"),
            [Hook::default(); 2],
        );
        push(
            &mut project,
            RebarKind::Single,
            16.,
            "A500C",
            vec![bar_of("gost.straight", &[("A", 1000.)])],
            Some("gost.straight"),
            [Hook::default(); 2],
        );
        let rows = detail_schedule(&project);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 3);
        assert_eq!(rows[0].code, "ПР");
        assert_eq!(rows[0].grade, "А500С");
        assert_eq!(rows[0].standard, "ГОСТ 34028-2016");
        assert!((rows[0].lengths_of("A")[0] - 1000.).abs() < 1e-6);
        assert!(
            (rows[0].unit_mass_kg - 1.578).abs() < 1e-9,
            "{}",
            rows[0].unit_mass_kg
        );
        assert!((rows[0].total_mass_kg - 1.578 * 3.).abs() < 1e-9);
        push(
            &mut project,
            RebarKind::Single,
            16.,
            "А500С",
            vec![bar_of("gost.straight", &[("A", 800.)])],
            Some("gost.straight"),
            [Hook::default(); 2],
        );
        push(
            &mut project,
            RebarKind::Single,
            16.,
            "А500С",
            vec![bar_of("gost.hooks", &[("A", 1000.)])],
            Some("gost.hooks"),
            [Hook::new(HookKind::Deg180); 2],
        );
        let rows = detail_schedule(&project);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|r| r.code == "К"));
        assert!(rows.iter().any(|r| r.lengths_of("A") == vec![800.]));
        let steel = steel_schedule(&project);
        assert_eq!(steel.total_bars, 5);
        assert!((steel.total_kg - rows.iter().map(|r| r.total_mass_kg).sum::<f64>()).abs() < 1e-6);
    }

    #[test]
    fn an_l_sketch_shows_the_bend_the_letters_and_the_mandrel() {
        let shape = shape::builtin_shapes()
            .into_iter()
            .find(|s| s.id == "gost.l")
            .unwrap();
        let sketch = shape_sketch(&shape, &BTreeMap::new(), 12., "А500С").unwrap();
        let texts: Vec<_> = sketch.marks.iter().map(|m| m.text.as_str()).collect();
        assert!(
            texts.contains(&"A") && texts.contains(&"B") && texts.contains(&"90°"),
            "{texts:?}"
        );
        assert!(sketch.points.len() > 4, "{}", sketch.points.len());
        let corner = [1000., 0.];
        assert!(
            sketch
                .points
                .iter()
                .all(|p| (p[0] - corner[0]).hypot(p[1] - corner[1]) > 1.)
        );
        let end = *sketch.points.last().unwrap();
        assert!(
            (end[0] - 1000.).abs() < 1. && (end[1] - 500.).abs() < 1.,
            "{end:?}"
        );
    }

    #[test]
    fn a_hook_sketch_leaves_the_straight_axis() {
        let shape = shape::builtin_shapes()
            .into_iter()
            .find(|s| s.id == "gost.hooks")
            .unwrap();
        let sketch = shape_sketch(&shape, &BTreeMap::new(), 12., "А500С").unwrap();
        assert!(sketch.marks.iter().any(|m| m.text == "180°"));
        assert!(sketch.points.iter().any(|p| p[1].abs() > 10.));
    }

    fn slab(width: f64, depth: f64, thick: f64) -> (Project, ObjectId) {
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

    fn lay(p: &mut Project, host: ObjectId) {
        let mut ty = SlabRebarType::new("s", "Плита с сетками");
        let layer = MeshLayer::new(10., 200., 0.);
        ty.bottom = Some(layer.clone());
        ty.top = Some(layer);
        ty.edge_diameter = 12.;
        ty.edge_grade = "А500С".into();
        ty.edge_spacing = 200.;
        ty.frog_diameter = 8.;
        ty.frog_grade = "А500С".into();
        ty.frog_spacing = 600.;
        p.concrete.slab_types.retain(|t| t.id != ty.id);
        p.concrete.slab_types.push(ty);
        p.concrete = super::super::install_slab(p, host, "s").unwrap();
    }

    #[test]
    fn slab_meshes_u_bars_and_frogs_fill_both_schedules() {
        let (mut p, host) = slab(6000., 4000., 200.);
        lay(&mut p, host);
        let rows = detail_schedule(&p);
        let code = |c: &str| rows.iter().filter(|r| r.code == c).count();
        assert!(
            code("ПР") >= 2,
            "{:?}",
            rows.iter().map(|r| &r.code).collect::<Vec<_>>()
        );
        assert!(code("П") >= 2);
        assert!(code("Л") >= 1);
        for row in rows.iter().filter(|r| r.code == "П") {
            let tangent: f64 = row.segments.iter().map(|s| s.length_mm).sum();
            assert!(row.length_mm < tangent - 1., "{} {tangent}", row.length_mm);
            assert!(row.sketch.marks.iter().any(|m| m.text == "90°"));
            assert!(row.lengths_of("A")[0] > 200.);
        }
        let steel = steel_schedule(&p);
        assert!(steel.by_element.iter().all(|r| r.element == "Перекрытие"));
        assert!((steel.total_kg - rows.iter().map(|r| r.total_mass_kg).sum::<f64>()).abs() < 1e-6);
        let b_before: Vec<f64> = rows
            .iter()
            .filter(|r| r.code == "П" || r.code == "Л")
            .flat_map(|r| r.lengths_of("B"))
            .collect();
        let mark = p.concrete.zones[0].mark.clone();
        let mut thick = p.clone();
        let previous = thick.clone();
        if let ElementShape::Plate { thickness, .. } =
            &mut thick.elements.get_mut(&host).unwrap().shape
        {
            *thickness = LengthMm::new(300.).unwrap();
        }
        crate::rebar::rebuild_rebars(&mut thick, &previous).unwrap();
        let after = detail_schedule(&thick);
        let b_after: Vec<f64> = after
            .iter()
            .filter(|r| r.code == "П" || r.code == "Л")
            .flat_map(|r| r.lengths_of("B"))
            .collect();
        assert_ne!(b_before, b_after, "{b_before:?} {b_after:?}");
        assert_eq!(after.len(), rows.len());
        assert_eq!(thick.concrete.zones[0].mark, mark);
        let id2 = ObjectId(11);
        p.elements.insert(
            id2,
            Element {
                id: id2,
                name: "Перекрытие 2".into(),
                ..p.elements[&host].clone()
            },
        );
        p.concrete = super::super::install_slab(&p, id2, "s").unwrap();
        let again = detail_schedule(&p);
        assert_eq!(again.len(), rows.len());
        assert_eq!(p.concrete.zones[0].mark, p.concrete.zones[1].mark);
        assert!(
            again
                .iter()
                .all(|r| { r.count == rows.iter().find(|a| a.mark == r.mark).unwrap().count * 2 })
        );
        let (mut q, host) = slab(6000., 4000., 200.);
        lay(&mut q, host);
        let before = detail_schedule(&q)
            .iter()
            .filter(|r| r.code == "ПР")
            .count();
        q.details.insert(
            ObjectId(30),
            crate::Detail {
                id: ObjectId(30),
                kind: DetailKind::PrismCut {
                    part: host,
                    frame: Frame {
                        origin: Point3([2000., 1500., -20.]),
                        ..Frame::default()
                    },
                    contour: vec![[0., 0.], [800., 0.], [800., 800.], [0., 800.]],
                    length: 240.,
                },
            },
        );
        q.concrete.slab_types[0].cut_openings = true;
        q.concrete = super::super::install_slab(&q, host, "s").unwrap();
        let cut = detail_schedule(&q)
            .iter()
            .filter(|r| r.code == "ПР")
            .count();
        assert!(cut > before, "{before} → {cut}");
    }

    #[test]
    fn a_shape_edit_updates_the_position() {
        let mut project = Project::default();
        let mut shape = shape::builtin_shapes()
            .into_iter()
            .find(|s| s.id == "gost.l")
            .unwrap();
        shape.id = "user.l".into();
        project.concrete.shape_types.push(shape.clone());
        let bar = shape::instantiate(&shape, &BTreeMap::new(), 12., "А500С", &place())
            .unwrap()
            .bar;
        let mut set = RebarSet::basic(RebarKind::Shape, 12., "А500С");
        set.number = 1;
        set.shape_id = Some("user.l".into());
        set.place = Some(place());
        set.bars = vec![bar];
        project.concrete.rebars.push(set);
        assert!((detail_schedule(&project)[0].lengths_of("A")[0] - 1000.).abs() < 1e-6);
        let previous = project.clone();
        project.concrete.shape_types[0].segments[0].length = 700.;
        crate::rebar::rebuild_rebars(&mut project, &previous).unwrap();
        let after = detail_schedule(&project);
        assert!((after[0].lengths_of("A")[0] - 700.).abs() < 1e-6);
        assert!((after[0].sketch.points.last().unwrap()[0] - 700.).abs() < 1.);
    }
}
