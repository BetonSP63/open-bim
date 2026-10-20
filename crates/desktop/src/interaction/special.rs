//! «Копировать — специальное» and «Переместить — специальное»: linear
//! (N copies by a vector), rotate (N copies about an axis, with a rise dZ per
//! copy), mirror about a vertical plane, and to another plane (the selection
//! keeps its position relative to the planes). Every copy is a full copy:
//! holes, cuts, bolts and welds between copied parts come along. N copies are
//! one transaction, so one undo removes them all.
use crate::interaction::Workplane;
use crate::tools::{Edit, edit_commands, rotation};
use open_bim_commands::{Command, apply_staged};
use open_bim_model::{Frame, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

/// Upper bound on copies in one command (a typo like 1000 instead of 10
/// should not freeze the application).
pub const MAX_COPIES: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Special {
    /// Copy k goes to `delta * k`.
    Linear { delta: DVec3 },
    /// Copy k turns `degrees * k` about the axis through `pivot` and rises
    /// `dz * k` along it (spiral stairs, radial frames).
    Rotate {
        pivot: DVec3,
        axis: DVec3,
        degrees: f64,
        dz: f64,
    },
    /// Mirror about the plane through `point` containing `direction` and `up`
    /// (dialog fields: X0, Y0 and the angle of the mirror line on the work plane).
    Mirror {
        point: DVec3,
        direction: DVec3,
        up: DVec3,
    },
    /// From one plane (origin + axes) to another.
    ToPlane { from: Workplane, to: Workplane },
}

fn p(v: DVec3) -> Point3 {
    Point3(v.to_array())
}

/// Rigid motion `world = R·x + t` as a `Frame` (axes are the columns of R).
fn motion(r: [DVec3; 3], t: DVec3) -> Frame {
    Frame {
        origin: p(t),
        axes: r.map(|a| a.to_array()),
    }
}

impl Special {
    /// The edit that places copy number `k` (1-based), or moves the selection
    /// for `k = 1`.
    pub fn step(&self, k: usize) -> Result<Edit, String> {
        let k = k as f64;
        Ok(match *self {
            Self::Linear { delta } => {
                if !delta.is_finite() || delta.length() < 1e-9 {
                    return Err("Задайте ненулевое смещение".into());
                }
                Edit::Move(p(delta * k))
            }
            Self::Rotate {
                pivot,
                axis,
                degrees,
                dz,
            } => {
                let n = axis.try_normalize().ok_or("Нулевая ось поворота")?;
                if !degrees.is_finite() || !dz.is_finite() || !pivot.is_finite() {
                    return Err("Некорректный поворот".into());
                }
                if degrees.abs() < 1e-9 && dz.abs() < 1e-9 {
                    return Err("Задайте угол или подъём dZ".into());
                }
                let f = rotation(p(n), degrees * k)?;
                let r = f.axes.map(DVec3::from_array);
                let turned = r[0] * pivot.x + r[1] * pivot.y + r[2] * pivot.z;
                Edit::Transform(motion(r, pivot - turned + n * dz * k))
            }
            Self::Mirror {
                point,
                direction,
                up,
            } => {
                let normal = direction
                    .cross(up)
                    .try_normalize()
                    .ok_or("Линия зеркала вырождена")?;
                if !point.is_finite() {
                    return Err("Некорректная точка зеркала".into());
                }
                Edit::Mirror {
                    normal: p(normal),
                    offset: normal.dot(point),
                }
            }
            Self::ToPlane { from, to } => {
                // world = B·(Aᵀ·(x − a)) + b
                let a = from.axes;
                let b = to.axes;
                let col = |e: DVec3| b[0] * a[0].dot(e) + b[1] * a[1].dot(e) + b[2] * a[2].dot(e);
                let r = [col(DVec3::X), col(DVec3::Y), col(DVec3::Z)];
                let moved = r[0] * from.origin.x + r[1] * from.origin.y + r[2] * from.origin.z;
                Edit::Transform(motion(r, to.origin - moved))
            }
        })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Linear { .. } => "линейно",
            Self::Rotate { .. } => "с поворотом",
            Self::Mirror { .. } => "зеркально",
            Self::ToPlane { .. } => "в другую плоскость",
        }
    }
}

