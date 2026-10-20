//! Contour plate picking: a pick that would make the contour cross itself is
//! refused with a message, rectangles by two or three points, and the plate
//! position across its thickness relative to the work plane.

/// How the plate contour is picked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlateMode {
    /// Any polygon; Enter or the middle button closes it.
    #[default]
    Polygon,
    /// Two diagonal corners of an axis-aligned rectangle (work plane axes).
    Rectangle,
    /// Start and end of one side, then a point on the opposite side.
    RotatedRectangle,
}

impl PlateMode {
    pub const ALL: [PlateMode; 3] = [Self::Polygon, Self::Rectangle, Self::RotatedRectangle];
    pub fn label(self) -> &'static str {
        match self {
            Self::Polygon => "Контур по точкам",
            Self::Rectangle => "Прямоугольник по 2 точкам",
            Self::RotatedRectangle => "Прямоугольник по 3 точкам",
        }
    }
}

/// Plate position across its thickness relative to the work plane
/// («At depth» for plates).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlatePosition {
    /// Above the plane: z ∈ [0, t].
    #[default]
    Up,
    /// Centred: z ∈ [-t/2, t/2].
    Middle,
    /// Below: z ∈ [-t, 0].
    Down,
}

impl PlatePosition {
    pub const ALL: [PlatePosition; 3] = [Self::Up, Self::Middle, Self::Down];
    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "Над плоскостью",
            Self::Middle => "По центру",
            Self::Down => "Под плоскостью",
        }
    }
    /// Offset of the plate's lower face from the work plane.
    pub fn offset(self, thickness: f64) -> f64 {
        match self {
            Self::Up => 0.,
            Self::Middle => -thickness / 2.,
            Self::Down => -thickness,
        }
    }
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2], eps: f64) -> bool {
    p[0] >= a[0].min(b[0]) - eps
        && p[0] <= a[0].max(b[0]) + eps
        && p[1] >= a[1].min(b[1]) - eps
        && p[1] <= a[1].max(b[1]) + eps
}

/// Whether segments a–b and c–d touch or cross (shared endpoints count).
pub fn segments_touch(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let eps = 1e-6;
    let (d1, d2, d3, d4) = (
        cross(c, d, a),
        cross(c, d, b),
        cross(a, b, c),
        cross(a, b, d),
    );
    if ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
        && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
    {
        return true;
    }
    (d1.abs() <= eps && on_segment(c, d, a, eps))
        || (d2.abs() <= eps && on_segment(c, d, b, eps))
        || (d3.abs() <= eps && on_segment(a, b, c, eps))
        || (d4.abs() <= eps && on_segment(a, b, d, eps))
}

/// Check the next contour point (work plane coordinates) against the points
/// picked so far: no repeated point and no crossing of earlier sides.
pub fn check_next(points: &[[f64; 2]], next: [f64; 2]) -> Result<(), String> {
    if let Some(last) = points.last() {
        if (last[0] - next[0]).hypot(last[1] - next[1]) < 1.0 {
            return Err("Точка совпадает с предыдущей — укажите другую".into());
        }
        // The new side last→next must not touch any non-adjacent earlier side.
        let n = points.len();
        for i in 0..n.saturating_sub(2) {
            if segments_touch(points[i], points[i + 1], *last, next) {
                return Err(format!(
                    "Сторона {}–{} пересекает сторону {}–{}: точка не принята (Backspace — убрать последнюю точку)",
                    n,
                    n + 1,
                    i + 1,
                    i + 2
                ));
            }
        }
        if n >= 2
            && points[..n - 1]
                .iter()
                .any(|p| (p[0] - next[0]).hypot(p[1] - next[1]) < 1.0)
        {
            return Err("Контур возвращается в уже указанную точку".into());
        }
    }
    Ok(())
}

/// Check the closing side (last → first) when the contour is finished.
pub fn check_closed(points: &[[f64; 2]]) -> Result<(), String> {
    let n = points.len();
    if n < 3 {
        return Err("Нужно не меньше 3 точек".into());
    }
    let (last, first) = (points[n - 1], points[0]);
    for i in 1..n - 2 {
        if segments_touch(points[i], points[i + 1], last, first) {
            return Err(format!(
                "Замыкающая сторона {}–1 пересекает сторону {}–{}: добавьте или уберите точки",
                n,
                i + 1,
                i + 2
            ));
        }
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.;
    if area.abs() < 1.0 {
        return Err("Контур вырожден (нулевая площадь)".into());
    }
    Ok(())
}

/// Rectangle with sides along the work plane axes from two diagonal corners.
pub fn rectangle(a: [f64; 2], b: [f64; 2]) -> Result<Vec<[f64; 2]>, String> {
    if (a[0] - b[0]).abs() < 1.0 || (a[1] - b[1]).abs() < 1.0 {
        return Err("Углы прямоугольника должны различаться по обеим осям".into());
    }
    Ok(vec![a, [b[0], a[1]], b, [a[0], b[1]]])
}

/// Rectangle with side a–b and the opposite side through c.
pub fn rotated_rectangle(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Result<Vec<[f64; 2]>, String> {
    let side = [b[0] - a[0], b[1] - a[1]];
    let len = side[0].hypot(side[1]);
    if len < 1.0 {
        return Err("Сторона прямоугольника нулевой длины".into());
    }
    let n = [-side[1] / len, side[0] / len];
    let w = (c[0] - b[0]) * n[0] + (c[1] - b[1]) * n[1];
    if w.abs() < 1.0 {
        return Err("Третья точка лежит на стороне — ширина нулевая".into());
    }
    Ok(vec![
        a,
        b,
        [b[0] + n[0] * w, b[1] + n[1] * w],
        [a[0] + n[0] * w, a[1] + n[1] * w],
    ])
}

/// Ear-clipping triangulation of a simple polygon (for previews).
pub fn triangulate(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
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
