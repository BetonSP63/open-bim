//! Creation tools beyond parts: grid from coordinate strings,
//! the default grid of a new model, bolts by two picks through the selected
//! parts and welds by picking the main and the secondary part.
use super::selection::corners;
use crate::tools::detailing_ui::DetailingUi;
use crate::tools::number;
use open_bim_commands::Command;
use open_bim_model::numbering_rules::{digits_along_x, level_mark, next_axis_label};
use open_bim_model::{Detail, DetailKind, Frame, GridAxis, Level, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

const MAX_LINES: usize = 500;

/// Grid X/Y string: the first value is absolute, each further value is
/// a spacing, `n*s` repeats a spacing: «0 4*6000» → 0, 6000, … 24000.
pub fn coordinates(text: &str) -> Result<Vec<f64>, String> {
    let mut values: Vec<f64> = Vec::new();
    for token in text.split_whitespace() {
        let (count, value) = match token.split_once('*') {
            Some((n, s)) => (
                n.parse::<usize>()
                    .map_err(|_| format!("Неверный множитель «{token}»"))?,
                s,
            ),
            None => (1, token),
        };
        let value = number(value).map_err(|_| format!("Неверное число «{token}»"))?;
        if count == 0 {
            return Err(format!("Нулевой множитель «{token}»"));
        }
        if values.is_empty() && count == 1 {
            values.push(value);
            continue;
        }
        if values.is_empty() {
            values.push(0.);
        }
        if value <= 0. {
            return Err(format!("Шаг должен быть положительным: «{token}»"));
        }
        if values.len() + count > MAX_LINES {
            return Err(format!("Не больше {MAX_LINES} линий"));
        }
        for _ in 0..count {
            let last = *values.last().unwrap_or(&0.);
            values.push(last + value);
        }
    }
    if values.is_empty() {
        return Err("Пустая строка координат".into());
    }
    if values.iter().any(|v| !v.is_finite() || v.abs() > 1e8) {
        return Err("Координата вне диапазона".into());
    }
    Ok(values)
}

/// Grid Z string: absolute elevations from the work plane origin
/// («0 3600 7200»), unique values.
pub fn elevations(text: &str) -> Result<Vec<f64>, String> {
    let values: Vec<f64> = text
        .split_whitespace()
        .map(|t| number(t).map_err(|_| format!("Неверная отметка «{t}»")))
        .collect::<Result<_, _>>()?;
    if values.is_empty() {
        return Err("Пустая строка отметок".into());
    }
    if values.len() > MAX_LINES || values.iter().any(|v| !v.is_finite() || v.abs() > 1e8) {
        return Err("Отметки вне диапазона".into());
    }
    let mut sorted = values.clone();
    sorted.sort_by(f64::total_cmp);
    if sorted.windows(2).any(|w| (w[1] - w[0]).abs() < 1e-6) {
        return Err("Отметки повторяются".into());
    }
    Ok(values)
}

/// Labels for `count` lines; missing labels continue the sequence
/// («А» → Б, В…; «1» → 2, 3…).
pub fn labels(text: &str, count: usize) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    if out.len() > count {
        return Err(format!("Меток {} больше, чем линий {count}", out.len()));
    }
    while out.len() < count {
        let next = out
            .last()
            .map_or(Some("1".to_string()), |l| next_axis_label(l))
            .ok_or_else(|| format!("Не продолжить метки после «{}»", out.last().unwrap()))?;
        out.push(next);
    }
    if out.iter().collect::<BTreeSet<_>>().len() != out.len() {
        return Err("Метки повторяются".into());
    }
    Ok(out)
}

/// Level names: the given ones, then the marks of the remaining elevations.
pub fn level_labels(text: &str, elevations: &[f64]) -> Result<Vec<String>, String> {
    let given: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    if given.len() > elevations.len() {
        return Err(format!(
            "Меток {} больше, чем отметок {}",
            given.len(),
            elevations.len()
        ));
    }
    let out: Vec<String> = given
        .into_iter()
        .chain(
            elevations
                .iter()
                .skip(text.split_whitespace().count())
                .map(|z| level_mark(*z)),
        )
        .collect();
    if out.iter().collect::<BTreeSet<_>>().len() != out.len() {
        return Err("Метки отметок повторяются".into());
    }
    Ok(out)
}