/// Commands of «Копировать — специальное» (`copies` ≥ 1) and the ids of the
/// new top-level objects (parts and grid lines).
pub fn copy_commands(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    special: &Special,
    copies: usize,
) -> Result<(Vec<Command>, BTreeSet<ObjectId>), String> {
    if selected.is_empty() {
        return Err("Выберите объекты".into());
    }
    if copies == 0 || copies > MAX_COPIES {
        return Err(format!("Количество копий: от 1 до {MAX_COPIES}"));
    }
    if matches!(special, Special::Mirror { .. } | Special::ToPlane { .. }) && copies > 1 {
        return Err("Зеркало и перенос в плоскость дают одну копию".into());
    }
    // Each copy: duplicate in place, then move the duplicates. The second
    // step is derived from the state the first one leaves, so both run on a
    // scratch copy; the document gets all commands as one transaction.
    let mut scratch = project.clone();
    let mut all = Vec::new();
    let mut created = BTreeSet::new();
    for k in 1..=copies {
        let duplicate = edit_commands(&scratch, selected, &Edit::Copy(Point3([0.; 3])))?;
        let fresh: BTreeSet<ObjectId> = duplicate
            .iter()
            .filter_map(|c| match c {
                Command::Create { element } => Some(element.id),
                Command::CreateAxis { axis } => Some(axis.id),
                _ => None,
            })
            .collect();
        apply_staged(&mut scratch, &duplicate).map_err(|e| e.to_string())?;
        let place = edit_commands(&scratch, &fresh, &special.step(k)?)?;
        apply_staged(&mut scratch, &place).map_err(|e| e.to_string())?;
        all.extend(duplicate);
        all.extend(place);
        created.extend(fresh);
    }
    Ok((all, created))
}

/// Commands of «Переместить — специальное».
pub fn move_commands(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    special: &Special,
) -> Result<Vec<Command>, String> {
    edit_commands(project, selected, &special.step(1)?)
}

/// Which dialog field a pick fills (the «Указать» buttons of the dialog).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickTarget {
    /// Two points: dX, dY, dZ of the linear copy.
    Vector,
    /// One point: the centre of rotation.
    Pivot,
    /// Two points on the work plane: the mirror line.
    MirrorLine,
    /// Three points of the source plane, then three of the target plane.
    Planes,
}

impl PickTarget {
    pub fn needs(self) -> usize {
        match self {
            Self::Vector | Self::MirrorLine => 2,
            Self::Pivot => 1,
            Self::Planes => 6,
        }
    }

    pub fn prompt(self, picked: usize) -> &'static str {
        match (self, picked) {
            (Self::Vector, 0) => "Укажите начальную точку смещения",
            (Self::Vector, _) => "Укажите конечную точку смещения",
            (Self::Pivot, _) => "Укажите центр поворота",
            (Self::MirrorLine, 0) => "Укажите первую точку линии зеркала",
            (Self::MirrorLine, _) => "Укажите вторую точку линии зеркала",
            (Self::Planes, 0) => "Исходная плоскость: укажите начало",
            (Self::Planes, 1) => "Исходная плоскость: укажите точку на оси X",
            (Self::Planes, 2) => "Исходная плоскость: укажите точку в плоскости XY",
            (Self::Planes, 3) => "Новая плоскость: укажите начало",
            (Self::Planes, 4) => "Новая плоскость: укажите точку на оси X",
            (Self::Planes, _) => "Новая плоскость: укажите точку в плоскости XY",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Linear,
    Rotate,
    Mirror,
    ToPlane,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Linear, Tab::Rotate, Tab::Mirror, Tab::ToPlane];

    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Линейно",
            Self::Rotate => "Поворот",
            Self::Mirror => "Зеркало",
            Self::ToPlane => "В другую плоскость",
        }
    }
}

/// Text fields of the dialog (kept between uses).
#[derive(Clone, Debug)]
pub struct Dialog {
    pub tab: Tab,
    pub delta: [String; 3],
    pub copies: String,
    pub pivot: [String; 3],
    /// 0 = X, 1 = Y, 2 = Z.
    pub axis: usize,
    pub angle: String,
    pub dz: String,
    /// Mirror line through (X0, Y0) at an angle to X, on the work plane.
    pub mirror_origin: [String; 2],
    pub mirror_angle: String,
    pub planes: Option<(Workplane, Workplane)>,
    /// Pick in progress for one of the «Указать» buttons.
    pub picking: Option<PickTarget>,
    pub picks: Vec<DVec3>,
}

