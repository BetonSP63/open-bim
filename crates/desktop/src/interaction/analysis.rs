//! «Анализ»: edits of the analysis data (models, load groups, loads, nodes,
//! rigid links, merged nodes, part properties) as undoable commands, and the
//! analytical model drawn over the 3D view.
use open_bim_analysis::{Analytical, build};
use open_bim_commands::Command;
use open_bim_model::{
    Analysis, AnalysisModel, Load, LoadGroup, LoadKind, LoadShape, Members, ObjectId, PartAnalysis,
    Project, RigidLink,
};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{Mark, Projector};
use std::collections::BTreeSet;

fn set(analysis: Analysis) -> Result<Vec<Command>, String> {
    analysis.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::SetAnalysis { analysis }])
}

fn next(numbers: impl Iterator<Item = u32>) -> u32 {
    numbers.max().unwrap_or(0) + 1
}

/// The analysis model numbered `number`, else the first one, else a default
/// model of all members (not stored until edited).
pub fn model(project: &Project, number: Option<u32>) -> AnalysisModel {
    let models = &project.analysis.models;
    number
        .and_then(|n| models.iter().find(|m| m.number == n))
        .or(models.first())
        .cloned()
        .unwrap_or_else(|| default_model(1))
}

pub fn default_model(number: u32) -> AnalysisModel {
    AnalysisModel {
        number,
        name: format!("Расчётная модель {number}"),
        members: Members::All,
        merge_tolerance: 20.,
        load_groups: BTreeSet::new(),
    }
}

/// Add a model of all members; returns its number.
pub fn add_model(project: &Project) -> Result<(Vec<Command>, u32), String> {
    let mut a = project.analysis.clone();
    let n = next(a.models.iter().map(|m| m.number));
    a.models.push(default_model(n));
    Ok((set(a)?, n))
}

/// Replace a model (matched by number), adding it when new.
pub fn put_model(project: &Project, model: AnalysisModel) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    match a.models.iter_mut().find(|m| m.number == model.number) {
        Some(m) => *m = model,
        None => a.models.push(model),
    }
    set(a)
}

pub fn remove_model(project: &Project, number: u32) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    a.models.retain(|m| m.number != number);
    set(a)
}

/// A new load group of `kind` with its usual factor; returns its number.
pub fn add_group(project: &Project, kind: LoadKind) -> Result<(Vec<Command>, u32), String> {
    let mut a = project.analysis.clone();
    let n = next(a.load_groups.iter().map(|g| g.number));
    a.load_groups.push(LoadGroup {
        number: n,
        name: format!("{} {n}", kind.label()),
        kind,
        factor: kind.factor(),
    });
    Ok((set(a)?, n))
}

pub fn put_group(project: &Project, group: LoadGroup) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    match a.load_groups.iter_mut().find(|g| g.number == group.number) {
        Some(g) => *g = group,
        None => return Err("Группа нагрузок не найдена".into()),
    }
    set(a)
}

/// Remove a group with its loads.
pub fn remove_group(project: &Project, number: u32) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    a.load_groups.retain(|g| g.number != number);
    a.loads.retain(|l| l.group != number);
    for m in &mut a.models {
        m.load_groups.remove(&number);
    }
    set(a)
}

/// Add a load to `group`; the first group is made when there is none.
pub fn add_load(project: &Project, group: u32, shape: LoadShape) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    let group = if a.load_groups.iter().any(|g| g.number == group) {
        group
    } else if let Some(g) = a.load_groups.first() {
        g.number
    } else {
        a.load_groups.push(LoadGroup {
            number: 1,
            name: "Постоянная 1".into(),
            kind: LoadKind::Dead,
            factor: LoadKind::Dead.factor(),
        });
        1
    };
    let number = next(a.loads.iter().map(|l| l.number));
    a.loads.push(Load {
        number,
        group,
        shape,
    });
    set(a)
}

pub fn put_load(project: &Project, load: Load) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    match a.loads.iter_mut().find(|l| l.number == load.number) {
        Some(l) => *l = load,
        None => return Err("Нагрузка не найдена".into()),
    }
    set(a)
}

pub fn remove_load(project: &Project, number: u32) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    a.loads.retain(|l| l.number != number);
    set(a)
}

/// The analytical node nearest to `p` in `model`, within 300 mm, else `p`.
fn snap(analytical: &Analytical, p: DVec3) -> [f64; 3] {
    analytical
        .node_at(p.to_array(), 300.)
        .map_or(p.to_array(), |i| analytical.nodes[i])
}

