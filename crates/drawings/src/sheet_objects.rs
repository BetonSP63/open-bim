//! Sheet graphics of the user's drawing objects: line styles of ГОСТ 2.303,
//! hatching lines clipped to their area (ГОСТ 2.306) and masking areas.
use crate::{LineStyle, ObjectGraphic, Polyline};
use open_bim_model::{HatchPattern, LineType, SheetObject, SheetObjectKind};

fn style(o: &SheetObject) -> LineStyle {
    match (o.line_type, o.thick) {
        (LineType::Solid, true) => LineStyle::Visible,
        (LineType::Solid, false) => LineStyle::Thin,
        (LineType::Dashed, false) => LineStyle::Hidden,
        (LineType::Dashed, true) => LineStyle::ThickDashed,
        (LineType::DashDot, false) => LineStyle::DashDot,
        (LineType::DashDot, true) => LineStyle::ThickDashDot,
    }
}

/// Lines `spacing` apart at `angle` degrees, clipped to a simple polygon.
pub fn hatch_lines(polygon: &[[f64; 2]], angle: f64, spacing: f64) -> Vec<[[f64; 2]; 2]> {
    let (s, c) = angle.to_radians().sin_cos();
    let (u, n) = ([c, s], [-s, c]);
    let dot = |p: [f64; 2], d: [f64; 2]| p[0] * d[0] + p[1] * d[1];
    let (lo, hi) = polygon
        .iter()
        .map(|p| dot(*p, n))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v), b.max(v))
        });
    let mut out = Vec::new();
    let mut k = (lo / spacing).ceil();
    while k * spacing <= hi && out.len() < 100_000 {
        let level = k * spacing;
        let mut hits: Vec<f64> = Vec::new();
        for i in 0..polygon.len() {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            let (da, db) = (dot(a, n) - level, dot(b, n) - level);
            if (da < 0.) != (db < 0.) {
                let t = da / (da - db);
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                hits.push(dot(p, u));
            }
        }
        hits.sort_by(f64::total_cmp);
        for pair in hits.as_chunks::<2>().0 {
            let at = |t: f64| [u[0] * t + n[0] * level, u[1] * t + n[1] * level];
            out.push([at(pair[0]), at(pair[1])]);
        }
        k += 1.;
    }
    out
}

pub fn graphic(o: &SheetObject) -> ObjectGraphic {
    let line = |points: Vec<[f64; 2]>, style| Polyline {
        points,
        style,
        object: None,
    };
    let (lines, mask) = match &o.kind {
        SheetObjectKind::Line { points } => (vec![line(points.clone(), style(o))], None),
        SheetObjectKind::Rectangle { a, b } => (
            vec![line(vec![*a, [b[0], a[1]], *b, [a[0], b[1]], *a], style(o))],
            None,
        ),
        SheetObjectKind::Circle { center, radius } => (
            vec![line(
                (0..=72)
                    .map(|k| {
                        let t = k as f64 / 72. * std::f64::consts::TAU;
                        [center[0] + radius * t.cos(), center[1] + radius * t.sin()]
                    })
                    .collect(),
                style(o),
            )],
            None,
        ),
        SheetObjectKind::Hatch {
            polygon,
            pattern,
            angle,
            spacing,
        } => {
            let angles: &[f64] = match pattern {
                HatchPattern::Metal => &[0.],
                HatchPattern::NonMetal => &[0., 90.],
            };
            let lines = angles
                .iter()
                .flat_map(|extra| hatch_lines(polygon, angle + extra, *spacing))
                .map(|[a, b]| line(vec![a, b], LineStyle::Hatch))
                .collect();
            (lines, None)
        }
        SheetObjectKind::Wipeout { polygon } => (Vec::new(), Some(polygon.clone())),
    };
    ObjectGraphic {
        id: o.id,
        lines,
        mask,
    }
}

/// Ear-clipping triangulation of a simple polygon (masks in DXF, previews).
pub fn triangulate(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let mut idx: Vec<usize> = if area >= 0. {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };
    let mut out = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let ear = (0..m).find(|&i| {
            let (p, c, q) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            if cross(points[p], points[c], points[q]) <= 1e-12 {
                return false;
            }
            idx.iter().all(|&o| {
                o == p
                    || o == c
                    || o == q
                    || !(cross(points[p], points[c], points[o]) >= 0.
                        && cross(points[c], points[q], points[o]) >= 0.
                        && cross(points[q], points[p], points[o]) >= 0.)
            })
        });
        match ear {
            Some(i) => {
                out.push([idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]]);
                idx.remove(i);
            }
            None => break,
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hatch_fills_a_square_at_45_degrees() {
        let square = [[0., 0.], [10., 0.], [10., 10.], [0., 10.]];
        let lines = hatch_lines(&square, 45., 2.);
        assert!(!lines.is_empty());
        for [a, b] in &lines {
            let d = [b[0] - a[0], b[1] - a[1]];
            assert!((d[1].atan2(d[0]).to_degrees() - 45.).abs() < 1e-9);
            for p in [a, b] {
                assert!(p[0] > -1e-9 && p[0] < 10. + 1e-9 && p[1] > -1e-9 && p[1] < 10. + 1e-9);
            }
        }
        // Consecutive lines are 2 mm apart.
        let offset = |l: &[[f64; 2]; 2]| (l[0][1] - l[0][0]) / 2f64.sqrt();
        assert!(((offset(&lines[1]) - offset(&lines[0])).abs() - 2.).abs() < 1e-9);
    }
}
