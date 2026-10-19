//! Tekla-style selection and direct editing: click modes (replace, Shift add,
//! Ctrl toggle), area selection (left→right window = fully inside, right→left
//! crossing = touching), selection switches, handle dragging, and move/copy/
//! mirror/rotate by picked points.
use crate::tools::Edit;
use open_bim_commands::{Command, DeletePolicy, Document};
use open_bim_model::{DetailKind, ElementShape, Frame, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{Handle, HandleKind, Projector};
use std::collections::BTreeSet;

/// Selection switches (Tekla «Selecting» toolbar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub parts: bool,
    /// Clicking a part selects its whole assembly.
    pub assemblies: bool,
    pub bolts: bool,
    pub welds: bool,
    pub grids: bool,
    pub levels: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            parts: true,
            assemblies: false,
            bolts: true,
            welds: true,
            grids: true,
            levels: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Replace,
    /// Shift: add to the selection.
    Add,
    /// Ctrl: toggle.
    Toggle,
}

/// Apply a click result to the selection.
pub fn apply(selected: &mut BTreeSet<ObjectId>, ids: &BTreeSet<ObjectId>, mode: Click) {
    match mode {
        Click::Replace => *selected = ids.clone(),
        Click::Add => selected.extend(ids.iter().copied()),
        Click::Toggle => {
            for id in ids {
                if !selected.remove(id) {
                    selected.insert(*id);
                }
            }
        }
    }
}

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

/// World corners of a part's solid, ordered contour × [start, end]:
/// `[c0 start, c0 end, c1 start, c1 end, …]`.
pub fn corners(project: &Project, id: ObjectId) -> Result<Vec<DVec3>, String> {
    let e = project
        .elements
        .get(&id)
        .ok_or_else(|| format!("Нет детали #{}", id.0))?;
    let (contour, length) = match &e.shape {
        ElementShape::Plate {
            contour, thickness, ..
        } => (contour.clone(), thickness.value()),
        ElementShape::Member {
            profile, length, ..
        } => {
            let shape = &project.profiles.get(profile).ok_or("Нет профиля")?.shape;
            let contour = shape.solid_contour().unwrap_or_else(|_| {
                // Hollow sections: the bounding rectangle is enough for selection.
                let (w, h) = match *shape {
                    open_bim_model::ProfileShape::CircularHollow { diameter, .. } => {
                        (diameter, diameter)
                    }
                    open_bim_model::ProfileShape::RectangularHollow { height, width, .. } => {
                        (width, height)
                    }
                    _ => (100., 100.),
                };
                vec![
                    [-w / 2., -h / 2.],
                    [w / 2., -h / 2.],
                    [w / 2., h / 2.],
                    [-w / 2., h / 2.],
                ]
            });
            (contour, length.value())
        }
    };
    let frame = e.shape.frame();
    Ok(contour
        .iter()
        .flat_map(|c| [0., length].map(|z| v(frame.world(Point3([c[0], c[1], z])))))
        .collect())
}