/// «Узел»: an extra node at a point.
pub fn add_node(project: &Project, at: DVec3) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    a.nodes.push(at.to_array());
    set(a)
}

/// «Жёсткая связь» between the nodes nearest to the two points.
pub fn add_link(
    project: &Project,
    model_number: Option<u32>,
    p: DVec3,
    q: DVec3,
) -> Result<Vec<Command>, String> {
    let analytical = build(project, &model(project, model_number));
    let (a, b) = (snap(&analytical, p), snap(&analytical, q));
    if a == b {
        return Err("Жёсткая связь: укажите два разных узла".into());
    }
    let mut an = project.analysis.clone();
    let number = next(an.links.iter().map(|l| l.number));
    an.links.push(RigidLink { number, a, b });
    set(an)
}

/// «Объединить узлы»: the node at `from` moves to the node at `keep`.
pub fn merge_nodes(
    project: &Project,
    model_number: Option<u32>,
    keep: DVec3,
    from: DVec3,
) -> Result<Vec<Command>, String> {
    let analytical = build(project, &model(project, model_number));
    let (Some(k), Some(f)) = (
        analytical.node_at(keep.to_array(), 300.),
        analytical.node_at(from.to_array(), 300.),
    ) else {
        return Err("Объединить узлы: укажите точки у узлов расчётной модели".into());
    };
    if k == f {
        return Err("Объединить узлы: это один и тот же узел".into());
    }
    let mut a = project.analysis.clone();
    a.merges.push([analytical.nodes[k], analytical.nodes[f]]);
    set(a)
}

fn parts_of(project: &Project, selected: &BTreeSet<ObjectId>) -> Result<Vec<ObjectId>, String> {
    let parts: Vec<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| project.elements.contains_key(id))
        .collect();
    if parts.is_empty() {
        return Err("Выберите детали".into());
    }
    Ok(parts)
}

/// «Свойства детали в расчётах»: set the properties of the selected parts.
pub fn set_parts(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    props: &PartAnalysis,
) -> Result<Vec<Command>, String> {
    let mut a = project.analysis.clone();
    for id in parts_of(project, selected)? {
        if *props == PartAnalysis::default() {
            a.parts.remove(&id);
        } else {
            a.parts.insert(id, props.clone());
        }
    }
    set(a)
}

/// «Сбросить изменения»: the selected parts get default properties.
pub fn reset_parts(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
) -> Result<Vec<Command>, String> {
    set_parts(project, selected, &PartAnalysis::default())
}

/// «Получить расчётные свойства»: the selected parts take those of `source`.
pub fn copy_from(
    project: &Project,
    source: ObjectId,
    selected: &BTreeSet<ObjectId>,
) -> Result<Vec<Command>, String> {
    let props = project
        .analysis
        .parts
        .get(&source)
        .cloned()
        .unwrap_or_default();
    let targets: BTreeSet<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| *id != source)
        .collect();
    if targets.is_empty() {
        return Err("Сначала выберите детали, затем укажите деталь-образец".into());
    }
    set_parts(project, &targets, &props)
}

/// Write the CSV tables of a model into «Расчётная модель <имя>» next to
/// the model file; returns the folder.
pub fn export(
    project: &Project,
    model: &AnalysisModel,
    project_path: &str,
) -> Result<std::path::PathBuf, String> {
    let name: String = model
        .name
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let folder = match std::path::Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(format!("Расчётная модель {name}")),
        _ => std::path::PathBuf::from(format!("Расчётная модель {name}")),
    };
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let analytical = build(project, model);
    for (file, text) in open_bim_analysis::csv(project, &analytical) {
        std::fs::write(folder.join(file), text).map_err(|e| e.to_string())?;
    }
    Ok(folder)
}

const BAR: [u8; 4] = [200, 30, 160, 255];
const NODE: [u8; 4] = [30, 30, 30, 255];
const LINK: [u8; 4] = [20, 140, 60, 255];
const LOAD: [u8; 4] = [210, 40, 30, 255];