impl Default for Dialog {
    fn default() -> Self {
        let s = |v: &str| v.to_string();
        Self {
            tab: Tab::Linear,
            delta: [s("6000"), s("0"), s("0")],
            copies: s("1"),
            pivot: [s("0"), s("0"), s("0")],
            axis: 2,
            angle: s("90"),
            dz: s("0"),
            mirror_origin: [s("0"), s("0")],
            mirror_angle: s("90"),
            planes: None,
            picking: None,
            picks: Vec::new(),
        }
    }
}

fn number(text: &str) -> Result<f64, String> {
    crate::tools::number(&text.replace(',', "."))
}

impl Dialog {
    /// The operation described by the active tab; `workplane` gives the plane
    /// of the mirror line.
    pub fn special(&self, workplane: &Workplane) -> Result<Special, String> {
        let v = |t: &[String; 3]| -> Result<DVec3, String> {
            Ok(DVec3::new(number(&t[0])?, number(&t[1])?, number(&t[2])?))
        };
        Ok(match self.tab {
            Tab::Linear => Special::Linear {
                delta: v(&self.delta)?,
            },
            Tab::Rotate => Special::Rotate {
                pivot: v(&self.pivot)?,
                axis: [DVec3::X, DVec3::Y, DVec3::Z][self.axis.min(2)],
                degrees: number(&self.angle)?,
                dz: number(&self.dz)?,
            },
            Tab::Mirror => {
                let a = number(&self.mirror_angle)?.to_radians();
                let local = DVec3::new(
                    number(&self.mirror_origin[0])?,
                    number(&self.mirror_origin[1])?,
                    0.,
                );
                Special::Mirror {
                    point: workplane.to_world(local),
                    direction: workplane.axes[0] * a.cos() + workplane.axes[1] * a.sin(),
                    up: workplane.axes[2],
                }
            }
            Tab::ToPlane => {
                let (from, to) = self
                    .planes
                    .ok_or("Укажите исходную и новую плоскости (кнопка «Указать»)")?;
                Special::ToPlane { from, to }
            }
        })
    }

    pub fn copies(&self) -> Result<usize, String> {
        match self.tab {
            Tab::Mirror | Tab::ToPlane => Ok(1),
            _ => self
                .copies
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=MAX_COPIES).contains(n))
                .ok_or(format!("Количество копий: целое от 1 до {MAX_COPIES}")),
        }
    }

    pub fn start_pick(&mut self, target: PickTarget) {
        self.picking = Some(target);
        self.picks.clear();
    }

    pub fn prompt(&self) -> Option<&'static str> {
        Some(self.picking?.prompt(self.picks.len()))
    }

    /// Accept a pick; fills the fields when the picks are complete.
    pub fn pick(&mut self, point: DVec3, workplane: &Workplane) -> Result<bool, String> {
        let target = self.picking.ok_or("Нет указания точек")?;
        self.picks.push(point);
        if self.picks.len() < target.needs() {
            return Ok(false);
        }
        let picks = std::mem::take(&mut self.picks);
        self.picking = None;
        let f = |v: f64| format!("{}", (v * 1000.).round() / 1000.);
        match target {
            PickTarget::Vector => {
                let d = picks[1] - picks[0];
                self.delta = [f(d.x), f(d.y), f(d.z)];
            }
            PickTarget::Pivot => self.pivot = picks[0].to_array().map(f),
            PickTarget::MirrorLine => {
                let (a, b) = (workplane.to_local(picks[0]), workplane.to_local(picks[1]));
                let d = b - a;
                if d.truncate().length() < 1e-6 {
                    return Err("Точки линии зеркала совпадают".into());
                }
                self.mirror_origin = [f(a.x), f(a.y)];
                self.mirror_angle = f(d.y.atan2(d.x).to_degrees());
            }
            PickTarget::Planes => {
                self.planes = Some((
                    Workplane::from_points(picks[0], picks[1], picks[2])?,
                    Workplane::from_points(picks[3], picks[4], picks[5])?,
                ));
            }
        }
        Ok(true)
    }

    pub fn cancel_pick(&mut self) -> bool {
        let was = self.picking.is_some();
        self.picking = None;
        self.picks.clear();
        was
    }
}