/// Grid dialog contents («Сетка»): X and Y lines (relative), Z levels
/// (absolute).
#[derive(Clone, Debug, PartialEq)]
pub struct GridSpec {
    pub x: String,
    pub y: String,
    pub z: String,
    pub labels_x: String,
    pub labels_y: String,
    pub labels_z: String,
    /// Line extension beyond the outermost lines, mm.
    pub extension: String,
}

impl Default for GridSpec {
    fn default() -> Self {
        Self {
            x: "0 4*6000".into(),
            y: "0 3*6000".into(),
            z: "0 3600".into(),
            labels_x: "1 2 3 4 5".into(),
            labels_y: "А Б В Г".into(),
            labels_z: "0,000 +3,600".into(),
            extension: "2000".into(),
        }
    }
}

/// One transaction: X lines (parallel to Y, labelled 1…), Y lines (parallel
/// to X, labelled А…) at the lowest level, and the levels.
pub fn grid_commands(project: &Project, spec: &GridSpec) -> Result<Vec<Command>, String> {
    let (xs, ys, zs) = (
        coordinates(&spec.x)?,
        coordinates(&spec.y)?,
        elevations(&spec.z)?,
    );
    // Without labels, digits go along the side with more axes (ГОСТ Р
    // 21.101-2026, 5.3.2); missing level names are the marks («+3,600»).
    let (auto_x, auto_y) = if spec.labels_x.trim().is_empty() && spec.labels_y.trim().is_empty() {
        if digits_along_x(xs.len(), ys.len()) {
            ("1", "А")
        } else {
            ("А", "1")
        }
    } else {
        (spec.labels_x.as_str(), spec.labels_y.as_str())
    };
    let (lx, ly, lz) = (
        labels(auto_x, xs.len())?,
        labels(auto_y, ys.len())?,
        level_labels(&spec.labels_z, &zs)?,
    );
    let ext = number(&spec.extension)?;
    if !(0.0..=1e6).contains(&ext) {
        return Err("Удлинение линий вне диапазона".into());
    }
    let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let z0 = min(&zs);
    let mut next = project.next_id;
    let mut id = || {
        let v = ObjectId(next);
        next += 1;
        v
    };
    let mut commands = Vec::new();
    for (x, name) in xs.iter().zip(lx) {
        commands.push(Command::CreateAxis {
            axis: GridAxis {
                id: id(),
                name,
                start: Point3([*x, min(&ys) - ext, z0]),
                end: Point3([*x, max(&ys) + ext, z0]),
            },
        });
    }
    for (y, name) in ys.iter().zip(ly) {
        commands.push(Command::CreateAxis {
            axis: GridAxis {
                id: id(),
                name,
                start: Point3([min(&xs) - ext, *y, z0]),
                end: Point3([max(&xs) + ext, *y, z0]),
            },
        });
    }
    for (z, name) in zs.iter().zip(lz) {
        commands.push(Command::CreateLevel {
            level: Level {
                id: id(),
                name,
                elevation_mm: *z,
            },
        });
    }
    Ok(commands)
}

/// A new model with the usual defaults: steel С255 and common ГОСТ
/// sections, default grid 1–5 × А–Г at 6000 mm, levels 0 and +3600, work
/// plane at level 0.
pub fn default_project() -> Result<Project, String> {
    let mut project = Project::default();
    gost_starter(&mut project)?;
    with_default_grid(project)
}

/// `project` with the default grid and levels added, as one undo step that
/// is not counted as a revision.
pub fn with_default_grid(project: Project) -> Result<Project, String> {
    let commands = grid_commands(&project, &GridSpec::default())?;
    let mut document = open_bim_commands::Document::new(project).map_err(|e| e.to_string())?;
    document
        .transact("Сетка по умолчанию", &commands)
        .map_err(|e| e.to_string())?;
    let mut project = document.project().clone();
    project.revision = 0;
    Ok(project)
}

/// Sections of a new model, under the ids the tools start with (material 1,
/// profile 2 for beams and columns): one of each common family.
pub const STARTER_SECTIONS: [&str; 5] = [
    "30Б1 (Р 57837)",
    "L100×8",
    "[20П",
    "Труба 114×4,5",
    "Гн 150×100×6",
];

