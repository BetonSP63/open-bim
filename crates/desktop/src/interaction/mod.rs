//! Pick-command engine (Tekla-style): a command declares its picks and a
//! Russian prompt per step; picks are projected onto the work plane; the
//! object is created after the last pick and the command re-arms until Esc.
pub mod numeric;
pub mod selection;
pub mod snap;
pub mod tools;
pub mod workplane;

use crate::tools::ToolKind;
use open_bim_commands::Command;
use open_bim_model::{
    Element, ElementShape, Frame, GridAxis, LengthMm, MemberKind, ObjectId, Point3, Project,
    PropertyValue,
};
use open_bim_renderer::DVec3;
pub use workplane::Workplane;

/// Pick sequence of a command: fixed count, or open polygon (min picks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picks {
    Exactly(usize),
    AtLeast(usize),
}

pub fn picks(kind: ToolKind) -> Picks {
    match kind {
        ToolKind::Column => Picks::Exactly(1),
        ToolKind::Beam | ToolKind::Axis => Picks::Exactly(2),
        ToolKind::Plate | ToolKind::Gusset | ToolKind::Stiffener => Picks::AtLeast(3),
    }
}

/// Prompt for the next pick (status bar text).
pub fn prompt(kind: ToolKind, picked: usize) -> String {
    let object = match kind {
        ToolKind::Beam => "балки",
        ToolKind::Column => "колонны",
        ToolKind::Axis => "оси",
        ToolKind::Plate => "пластины",
        ToolKind::Gusset => "косынки",
        ToolKind::Stiffener => "ребра жёсткости",
    };
    match (picks(kind), picked) {
        (Picks::Exactly(1), _) => format!("Укажите положение {object}"),
        (Picks::Exactly(_), 0) => format!("Укажите начальную точку {object}"),
        (Picks::Exactly(_), _) => format!("Укажите конечную точку {object}"),
        (Picks::AtLeast(_), 0) => format!("Укажите первую точку контура {object}"),
        (Picks::AtLeast(n), p) if p < n => format!("Укажите следующую точку контура {object}"),
        (Picks::AtLeast(_), _) => {
            format!(
                "Укажите следующую точку контура {object} (Enter или средняя кнопка — завершить)"
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Pick accepted; more picks are needed.
    Continue,
    /// The command has all picks: create the object, then the command re-arms.
    Complete {
        kind: ToolKind,
        points: Vec<DVec3>,
    },
    Cancelled,
    Ignored(String),
}

#[derive(Clone, Debug, Default)]
pub struct Engine {
    pub tool: Option<ToolKind>,
    pub picks: Vec<DVec3>,
    pub last: Option<ToolKind>,
    pub workplane: Workplane,
    /// Text being typed as a numeric location, if the input box is open.
    pub numeric: Option<String>,
    pub error: Option<String>,
    /// Temporary snap reference point (Tekla Ctrl+pick): numeric input,
    /// ortho and perpendicular snapping work from it until the next pick.
    pub reference: Option<DVec3>,
}

impl Engine {
    pub fn start(&mut self, kind: ToolKind) {
        self.tool = Some(kind);
        self.picks.clear();
        self.numeric = None;
        self.error = None;
        self.last = Some(kind);
    }

    pub fn active(&self) -> bool {
        self.tool.is_some()
    }

    pub fn prompt(&self) -> String {
        match self.tool {
            Some(kind) => prompt(kind, self.picks.len()),
            None => "Выберите команду на ленте или объект в модели".into(),
        }
    }

    /// Esc: drop the current picks and leave the command.
    pub fn cancel(&mut self) -> Outcome {
        let was = self.tool.take().is_some();
        self.picks.clear();
        self.numeric = None;
        if was {
            Outcome::Cancelled
        } else {
            Outcome::Ignored("Нет активной команды".into())
        }
    }

    /// Enter / middle click: finish an open polygon, or repeat the last command.
    pub fn finish(&mut self) -> Outcome {
        match self.tool {
            Some(kind) => match picks(kind) {
                Picks::AtLeast(n) if self.picks.len() >= n => self.complete(kind),
                Picks::AtLeast(n) => Outcome::Ignored(format!("Нужно не меньше {n} точек")),
                Picks::Exactly(_) => Outcome::Ignored("Укажите оставшиеся точки".into()),
            },
            None => match self.last {
                Some(kind) => {
                    self.start(kind);
                    Outcome::Continue
                }
                None => Outcome::Ignored("Нет команды для повтора".into()),
            },
        }
    }

    fn complete(&mut self, kind: ToolKind) -> Outcome {
        let points = std::mem::take(&mut self.picks);
        Outcome::Complete { kind, points }
    }

    /// Accept a world point (already snapped); it is projected onto the work plane.
    pub fn pick(&mut self, point: DVec3) -> Outcome {
        let Some(kind) = self.tool else {
            return Outcome::Ignored("Нет активной команды".into());
        };
        if !point.is_finite() {
            return Outcome::Ignored("Точка вне диапазона".into());
        }
        // Repeated identical pick is ignored (double click on the same spot).
        if self.picks.last().is_some_and(|p| p.distance(point) < 1e-6) {
            return Outcome::Ignored("Точка совпадает с предыдущей".into());
        }
        self.picks.push(point);
        self.reference = None;
        match picks(kind) {
            Picks::Exactly(n) if self.picks.len() >= n => self.complete(kind),
            _ => Outcome::Continue,
        }
    }

    /// Ray pick: intersect the work plane.
    pub fn pick_ray(&mut self, origin: DVec3, direction: DVec3) -> Outcome {
        match self.workplane.intersect(origin, direction) {
            Some(p) => self.pick(p),
            None => Outcome::Ignored("Луч не пересекает рабочую плоскость".into()),
        }
    }

    /// Typed coordinates (see `numeric::parse`); errors keep the command alive.
    pub fn pick_numeric(&mut self, text: &str, cursor: Option<DVec3>) -> Outcome {
        let previous = self.reference.or(self.picks.last().copied());
        let direction = match (previous, cursor) {
            (Some(p), Some(c)) => Some(c - p),
            _ => None,
        };
        match numeric::parse(text, &self.workplane, previous, direction) {
            Ok(p) => {
                self.error = None;
                self.numeric = None;
                self.pick(p)
            }
            Err(e) => {
                self.error = Some(e.clone());
                Outcome::Ignored(e)
            }
        }
    }
}

/// Model commands for a completed pick sequence, using the property defaults
/// held in `draft` (name, profile, material, thickness, column height).
pub fn commands(
    project: &Project,
    kind: ToolKind,
    points: &[DVec3],
    workplane: &Workplane,
    draft: &crate::tools::Draft,
) -> Result<Vec<Command>, String> {
    let id = ObjectId(project.next_id);
    let p = |v: DVec3| Point3(v.to_array());
    let number = |s: &str| crate::tools::number(s);
    match kind {
        ToolKind::Axis => Ok(vec![Command::CreateAxis {
            axis: GridAxis {
                id,
                name: draft.name.clone(),
                start: p(points[0]),
                end: p(points[1]),
            },
        }]),
        ToolKind::Beam | ToolKind::Column => {
            let (start, end) = if kind == ToolKind::Column {
                // Tekla: the pick gives X/Y; bottom and top are global Z values.
                let (bottom, top) = (number(&draft.bottom)?, number(&draft.top)?);
                if top <= bottom {
                    return Err("Верх колонны должен быть выше низа".into());
                }
                (
                    DVec3::new(points[0].x, points[0].y, bottom),
                    DVec3::new(points[0].x, points[0].y, top),
                )
            } else {
                (points[0], points[1])
            };
            let (frame, length) = Frame::along(p(start), p(end)).map_err(|e| e.to_string())?;
            Ok(vec![Command::Create {
                element: Element {
                    id,
                    name: draft.name.clone(),
                    material: draft.material,
                    shape: ElementShape::Member {
                        member_kind: if kind == ToolKind::Beam {
                            MemberKind::Beam
                        } else {
                            MemberKind::Column
                        },
                        profile: draft.profile,
                        length,
                        frame,
                    },
                    properties: Default::default(),
                },
            }])
        }
        ToolKind::Plate | ToolKind::Gusset | ToolKind::Stiffener => {
            // Contour in work plane coordinates relative to the first pick.
            let first = workplane.to_local(points[0]);
            let mut contour: Vec<[f64; 2]> = points
                .iter()
                .map(|q| {
                    let l = workplane.to_local(*q) - first;
                    [l.x, l.y]
                })
                .collect();
            let signed: f64 = (0..contour.len())
                .map(|i| {
                    let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum();
            // Counter-clockwise, keeping the first pick as vertex 0 (start handle).
            if signed < 0. {
                contour[1..].reverse();
            }
            open_bim_model::polygon_area(&contour).map_err(|e| e.to_string())?;
            let role = match kind {
                ToolKind::Gusset => "gusset",
                ToolKind::Stiffener => "stiffener",
                _ => "plate",
            };
            Ok(vec![Command::Create {
                element: Element {
                    id,
                    name: draft.name.clone(),
                    material: draft.material,
                    shape: ElementShape::Plate {
                        contour,
                        thickness: LengthMm::new(number(&draft.thickness)?)
                            .map_err(|e| e.to_string())?,
                        frame: Frame {
                            origin: p(points[0]),
                            axes: workplane.axes.map(|a| a.to_array()),
                        },
                    },
                    properties: std::collections::BTreeMap::from([(
                        "fabrication_role".into(),
                        PropertyValue::Text(role.into()),
                    )]),
                },
            }])
        }
    }
}
