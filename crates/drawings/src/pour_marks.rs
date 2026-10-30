//! Construction joints and pour zones on concrete arrangement drawings: a
//! joint seen edge-on is a thick dash-dot line with the note «Шов
//! бетонирования» on a leader shelf; on plans each pour zone gets
//! «Захватка N» at its centre.
use crate::conventions::LEVEL_TEXT;
use crate::{Align, Label, LineStyle, Polyline, ViewContext, ViewGraphic};
use open_bim_model::{ObjectId, PourBreak, PourZone, Project};
use std::collections::BTreeSet;

/// Joint lines and zone labels for the concrete parts shown in a view.
pub(crate) fn annotate(
    project: &Project,
    zones: &[PourZone],
    shown: &BTreeSet<ObjectId>,
    ctx: &ViewContext,
    plan: bool,
    graphic: &mut ViewGraphic,
) {
    let boxes: Vec<[[f64; 3]; 2]> = shown
        .iter()
        .filter(|id| project.concrete_type(**id).is_some())
        .filter_map(|id| project.element_bounds(*id).ok())
        .map(|b| {
            [
                std::array::from_fn(|k| b[0][k] - 1.),
                std::array::from_fn(|k| b[1][k] + 1.),
            ]
        })
        .collect();
    let paper = |p: [f64; 3]| {
        let q = ctx.basis.project(p);
        ctx.paper([q[0], q[1]])
    };
    for joint in &project.concrete.breaks {
        let Some(quad) = joint_quad(joint, &boxes) else {
            continue;
        };
        let points: Vec<[f64; 2]> = quad.iter().map(|p| paper(*p)).collect();
        let Some((a, b)) = edge_on(&points) else {
            continue;
        };
        graphic.lines.push(Polyline {
            points: vec![a, b],
            style: LineStyle::ThickDashDot,
            object: None,
        });
        // The note on a leader shelf from the upper end of the line.
        let top = if (b[1], b[0]) >= (a[1], a[0]) { b } else { a };
        let text = "Шов бетонирования";
        let width = text.chars().count() as f64 * LEVEL_TEXT * 0.62;
        let knee = [top[0] + 4., top[1] + 4.];
        graphic.lines.push(Polyline {
            points: vec![top, knee, [knee[0] + width, knee[1]]],
            style: LineStyle::Thin,
            object: None,
        });
        graphic.labels.push(Label {
            position: [knee[0], knee[1] + 1.],
            text: text.into(),
            height: LEVEL_TEXT,
            align: Align::Start,
            rotation: 0.,
            object: None,
            lost: false,
        });
    }
    // Zone numbers go on plans.
    if zones.len() < 2 || !plan {
        return;
    }
    for z in zones {
        if !z.pieces.iter().any(|p| shown.contains(&p.part)) {
            continue;
        }
        graphic.labels.push(Label {
            position: paper(z.centre()),
            text: format!("Захватка {}", z.number),
            height: LEVEL_TEXT,
            align: Align::Middle,
            rotation: 0.,
            object: None,
            lost: false,
        });
    }
}

/// The joint face over the shown parts it cuts or bounds, world mm.
fn joint_quad(joint: &PourBreak, boxes: &[[[f64; 3]; 2]]) -> Option<[[f64; 3]; 4]> {
    let hit: Vec<&[[f64; 3]; 2]> = boxes.iter().filter(|b| joint.cuts(b)).collect();
    if hit.is_empty() {
        return None;
    }
    let lo: [f64; 3] =
        std::array::from_fn(|k| hit.iter().map(|b| b[0][k]).fold(f64::MAX, f64::min));
    let hi: [f64; 3] =
        std::array::from_fn(|k| hit.iter().map(|b| b[1][k]).fold(f64::MIN, f64::max));
    let (s, e) = (joint.start.0, joint.end.0);
    if joint.horizontal {
        let x0 = s[0].min(e[0]).max(lo[0]);
        let x1 = s[0].max(e[0]).min(hi[0]);
        let y0 = s[1].min(e[1]).max(lo[1]);
        let y1 = s[1].max(e[1]).min(hi[1]);
        let z = s[2];
        return Some([[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]);
    }
    // The joint line clipped to the parts' extent in plan.
    let (mut t0, mut t1) = (0f64, 1f64);
    for k in 0..2 {
        let d = e[k] - s[k];
        if d.abs() > 1e-12 {
            let (u, v) = ((lo[k] - s[k]) / d, (hi[k] - s[k]) / d);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
        }
    }
    if t0 >= t1 {
        return None;
    }
    let at = |t: f64, z: f64| [s[0] + (e[0] - s[0]) * t, s[1] + (e[1] - s[1]) * t, z];
    Some([at(t0, lo[2]), at(t1, lo[2]), at(t1, hi[2]), at(t0, hi[2])])
}

/// The quad's two farthest points when the quad projects to a line.
fn edge_on(points: &[[f64; 2]]) -> Option<([f64; 2], [f64; 2])> {
    let dist = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let mut best = (0., points[0], points[0]);
    for (i, a) in points.iter().enumerate() {
        for b in &points[i + 1..] {
            if dist(*a, *b) > best.0 {
                best = (dist(*a, *b), *a, *b);
            }
        }
    }
    let (length, a, b) = best;
    if length < 1. {
        return None;
    }
    let width = points
        .iter()
        .map(|p| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / length)
        .fold(0., f64::max);
    (width < 0.5).then_some((a, b))
}
