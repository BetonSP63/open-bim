use open_bim_model::{ElementShape, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapMode {
    All,
    Axis,
    End,
    Midpoint,
    Intersection,
}
#[derive(Clone, Copy, Debug)]
pub struct SnapHit {
    pub point: Point3,
    pub kind: SnapMode,
    pub owner: ObjectId,
    pub other: Option<ObjectId>,
}
#[derive(Clone, Copy, Debug)]
struct Segment {
    id: ObjectId,
    start: DVec3,
    end: DVec3,
    axis: bool,
}
#[derive(Clone, Debug, Default)]
pub struct SnapIndex {
    segments: Vec<Segment>,
}
impl SnapIndex {
    pub fn new(project: &Project) -> Self {
        let mut segments: Vec<_> = project
            .axes
            .values()
            .map(|a| Segment {
                id: a.id,
                start: DVec3::from_array(a.start.0),
                end: DVec3::from_array(a.end.0),
                axis: true,
            })
            .collect();
        for element in project.elements.values() {
            match &element.shape {
                ElementShape::Member { length, frame, .. } => segments.push(Segment {
                    id: element.id,
                    start: DVec3::from_array(frame.origin.0),
                    end: DVec3::from_array(frame.world(Point3([0.0, 0.0, length.value()])).0),
                    axis: false,
                }),
                ElementShape::Plate { contour, frame, .. } => {
                    for i in 0..contour.len() {
                        let a = contour[i];
                        let b = contour[(i + 1) % contour.len()];
                        segments.push(Segment {
                            id: element.id,
                            start: DVec3::from_array(frame.world(Point3([a[0], a[1], 0.0])).0),
                            end: DVec3::from_array(frame.world(Point3([b[0], b[1], 0.0])).0),
                            axis: false,
                        });
                    }
                }
            }
        }
        Self { segments }
    }
    /// All distances are world millimetres. Hidden objects never attract the pointer.
    pub fn snap(
        &self,
        point: Point3,
        tolerance: f64,
        mode: SnapMode,
        hidden: &BTreeSet<ObjectId>,
    ) -> Option<SnapHit> {
        let query = DVec3::from_array(point.0);
        if !query.is_finite() || !tolerance.is_finite() || tolerance <= 0.0 {
            return None;
        }
        let mut best: Option<(f64, u8, SnapHit)> = None;
        let mut offer = |p: DVec3, kind: SnapMode, owner: ObjectId, other: Option<ObjectId>| {
            let distance = p.distance(query);
            let priority = match kind {
                SnapMode::Intersection => 0,
                SnapMode::End => 1,
                SnapMode::Midpoint => 2,
                _ => 3,
            };
            if distance <= tolerance
                && best.as_ref().is_none_or(|(d, priority_old, _)| {
                    priority < *priority_old || (priority == *priority_old && distance < *d - 1e-9)
                })
            {
                best = Some((
                    distance,
                    priority,
                    SnapHit {
                        point: Point3(p.to_array()),
                        kind,
                        owner,
                        other,
                    },
                ));
            }
        };
        let visible: Vec<_> = self
            .segments
            .iter()
            .filter(|s| !hidden.contains(&s.id))
            .collect();
        for s in &visible {
            if matches!(mode, SnapMode::All | SnapMode::End) {
                offer(s.start, SnapMode::End, s.id, None);
                offer(s.end, SnapMode::End, s.id, None);
            }
            if matches!(mode, SnapMode::All | SnapMode::Midpoint) {
                offer((s.start + s.end) * 0.5, SnapMode::Midpoint, s.id, None);
            }
            if s.axis && matches!(mode, SnapMode::All | SnapMode::Axis) {
                let direction = s.end - s.start;
                let t = (query - s.start).dot(direction) / direction.length_squared();
                offer(
                    s.start + direction * t.clamp(0.0, 1.0),
                    SnapMode::Axis,
                    s.id,
                    None,
                );
            }
        }
        if matches!(mode, SnapMode::All | SnapMode::Intersection) {
            // Grid axes only: do not build quadratic pairs of 100k members.
            let axes: Vec<_> = visible.into_iter().filter(|s| s.axis).collect();
            for (i, a) in axes.iter().enumerate() {
                for b in axes.iter().skip(i + 1) {
                    let u = a.end - a.start;
                    let v = b.end - b.start;
                    let w = a.start - b.start;
                    let aa = u.dot(u);
                    let bb = u.dot(v);
                    let cc = v.dot(v);
                    let dd = u.dot(w);
                    let ee = v.dot(w);
                    let denominator = aa * cc - bb * bb;
                    if denominator.abs() <= 1e-12 * aa * cc {
                        continue;
                    }
                    let t = (bb * ee - cc * dd) / denominator;
                    let q = (aa * ee - bb * dd) / denominator;
                    if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&q) {
                        continue;
                    }
                    let pa = a.start + u * t;
                    let pb = b.start + v * q;
                    if pa.distance(pb) <= 0.0001 {
                        offer((pa + pb) * 0.5, SnapMode::Intersection, a.id, Some(b.id));
                    }
                }
            }
        }
        best.map(|(_, _, hit)| hit)
    }
}
