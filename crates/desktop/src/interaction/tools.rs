//! Tekla-style creation tools beyond parts: grid from coordinate strings,
//! the default grid of a new model, bolts by two picks through the selected
//! parts and welds by picking the main and the secondary part.
use super::selection::corners;
use crate::tools::detailing_ui::DetailingUi;
use crate::tools::number;
use open_bim_commands::Command;
use open_bim_model::{
    Detail, DetailKind, Frame, GridAxis, Level, ObjectId, Point3, Project, WeldType,
};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

const MAX_LINES: usize = 500;

/// Tekla grid X/Y string: the first value is absolute, each further value is
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

/// Tekla grid Z string: absolute elevations from the work plane origin
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

/// Grid letters per ГОСТ Р 21.101-2026 5.3.2 (no Е, Ё, З, Й, О, Х, Ц, Ч, Щ, Ъ, Ы, Ь).
const CYRILLIC: &str = "АБВГДЖИКЛМНПРСТУФШЭЮЯ";
const LATIN: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";

fn next_label(label: &str) -> Option<String> {
    if let Ok(n) = label.parse::<i64>() {
        return Some((n + 1).to_string());
    }
    let mut chars = label.chars();
    let (c, rest) = (chars.next()?, chars.as_str());
    if !rest.is_empty() {
        return None;
    }
    for alphabet in [CYRILLIC, LATIN] {
        let letters: Vec<char> = alphabet.chars().collect();
        if let Some(i) = letters.iter().position(|l| *l == c) {
            return letters.get(i + 1).map(|l| l.to_string());
        }
    }
    None
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
            .map_or(Some("1".to_string()), |l| next_label(l))
            .ok_or_else(|| format!("Не продолжить метки после «{}»", out.last().unwrap()))?;
        out.push(next);
    }
    if out.iter().collect::<BTreeSet<_>>().len() != out.len() {
        return Err("Метки повторяются".into());
    }
    Ok(out)
}

/// Grid dialog contents (Tekla «Сетка»): X and Y lines (relative), Z levels
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
            labels_z: "0.000 +3.600".into(),
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
    let (lx, ly, lz) = (
        labels(&spec.labels_x, xs.len())?,
        labels(&spec.labels_y, ys.len())?,
        labels(&spec.labels_z, zs.len())?,
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

/// A new model like a new Tekla model: default grid 1–5 × А–Г at 6000 mm,
/// levels 0 and +3600, work plane at level 0.
pub fn default_project() -> Result<Project, String> {
    let project = Project::default();
    let commands = grid_commands(&project, &GridSpec::default())?;
    let mut document = open_bim_commands::Document::new(project).map_err(|e| e.to_string())?;
    document
        .transact("Сетка по умолчанию", &commands)
        .map_err(|e| e.to_string())?;
    let mut project = document.project().clone();
    project.revision = 0;
    Ok(project)
}

/// Bolt group through `parts`: rows start at `a` and run towards `b`; the bolt
/// axis is `axis` (work plane normal) and spans all parts, so every part gets
/// a through hole. Spacing, diameter and standard come from the bolt properties.
pub fn bolt_by_picks(
    project: &Project,
    parts: &BTreeSet<ObjectId>,
    a: DVec3,
    b: DVec3,
    axis: DVec3,
    props: &DetailingUi,
) -> Result<Vec<Command>, String> {
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
    let steps = |s: &str| -> Result<Vec<f64>, String> {
        s.split([';', ' '])
            .filter(|t| !t.trim().is_empty())
            .map(number)
            .collect()
    };
    let origin = a + z * low;
    open_bim_steel::bolt_commands(
        project,
        &open_bim_steel::BoltPattern {
            parts: parts.clone(),
            frame: Frame {
                origin: Point3(origin.to_array()),
                axes: [x.to_array(), y.to_array(), z.to_array()],
            },
            diameter: number(&props.diameter)?,
            clearance: number(&props.clearance)?,
            hole_depth: high - low,
            steps_x: steps(&props.steps_x)?,
            steps_y: steps(&props.steps_y)?,
            standard: props.standard.clone(),
            grade: props.grade.clone(),
        },
    )
    .map_err(|e| e.to_string())
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
            weld_type: if props.butt {
                WeldType::Butt
            } else {
                WeldType::Fillet
            },
            size: number(&props.weld_size)?,
            path: vec![Point3(first.to_array()), Point3(second.to_array())],
            shop: props.shop,
        },
    };
    candidate.details.insert(id, detail.clone());
    candidate.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::CreateDetail { detail }])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTool {
    Bolt,
    Weld,
}

/// Pick state of the bolt and weld commands. Bolt (Tekla «by parts and
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
