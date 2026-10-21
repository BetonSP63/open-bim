//! Plate corner chamfers («Фаска угла»): a straight cut by two legs or a
//! rounding by a radius at chosen contour corners. The cut is applied to the
//! contour points, so drawings, NC and the bill of materials see the real
//! outline.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerKind {
    /// Straight cut: leg along the edge to the previous point, leg along the
    /// edge to the next point.
    #[default]
    Line,
    /// Arc tangent to both edges.
    Round,
}

impl CornerKind {
    pub const ALL: [Self; 2] = [Self::Line, Self::Round];
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Фаска",
            Self::Round => "Скругление",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub kind: CornerKind,
    /// Leg along the edge to the previous point, or the radius, mm.
    pub x: f64,
    /// Leg along the edge to the next point (straight cut only), mm.
    pub y: f64,
}

/// Points of an arc are spaced at most this many degrees apart.
const ARC_STEP_DEG: f64 = 15.0;

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn len(v: [f64; 2]) -> f64 {
    v[0].hypot(v[1])
}
fn at(p: [f64; 2], dir: [f64; 2], k: f64) -> [f64; 2] {
    [p[0] + dir[0] * k, p[1] + dir[1] * k]
}

/// Corner numbers from text: «все» or numbers from 1 separated by spaces
/// or commas.
pub fn parse_corners(text: &str, count: usize) -> Result<Vec<usize>, String> {
    let t = text.trim().to_lowercase();
    if t.is_empty() || t == "все" || t == "*" {
        return Ok((0..count).collect());
    }
    let mut out = Vec::new();
    for part in t.split([' ', ',', ';']).filter(|s| !s.is_empty()) {
        let n: usize = part
            .parse()
            .map_err(|_| format!("Номер угла: {part} (1…{count} или «все»)"))?;
        if n == 0 || n > count {
            return Err(format!("Нет угла {n}: у контура {count} углов"));
        }
        if !out.contains(&(n - 1)) {
            out.push(n - 1);
        }
    }
    Ok(out)
}