/// Replace the built-in test material and sections of `Project::default`
/// with С255 (ГОСТ 27772) and [`STARTER_SECTIONS`]; the bar keeps its id 7.
fn gost_starter(project: &mut Project) -> Result<(), String> {
    use open_bim_model::library;
    let steel = library::steel(ObjectId(1), "С255").ok_or("Нет стали С255 в каталоге")?;
    project.materials = [(steel.id, steel)].into();
    for (i, name) in STARTER_SECTIONS.iter().enumerate() {
        let id = ObjectId(i as u64 + 2);
        let entry =
            library::section(name).ok_or_else(|| format!("Нет профиля {name} в каталоге"))?;
        project.profiles.insert(id, entry.profile(id));
    }
    if let Some(bar) = project.profiles.get_mut(&ObjectId(7)) {
        bar.name = "Брус 100×50".into();
        bar.source = "Прямоугольное сечение без стандарта".into();
    }
    Ok(())
}

/// Bolt group through `parts` («by parts and points»): the first
/// point is the group origin, the second sets its X direction; the bolt axis
/// is `axis` (work plane normal) and spans all parts, so every part gets a
/// through hole. Spacings, shift and rotation come from the bolt properties.
pub fn bolt_pattern(
    project: &Project,
    parts: &BTreeSet<ObjectId>,
    a: DVec3,
    b: DVec3,
    axis: DVec3,
    props: &DetailingUi,
) -> Result<open_bim_steel::BoltPattern, String> {
    if parts.is_empty() {
        return Err("Сначала выберите соединяемые детали".into());
    }
    let z = axis.try_normalize().ok_or("Неверная ось болта")?;
    let x = (b - a - z * (b - a).dot(z))
        .try_normalize()
        .ok_or("Вторая точка задаёт направление ряда: укажите другую точку")?;
    let y = z.cross(x);
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for part in parts {
        for c in corners(project, *part)? {
            let t = (c - a).dot(z);
            low = low.min(t);
            high = high.max(t);
        }
    }
    let (mut steps_x, mut steps_y) = (
        crate::tools::detailing_ui::spacings(&props.steps_x)?,
        crate::tools::detailing_ui::spacings(&props.steps_y)?,
    );
    if props.rotate {
        std::mem::swap(&mut steps_x, &mut steps_y);
    }
    let origin = a + z * low + x * number(&props.shift_x)? + y * number(&props.shift_y)?;
    Ok(open_bim_steel::BoltPattern {
        parts: parts.clone(),
        frame: Frame {
            origin: Point3(origin.to_array()),
            axes: [x.to_array(), y.to_array(), z.to_array()],
        },
        diameter: number(&props.diameter)?,
        clearance: number(&props.clearance)?,
        hole_depth: high - low,
        steps_x,
        steps_y,
        standard: props.standard.clone(),
        grade: props.grade.clone(),
    })
}

/// Bolt axes of a pattern (start and end of each bolt), for the preview.
pub fn bolt_axes(pattern: &open_bim_steel::BoltPattern) -> Vec<(DVec3, DVec3)> {
    let f = &pattern.frame;
    let prefix = |s: &[f64]| {
        let mut out = vec![0.];
        for v in s {
            out.push(out.last().copied().unwrap_or(0.) + v);
        }
        out
    };
    let mut out = Vec::new();
    for x in prefix(&pattern.steps_x) {
        for y in prefix(&pattern.steps_y) {
            let a = DVec3::from_array(f.world(Point3([x, y, 0.])).0);
            let b = DVec3::from_array(f.world(Point3([x, y, pattern.hole_depth])).0);
            out.push((a, b));
        }
    }
    out
}

pub fn bolt_by_picks(
    project: &Project,
    parts: &BTreeSet<ObjectId>,
    a: DVec3,
    b: DVec3,
    axis: DVec3,
    props: &DetailingUi,
) -> Result<Vec<Command>, String> {
    let pattern = bolt_pattern(project, parts, a, b, axis, props)?;
    open_bim_steel::bolt_commands(project, &pattern).map_err(|e| e.to_string())
}

