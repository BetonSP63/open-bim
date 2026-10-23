//! Analytical model of a structure for analysis software: nodes at member
//! ends and crossings (merged within a tolerance), bar elements between
//! them, rigid links and the loads of the chosen groups. Derived from the
//! physical model and its analysis properties every time, never stored.
use open_bim_model::{
    AnalysisModel, ElementShape, Load, Members, ObjectId, PartAnalysis, PartRole, Project,
};

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn dist(a: V, b: V) -> f64 {
    dot(sub(a, b), sub(a, b)).sqrt()
}

/// One bar of the analytical model.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    /// The part it comes from.
    pub part: ObjectId,
    /// Node indices (0-based; node numbers are index + 1).
    pub nodes: [usize; 2],
    /// Moment releases at the two ends.
    pub hinges: [bool; 2],
    /// Axial force only.
    pub truss: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analytical {
    /// Node coordinates, numbered in order (number = index + 1): by level,
    /// then Y, then X.
    pub nodes: Vec<V>,
    pub bars: Vec<Bar>,
    /// Rigid links between nodes.
    pub links: Vec<[usize; 2]>,
    pub loads: Vec<Load>,
    /// Plates of the model, not taken into a bar model.
    pub plates: usize,
}

impl Analytical {
    /// The node nearest to `p` within `tolerance`.
    pub fn node_at(&self, p: V, tolerance: f64) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (i, dist(*n, p)))
            .filter(|(_, d)| *d <= tolerance)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

/// Analysis axis of a member part: start and end, with the axis offset.
pub fn member_axis(project: &Project, part: ObjectId) -> Option<(V, V)> {
    let e = project.elements.get(&part)?;
    let ElementShape::Member { frame, length, .. } = &e.shape else {
        return None;
    };
    let props = project
        .analysis
        .parts
        .get(&part)
        .cloned()
        .unwrap_or_default();
    let shift = add(
        mul(frame.axes[0], props.offset[0]),
        mul(frame.axes[1], props.offset[1]),
    );
    let a = add(frame.origin.0, shift);
    Some((a, add(a, mul(frame.axes[2], length.value()))))
}

/// Parameters (s on p0→p1, t on q0→q1, both 0..1) of the closest points of
/// two segments' lines, if not parallel.
fn closest(p: (V, V), q: (V, V)) -> Option<(f64, f64)> {
    let (d1, d2, r) = (sub(p.1, p.0), sub(q.1, q.0), sub(p.0, q.0));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (b, c) = (dot(d1, d2), dot(d1, r));
    let den = a * e - b * b;
    if den <= 1e-9 * a * e {
        return None;
    }
    let s = (b * f - c * e) / den;
    let t = (a * f - b * c) / den;
    Some((s, t))
}

/// Nodes within `tolerance` of each other become one (the first found).
#[derive(Default)]
struct Nodes {
    points: Vec<V>,
    tolerance: f64,
}

impl Nodes {
    fn add(&mut self, p: V) -> usize {
        if let Some(i) = self
            .points
            .iter()
            .position(|q| dist(*q, p) <= self.tolerance)
        {
            return i;
        }
        self.points.push(p);
        self.points.len() - 1
    }
}

/// Parts an analysis model takes: members in the model that are not left
/// out, and the number of plates it leaves aside.
pub fn parts(project: &Project, model: &AnalysisModel) -> (Vec<ObjectId>, usize) {
    let taken = |id: &ObjectId| match &model.members {
        Members::All => true,
        Members::Listed { parts } => parts.contains(id),
    };
    let mut members = Vec::new();
    let mut plates = 0;
    for (id, e) in &project.elements {
        if !taken(id) {
            continue;
        }
        let role = project.analysis.parts.get(id).map(|p| p.role);
        match e.shape {
            ElementShape::Plate { .. } => plates += 1,
            ElementShape::Member { .. } if role != Some(PartRole::Ignore) => members.push(*id),
            ElementShape::Member { .. } => {}
        }
    }
    (members, plates)
}