/// The contour with the given corners cut. Refused when a cut is longer
/// than its edge (with the cut at the other end of the edge) or the
/// outline would cross itself.
pub fn chamfer(
    contour: &[[f64; 2]],
    corners: &[usize],
    corner: Corner,
) -> Result<Vec<[f64; 2]>, String> {
    let n = contour.len();
    if n < 3 {
        return Err("Контур пластины: не меньше трёх точек".into());
    }
    let valid = |v: f64| v.is_finite() && v > 0.0 && v <= 1e6;
    if !valid(corner.x) || corner.kind == CornerKind::Line && !valid(corner.y) {
        return Err("Размер фаски: больше 0 мм".into());
    }
    // Distance cut off each edge from its start (index i) and end.
    let mut cut_start = vec![0.0; n];
    let mut cut_end = vec![0.0; n];
    let mut replaced: Vec<Option<Vec<[f64; 2]>>> = vec![None; n];
    for &i in corners {
        let v = contour[i];
        let prev = contour[(i + n - 1) % n];
        let next = contour[(i + 1) % n];
        let (a, b) = (sub(prev, v), sub(next, v));
        let (la, lb) = (len(a), len(b));
        if la < 1e-9 || lb < 1e-9 {
            return Err(format!("Угол {}: совпадающие точки", i + 1));
        }
        let (ua, ub) = ([a[0] / la, a[1] / la], [b[0] / lb, b[1] / lb]);
        let cos = (ua[0] * ub[0] + ua[1] * ub[1]).clamp(-1.0, 1.0);
        let angle = cos.acos();
        if angle > std::f64::consts::PI - 1e-6 {
            // A straight-through point has no corner to cut.
            continue;
        }
        let (da, db, points) = match corner.kind {
            CornerKind::Line => (
                corner.x,
                corner.y,
                vec![at(v, ua, corner.x), at(v, ub, corner.y)],
            ),
            CornerKind::Round => {
                let r = corner.x;
                let t = r / (angle / 2.0).tan();
                let bis = [ua[0] + ub[0], ua[1] + ub[1]];
                let lbis = len(bis);
                let centre = at(v, [bis[0] / lbis, bis[1] / lbis], r / (angle / 2.0).sin());
                let (pa, pb) = (at(v, ua, t), at(v, ub, t));
                let (sa, sb) = (sub(pa, centre), sub(pb, centre));
                let start = sa[1].atan2(sa[0]);
                let mut sweep = sb[1].atan2(sb[0]) - start;
                // The arc between the tangent points turns less than 180°.
                while sweep > std::f64::consts::PI {
                    sweep -= 2.0 * std::f64::consts::PI;
                }
                while sweep < -std::f64::consts::PI {
                    sweep += 2.0 * std::f64::consts::PI;
                }
                let steps = (sweep.abs().to_degrees() / ARC_STEP_DEG).ceil().max(1.0) as usize;
                let points = (0..=steps)
                    .map(|k| {
                        let phi = start + sweep * k as f64 / steps as f64;
                        [centre[0] + r * phi.cos(), centre[1] + r * phi.sin()]
                    })
                    .collect();
                (t, t, points)
            }
        };
        cut_end[(i + n - 1) % n] = da;
        cut_start[i] = db;
        replaced[i] = Some(points);
    }
    for e in 0..n {
        let edge = len(sub(contour[(e + 1) % n], contour[e]));
        if cut_start[e] + cut_end[e] > edge + 1e-6 {
            return Err(format!(
                "Фаска длиннее стороны {}–{} ({:.0} мм)",
                e + 1,
                (e + 1) % n + 1,
                edge
            ));
        }
    }
    let mut out: Vec<[f64; 2]> = Vec::new();
    for (i, p) in contour.iter().enumerate() {
        match &replaced[i] {
            Some(points) => out.extend(points),
            None => out.push(*p),
        }
    }
    // Cuts that meet in the middle of an edge leave one point there.
    out.dedup_by(|a, b| len(sub(*a, *b)) < 1e-6);
    if out.len() > 1 && len(sub(out[0], out[out.len() - 1])) < 1e-6 {
        out.pop();
    }
    open_bim_model::polygon_area(&out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Fields of «Фаски углов» in the property pane.
#[derive(Clone, Debug)]
pub struct CornerUi {
    pub kind: CornerKind,
    pub corners: String,
    pub x: String,
    pub y: String,
}

impl Default for CornerUi {
    fn default() -> Self {
        Self {
            kind: CornerKind::Line,
            corners: "все".into(),
            x: "20".into(),
            y: "20".into(),
        }
    }
}

impl CornerUi {
    pub fn corner(&self) -> Result<Corner, String> {
        let n = |s: &str| crate::tools::number(&s.replace(',', "."));
        let x = n(&self.x)?;
        Ok(Corner {
            kind: self.kind,
            x,
            y: if self.kind == CornerKind::Line {
                n(&self.y)?
            } else {
                x
            },
        })
    }

    /// Cut the corners of each selected plate; other parts are refused.
    pub fn commands(
        &self,
        project: &open_bim_model::Project,
        parts: &[open_bim_model::ObjectId],
    ) -> Result<Vec<open_bim_commands::Command>, String> {
        let corner = self.corner()?;
        parts
            .iter()
            .map(|id| {
                let mut element = project.elements.get(id).ok_or("Выберите пластину")?.clone();
                let open_bim_model::ElementShape::Plate { contour, .. } = &mut element.shape else {
                    return Err("Фаски углов — только для пластин".to_string());
                };
                let corners = parse_corners(&self.corners, contour.len())?;
                *contour = chamfer(contour, &corners, corner)
                    .map_err(|e| format!("{}: {e}", element.name))?;
                Ok(open_bim_commands::Command::ReplaceElement { element })
            })
            .collect()
    }
}