/// Overlay marks of the analytical model: bars, nodes, hinges, rigid links,
/// loads as arrows, and node and bar numbers when asked.
pub fn marks(
    a: &Analytical,
    p: &Projector,
    node_numbers: bool,
    bar_numbers: bool,
    span: f64,
) -> Vec<Mark> {
    let mut out = Vec::new();
    let v = |x: [f64; 3]| DVec3::from_array(x);
    let line = |a: [f64; 2], b: [f64; 2], color, width, dash| Mark::Line {
        a,
        b,
        color,
        width,
        dash,
    };
    let text = |at: [f64; 2], text: String, color, align| Mark::Text {
        at,
        text,
        color,
        align,
        size: 11.,
    };
    for (i, bar) in a.bars.iter().enumerate() {
        let (wa, wb) = (v(a.nodes[bar.nodes[0]]), v(a.nodes[bar.nodes[1]]));
        let (Some(sa), Some(sb)) = (p.screen(wa), p.screen(wb)) else {
            continue;
        };
        out.push(line(sa, sb, BAR, 2., None));
        for (k, hinge) in bar.hinges.iter().enumerate() {
            if !hinge {
                continue;
            }
            // Hinge: a small open circle just inside the end.
            let (end, other) = if k == 0 { (wa, wb) } else { (wb, wa) };
            let at = end + (other - end).normalize_or_zero() * (span * 0.012);
            if let Some(c) = p.screen(at) {
                out.push(Mark::Circle {
                    center: c,
                    radius: 4.,
                    color: BAR,
                    fill: Some([255, 255, 255, 255]),
                });
            }
        }
        if bar_numbers {
            let mid = [(sa[0] + sb[0]) / 2., (sa[1] + sb[1]) / 2.];
            out.push(text(mid, format!("{}", i + 1), BAR, [-1, 1]));
        }
    }
    for [x, y] in &a.links {
        if let (Some(sa), Some(sb)) = (p.screen(v(a.nodes[*x])), p.screen(v(a.nodes[*y]))) {
            out.push(line(sa, sb, LINK, 2., Some([6., 3., 6., 3.])));
        }
    }
    for (i, n) in a.nodes.iter().enumerate() {
        let Some(c) = p.screen(v(*n)) else { continue };
        out.push(Mark::Square {
            center: c,
            half: 3.,
            color: NODE,
            fill: true,
        });
        if node_numbers {
            out.push(text(
                [c[0] + 5., c[1] - 5.],
                format!("{}", i + 1),
                NODE,
                [-1, 1],
            ));
        }
    }
    // Loads: arrows toward the point of action, the length a share of the
    // model size.
    let arrow_len = span * 0.06;
    let mut arrow = |at: DVec3, force: [f64; 3], label: Option<String>| {
        let f = v(force);
        let dir = f.normalize_or_zero();
        if dir == DVec3::ZERO {
            return;
        }
        let tail = at - dir * arrow_len;
        let (Some(t), Some(h)) = (p.screen(tail), p.screen(at)) else {
            return;
        };
        out.push(line(t, h, LOAD, 1.6, None));
        let d = [h[0] - t[0], h[1] - t[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
        let (ux, uy) = (d[0] / len, d[1] / len);
        for s in [-1., 1.] {
            let wing = [h[0] - ux * 8. + s * uy * 4., h[1] - uy * 8. - s * ux * 4.];
            out.push(line(h, wing, LOAD, 1.6, None));
        }
        if let Some(label) = label {
            out.push(text(t, label, LOAD, [0, 1]));
        }
    };
    let size = |f: [f64; 3]| v(f).length();
    let mut edges = Vec::new();
    for l in &a.loads {
        match &l.shape {
            LoadShape::Point { at, force } => {
                arrow(v(*at), *force, Some(format!("{:.1} кН", size(*force))));
            }
            LoadShape::Line {
                a: s,
                b: e,
                start,
                end,
            } => {
                for k in 0..=4 {
                    let t = k as f64 / 4.;
                    let f = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * t);
                    let label = (k == 2).then(|| format!("{:.1} кН/м", size(f)));
                    arrow(v(*s).lerp(v(*e), t), f, label);
                }
            }
            LoadShape::Area { polygon, pressure } => {
                let pts: Vec<DVec3> = polygon.iter().map(|q| v(*q)).collect();
                let centre = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
                for (k, q) in pts.iter().enumerate() {
                    arrow(q.lerp(centre, 0.3), *pressure, None);
                    let r = pts[(k + 1) % pts.len()];
                    if let (Some(sa), Some(sb)) = (p.screen(*q), p.screen(r)) {
                        edges.push(line(sa, sb, LOAD, 1., Some([4., 3., 4., 3.])));
                    }
                }
                arrow(
                    centre,
                    *pressure,
                    Some(format!("{:.2} кН/м²", size(*pressure))),
                );
            }
        }
    }
    out.extend(edges);
    out
}