/// Screen segments of an object (for area selection).
fn segments(project: &Project, id: ObjectId) -> Vec<(DVec3, DVec3)> {
    if let Some(axis) = project.axes.get(&id) {
        return vec![(v(axis.start), v(axis.end))];
    }
    if let Some(c) = project.construction.get(&id) {
        return c
            .segments()
            .into_iter()
            .map(|(a, b)| (DVec3::from_array(a), DVec3::from_array(b)))
            .collect();
    }
    if let Some(d) = project.dimensions.get(&id) {
        return match (project.dim_point(&d.a), project.dim_point(&d.b)) {
            (Some(a), Some(b)) => vec![(v(a), v(b))],
            _ => Vec::new(),
        };
    }
    if let Some(level) = project.levels.get(&id) {
        return open_bim_renderer::overlay::level_line(project, level)
            .into_iter()
            .collect();
    }
    if let Some(detail) = project.details.get(&id) {
        return match &detail.kind {
            DetailKind::BoltGroup {
                frame,
                steps_x,
                steps_y,
                ..
            } => {
                let prefix = |s: &[f64]| {
                    let mut out = vec![0.];
                    for x in s {
                        out.push(out.last().unwrap() + x);
                    }
                    out
                };
                let mut pts = Vec::new();
                for x in prefix(steps_x) {
                    for y in prefix(steps_y) {
                        let p = v(frame.world(Point3([x, y, 0.])));
                        pts.push((p, p));
                    }
                }
                pts
            }
            DetailKind::Weld { path, .. } => path.windows(2).map(|w| (v(w[0]), v(w[1]))).collect(),
            _ => Vec::new(),
        };
    }
    let Ok(c) = corners(project, id) else {
        return Vec::new();
    };
    let n = c.len() / 2;
    let mut out = Vec::new();
    for k in 0..n {
        let next = (k + 1) % n;
        out.push((c[2 * k], c[2 * k + 1]));
        out.push((c[2 * k], c[2 * next]));
        out.push((c[2 * k + 1], c[2 * next + 1]));
    }
    out
}

fn inside(p: [f64; 2], min: [f64; 2], max: [f64; 2]) -> bool {
    p[0] >= min[0] && p[0] <= max[0] && p[1] >= min[1] && p[1] <= max[1]
}