/// Build the analytical model of `model`.
pub fn build(project: &Project, model: &AnalysisModel) -> Analytical {
    let analysis = &project.analysis;
    let tol = model.merge_tolerance.max(0.1);
    let (members, plates) = parts(project, model);
    let axes: Vec<(ObjectId, (V, V))> = members
        .iter()
        .filter_map(|id| Some((*id, member_axis(project, *id)?)))
        .collect();
    // Candidate node points: ends, crossings, user nodes, link ends.
    let mut points: Vec<V> = Vec::new();
    for (_, (a, b)) in &axes {
        points.push(*a);
        points.push(*b);
    }
    for (i, (_, p)) in axes.iter().enumerate() {
        for (_, q) in &axes[i + 1..] {
            let Some((s, t)) = closest(*p, *q) else {
                continue;
            };
            let (lp, lq) = (dist(p.0, p.1), dist(q.0, q.1));
            let inside = |u: f64, len: f64| u >= -tol / len && u <= 1. + tol / len;
            if !(inside(s, lp) && inside(t, lq)) {
                continue;
            }
            let x = add(p.0, mul(sub(p.1, p.0), s.clamp(0., 1.)));
            let y = add(q.0, mul(sub(q.1, q.0), t.clamp(0., 1.)));
            if dist(x, y) <= tol {
                points.push(x);
            }
        }
    }
    points.extend(analysis.nodes.iter().copied());
    for l in &analysis.links {
        points.push(l.a);
        points.push(l.b);
    }
    // «Объединить узлы»: points near the second move to the first.
    let merged = |p: V| {
        analysis
            .merges
            .iter()
            .find(|[_, from]| dist(*from, p) <= tol)
            .map_or(p, |[to, _]| *to)
    };
    let mut nodes = Nodes {
        points: Vec::new(),
        tolerance: tol,
    };
    for p in points {
        nodes.add(merged(p));
    }
    // Number the nodes: by level, then Y, then X (rounded to the tolerance).
    let key = |p: &V| p.map(|v| (v / tol).round() as i64);
    let mut order: Vec<usize> = (0..nodes.points.len()).collect();
    order.sort_by_key(|i| {
        let [x, y, z] = key(&nodes.points[*i]);
        (z, y, x)
    });
    let points: Vec<V> = order.iter().map(|i| nodes.points[*i]).collect();
    // Bars: each member split at the nodes on its axis.
    let mut bars = Vec::new();
    for (part, (a, b)) in &axes {
        let props: PartAnalysis = analysis.parts.get(part).cloned().unwrap_or_default();
        let truss = props.role == PartRole::Truss;
        let len = dist(*a, *b).max(1e-9);
        let dir = mul(sub(*b, *a), 1. / len);
        let mut on: Vec<(f64, usize)> = points
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let t = dot(sub(*n, *a), dir);
                let foot = add(*a, mul(dir, t));
                (t >= -tol && t <= len + tol && dist(foot, *n) <= tol).then_some((t, i))
            })
            .collect();
        on.sort_by(|x, y| x.0.total_cmp(&y.0));
        on.dedup_by_key(|x| x.1);
        let count = on.len().saturating_sub(1);
        for (k, w) in on.windows(2).enumerate() {
            bars.push(Bar {
                part: *part,
                nodes: [w[0].1, w[1].1],
                hinges: [
                    truss || (k == 0 && props.hinges[0]),
                    truss || (k + 1 == count && props.hinges[1]),
                ],
                truss,
            });
        }
    }
    let node = |p: V| {
        points
            .iter()
            .enumerate()
            .min_by(|x, y| dist(*x.1, p).total_cmp(&dist(*y.1, p)))
            .map(|(i, _)| i)
    };
    let links = analysis
        .links
        .iter()
        .filter_map(|l| Some([node(merged(l.a))?, node(merged(l.b))?]))
        .filter(|[a, b]| a != b)
        .collect();
    let loads = analysis
        .loads
        .iter()
        .filter(|l| model.load_groups.is_empty() || model.load_groups.contains(&l.group))
        .cloned()
        .collect();
    Analytical {
        nodes: points,
        bars,
        links,
        loads,
        plates,
    }
}

fn number(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

/// CSV tables of the model (semicolon, decimal comma, UTF-8 with BOM):
/// nodes, bars, rigid links and loads, as (file name, text).
pub fn csv(project: &Project, a: &Analytical) -> Vec<(&'static str, String)> {
    let bom = "\u{feff}";
    let mut nodes = format!("{bom}Узел;X, мм;Y, мм;Z, мм\r\n");
    for (i, p) in a.nodes.iter().enumerate() {
        nodes.push_str(&format!(
            "{};{};{};{}\r\n",
            i + 1,
            number(p[0]),
            number(p[1]),
            number(p[2])
        ));
    }
    let mut bars = format!(
        "{bom}Элемент;Узел 1;Узел 2;Деталь;Профиль;Материал;Шарнир 1;Шарнир 2;Ферменный\r\n"
    );
    let yes = |b: bool| if b { "да" } else { "нет" };
    for (i, b) in a.bars.iter().enumerate() {
        let e = &project.elements[&b.part];
        let profile = match &e.shape {
            ElementShape::Member { profile, .. } => project
                .profiles
                .get(profile)
                .map_or(String::new(), |p| p.name.clone()),
            ElementShape::Plate { .. } => String::new(),
        };
        let material = project
            .materials
            .get(&e.material)
            .map_or(String::new(), |m| m.name.clone());
        bars.push_str(&format!(
            "{};{};{};{};{};{};{};{};{}\r\n",
            i + 1,
            b.nodes[0] + 1,
            b.nodes[1] + 1,
            e.name.replace(';', ","),
            profile.replace(';', ","),
            material.replace(';', ","),
            yes(b.hinges[0]),
            yes(b.hinges[1]),
            yes(b.truss)
        ));
    }
    let mut links = format!("{bom}Связь;Узел 1;Узел 2\r\n");
    for (i, [p, q]) in a.links.iter().enumerate() {
        links.push_str(&format!("{};{};{}\r\n", i + 1, p + 1, q + 1));
    }
    let mut loads = format!("{bom}Нагрузка;Группа;Вид;Точки, мм;Величина\r\n");
    for l in &a.loads {
        let pts = |ps: &[V]| {
            ps.iter()
                .map(|p| format!("({} {} {})", number(p[0]), number(p[1]), number(p[2])))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let f = |v: V| format!("({} {} {})", number(v[0]), number(v[1]), number(v[2]));
        let (kind, at, value) = match &l.shape {
            open_bim_model::LoadShape::Point { at, force } => {
                ("сосредоточенная, кН", pts(&[*at]), f(*force))
            }
            open_bim_model::LoadShape::Line { a, b, start, end } => (
                "линейная, кН/м",
                pts(&[*a, *b]),
                format!("{} → {}", f(*start), f(*end)),
            ),
            open_bim_model::LoadShape::Area { polygon, pressure } => {
                ("по площади, кН/м²", pts(polygon), f(*pressure))
            }
        };
        loads.push_str(&format!("{};{};{kind};{at};{value}\r\n", l.number, l.group));
    }
    vec![
        ("узлы.csv", nodes),
        ("элементы.csv", bars),
        ("жёсткие связи.csv", links),
        ("нагрузки.csv", loads),
    ]
}