/// Weld of `secondary` to `main`: the path is the secondary part's edge
/// nearest to the main part (two closest distinct solid corners).
pub fn weld_between(
    project: &Project,
    main: ObjectId,
    secondary: ObjectId,
    props: &DetailingUi,
) -> Result<Vec<Command>, String> {
    if main == secondary {
        return Err("Присоединяемая деталь должна отличаться от основной".into());
    }
    let main_corners = corners(project, main)?;
    let centre = main_corners.iter().copied().sum::<DVec3>() / main_corners.len() as f64;
    let mut c = corners(project, secondary)?;
    c.sort_by(|p, q| p.distance(centre).total_cmp(&q.distance(centre)));
    let first = c[0];
    let second = *c
        .iter()
        .find(|p| p.distance(first) > 1.)
        .ok_or("Нельзя определить линию шва")?;
    let mut candidate = project.clone();
    let id = candidate.allocate_id().map_err(|e| e.to_string())?;
    let detail = Detail {
        id,
        kind: DetailKind::Weld {
            parts: BTreeSet::from([main, secondary]),
            weld_type: props.weld_type,
            size: number(&props.weld_size)?,
            path: vec![Point3(first.to_array()), Point3(second.to_array())],
            shop: props.shop,
            sides: props.weld_sides,
            around: props.weld_around,
            process: props.weld_process,
        },
    };
    candidate.details.insert(id, detail.clone());
    candidate.validate().map_err(|e| e.to_string())?;
    let mut commands = vec![Command::CreateDetail { detail }];
    // A shop weld makes one assembly of the two parts.
    if props.shop {
        commands.extend(super::assembly_edit::join(&candidate, main, secondary));
    }
    Ok(commands)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTool {
    Bolt,
    Weld,
}

/// Pick state of the bolt and weld commands. Bolt («by parts and
/// points»): main part, secondary parts, middle click, origin, X direction.
/// Weld: main part, then secondary part.
#[derive(Clone, Debug, Default)]
pub struct DetailPicker {
    pub tool: Option<DetailTool>,
    pub parts: BTreeSet<ObjectId>,
    /// Bolt: part selection finished, points are picked next.
    pub parts_done: bool,
    pub points: Vec<DVec3>,
    pub main: Option<ObjectId>,
}

impl DetailPicker {
    pub fn start(tool: DetailTool) -> Self {
        Self {
            tool: Some(tool),
            ..Default::default()
        }
    }
    /// Viewport clicks pick parts (not points) in this state.
    pub fn picks_objects(&self) -> bool {
        match self.tool {
            Some(DetailTool::Weld) => true,
            Some(DetailTool::Bolt) => !self.parts_done,
            None => false,
        }
    }
    pub fn picks_points(&self) -> bool {
        self.tool == Some(DetailTool::Bolt) && self.parts_done
    }
    pub fn prompt(&self) -> Option<String> {
        Some(
            match (
                self.tool?,
                self.parts_done,
                self.parts.len(),
                self.points.len(),
                self.main,
            ) {
                (DetailTool::Bolt, false, 0, _, _) => {
                    "Выберите основную деталь болтового соединения"
                }
                (DetailTool::Bolt, false, _, _, _) => {
                    "Выберите присоединяемые детали (средняя кнопка или Enter — завершить выбор)"
                }
                (DetailTool::Bolt, true, _, 0, _) => "Укажите начало болтовой группы",
                (DetailTool::Bolt, true, _, _, _) => "Укажите направление оси X болтовой группы",
                (DetailTool::Weld, _, _, _, None) => "Выберите деталь, к которой приваривают",
                (DetailTool::Weld, _, _, _, Some(_)) => "Выберите привариваемую деталь",
            }
            .into(),
        )
    }
    /// Enter / middle click: finish the bolt part selection.
    pub fn finish_parts(&mut self) -> Result<(), String> {
        if self.tool != Some(DetailTool::Bolt) || self.parts_done {
            return Err("Нечего завершать".into());
        }
        if self.parts.is_empty() {
            return Err("Выберите хотя бы одну деталь".into());
        }
        self.parts_done = true;
        Ok(())
    }
    pub fn cancel(&mut self) -> bool {
        let was = self.tool.is_some();
        *self = Self::default();
        was
    }
}