/// Liang–Barsky: does the 2D segment touch the rectangle?
fn touches(a: [f64; 2], b: [f64; 2], min: [f64; 2], max: [f64; 2]) -> bool {
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    let d = [b[0] - a[0], b[1] - a[1]];
    for (p, q) in [
        (-d[0], a[0] - min[0]),
        (d[0], max[0] - a[0]),
        (-d[1], a[1] - min[1]),
        (d[1], max[1] - a[1]),
    ] {
        if p.abs() < 1e-12 {
            if q < 0. {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0. {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Area selection between screen points `from` and `to` (drag order matters:
/// left→right selects objects fully inside, right→left also those touching).
pub fn area(
    project: &Project,
    p: &Projector,
    hidden: &BTreeSet<ObjectId>,
    from: [f64; 2],
    to: [f64; 2],
    filter: Filter,
) -> BTreeSet<ObjectId> {
    let window = to[0] >= from[0];
    let min = [from[0].min(to[0]), from[1].min(to[1])];
    let max = [from[0].max(to[0]), from[1].max(to[1])];
    let mut ids: Vec<ObjectId> = Vec::new();
    if filter.parts || filter.assemblies {
        ids.extend(project.elements.keys());
    }
    if filter.grids {
        ids.extend(project.axes.keys());
    }
    if filter.levels {
        ids.extend(project.levels.keys());
    }
    ids.extend(project.construction.keys());
    ids.extend(project.dimensions.keys());
    ids.extend(project.details.values().filter_map(|d| match d.kind {
        DetailKind::BoltGroup { .. } if filter.bolts => Some(d.id),
        DetailKind::Weld { .. } if filter.welds => Some(d.id),
        _ => None,
    }));
    let mut out = BTreeSet::new();
    for id in ids {
        if hidden.contains(&id) {
            continue;
        }
        let segs: Vec<([f64; 2], [f64; 2])> = segments(project, id)
            .into_iter()
            .filter_map(|(a, b)| Some((p.screen(a)?, p.screen(b)?)))
            .collect();
        if segs.is_empty() {
            continue;
        }
        let hit = if window {
            segs.iter()
                .all(|(a, b)| inside(*a, min, max) && inside(*b, min, max))
        } else {
            segs.iter().any(|(a, b)| touches(*a, *b, min, max))
        };
        if hit {
            out.insert(id);
        }
    }
    if filter.assemblies {
        expand_assemblies(project, &mut out);
    }
    out
}

/// Replace parts by all members of their assemblies.
pub fn expand_assemblies(project: &Project, ids: &mut BTreeSet<ObjectId>) {
    let extra: Vec<ObjectId> = project
        .assemblies
        .values()
        .filter(|a| a.members.iter().any(|m| ids.contains(m)))
        .flat_map(|a| a.members.iter().copied())
        .collect();
    ids.extend(extra);
}

/// The selected-object handle under the cursor (within `tolerance` px).
pub fn handle_at(handles: &[Handle], cursor: [f64; 2], tolerance: f64) -> Option<Handle> {
    handles
        .iter()
        .map(|h| (h, (h.screen[0] - cursor[0]).hypot(h.screen[1] - cursor[1])))
        .filter(|(_, d)| *d <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(h, _)| *h)
}

/// The other end of a member handle (reference for snapping while dragging).
pub fn anchor(project: &Project, handle: &Handle) -> Option<DVec3> {
    let e = project.elements.get(&handle.object)?;
    let ElementShape::Member { length, frame, .. } = &e.shape else {
        return None;
    };
    let (start, end) = (
        v(frame.origin),
        v(frame.world(Point3([0., 0., length.value()]))),
    );
    match handle.kind {
        HandleKind::Start => Some(end),
        HandleKind::End => Some(start),
        HandleKind::Vertex(_) => None,
    }
}

/// Direct modification: move one member end or one plate vertex to `target`.
pub fn move_handle(
    project: &Project,
    handle: &Handle,
    target: DVec3,
) -> Result<Vec<Command>, String> {
    if !target.is_finite() {
        return Err("Точка вне диапазона".into());
    }
    if let Some(axis) = project.axes.get(&handle.object) {
        let mut axis = axis.clone();
        let p = Point3(target.to_array());
        match handle.kind {
            HandleKind::Start => axis.start = p,
            HandleKind::End => axis.end = p,
            HandleKind::Vertex(_) => return Err("Ручка не принадлежит оси".into()),
        }
        if v(axis.start).distance(v(axis.end)) < 1. {
            return Err("Ось нулевой длины".into());
        }
        return Ok(vec![Command::ReplaceAxis { axis }]);
    }
    let mut element = project
        .elements
        .get(&handle.object)
        .ok_or("Объект удалён")?
        .clone();
    element.shape = match (element.shape.clone(), handle.kind) {
        (
            ElementShape::Member {
                member_kind,
                profile,
                length,
                frame,
            },
            HandleKind::Start | HandleKind::End,
        ) => {
            let start = v(frame.origin);
            let end = v(frame.world(Point3([0., 0., length.value()])));
            let (a, b) = if handle.kind == HandleKind::Start {
                (target, end)
            } else {
                (start, target)
            };
            let (moved, length) = Frame::along(Point3(a.to_array()), Point3(b.to_array()))
                .map_err(|e| e.to_string())?;
            // Keep the profile rotation when the direction does not change.
            let frame = if (v(Point3(moved.axes[2])) - v(Point3(frame.axes[2]))).length() < 1e-9 {
                Frame {
                    origin: moved.origin,
                    axes: frame.axes,
                }
            } else {
                moved
            };
            ElementShape::Member {
                member_kind,
                profile,
                length,
                frame,
            }
        }
        (
            ElementShape::Plate {
                mut contour,
                thickness,
                frame,
            },
            HandleKind::Vertex(i),
        ) => {
            let o = v(frame.origin);
            let (ax, ay) = (v(Point3(frame.axes[0])), v(Point3(frame.axes[1])));
            let d = target - o;
            *contour.get_mut(i).ok_or("Нет такой вершины")? = [d.dot(ax), d.dot(ay)];
            open_bim_model::polygon_area(&contour).map_err(|e| e.to_string())?;
            ElementShape::Plate {
                contour,
                thickness,
                frame,
            }
        }
        _ => return Err("Ручка не принадлежит объекту".into()),
    };
    Ok(vec![Command::ReplaceElement { element }])
}

/// Objects removed by deleting `ids` with their dependants (Tekla asks first).
pub fn delete_count(project: &Project, ids: &BTreeSet<ObjectId>) -> Result<usize, String> {
    let mut doc = Document::new(project.clone()).map_err(|e| e.to_string())?;
    let commands: Vec<Command> = ids
        .iter()
        .filter(|id| project.contains_id(**id))
        .map(|id| Command::Delete {
            id: *id,
            policy: DeletePolicy::Cascade,
        })
        .collect();
    doc.transact("Удалить", &commands)
        .map_err(|e| e.to_string())?;
    let count = |p: &Project| {
        p.elements.len() + p.axes.len() + p.details.len() + p.connections.len() + p.assemblies.len()
    };
    Ok(count(project) - count(doc.project()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOp {
    Move,
    Copy,
    Mirror,
    Rotate,
}

impl EditOp {
    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Переместить",
            Self::Copy => "Копировать",
            Self::Mirror => "Зеркально",
            Self::Rotate => "Повернуть",
        }
    }
}

/// Pick state of move/copy (origin, destination; copy repeats), mirror (two
/// points of the mirror line on the work plane) and rotate (centre, from, to).
#[derive(Clone, Debug, Default)]
pub struct EditPicker {
    pub op: Option<EditOp>,
    pub points: Vec<DVec3>,
}

impl EditPicker {
    pub fn prompt(&self) -> Option<String> {
        Some(
            match (self.op?, self.points.len()) {
                (EditOp::Move | EditOp::Copy, 0) => "Укажите исходную точку",
                (EditOp::Move, _) => "Укажите точку назначения",
                (EditOp::Copy, _) => "Укажите точку назначения копии (Esc — завершить)",
                (EditOp::Mirror, 0) => "Укажите первую точку линии зеркала",
                (EditOp::Mirror, _) => "Укажите вторую точку линии зеркала",
                (EditOp::Rotate, 0) => "Укажите центр поворота",
                (EditOp::Rotate, 1) => "Укажите начальное направление",
                (EditOp::Rotate, _) => "Укажите конечное направление",
            }
            .into(),
        )
    }

    /// Accept a pick; returns the edit when the sequence is complete.
    pub fn pick(&mut self, point: DVec3, normal: DVec3) -> Result<Option<Edit>, String> {
        let op = self.op.ok_or("Нет команды правки")?;
        self.points.push(point);
        let p = |v: DVec3| Point3(v.to_array());
        let edit = match (op, self.points.as_slice()) {
            (EditOp::Move, [a, b]) => Edit::Move(p(*b - *a)),
            (EditOp::Copy, [a, .., b]) => Edit::Copy(p(*b - *a)),
            (EditOp::Mirror, [a, b]) => {
                let n = (*b - *a)
                    .cross(normal)
                    .try_normalize()
                    .ok_or("Линия зеркала вырождена")?;
                Edit::Mirror {
                    normal: p(n),
                    offset: n.dot(*a),
                }
            }
            (EditOp::Rotate, [c, from, to]) => {
                let flat = |q: DVec3| (q - *c) - normal * (q - *c).dot(normal);
                let (u, w) = (
                    flat(*from)
                        .try_normalize()
                        .ok_or("Направление совпадает с центром")?,
                    flat(*to)
                        .try_normalize()
                        .ok_or("Направление совпадает с центром")?,
                );
                let degrees = u.cross(w).dot(normal).atan2(u.dot(w)).to_degrees();
                Edit::Rotate {
                    axis: p(normal),
                    degrees,
                    pivot: p(*c),
                }
            }
            _ => return Ok(None),
        };
        match op {
            // Copy keeps its origin and repeats; the others finish.
            EditOp::Copy => self.points.truncate(1),
            _ => *self = Self::default(),
        }
        Ok(Some(edit))
    }

    pub fn cancel(&mut self) -> bool {
        let was = self.op.is_some();
        *self = Self::default();
        was
    }
}
