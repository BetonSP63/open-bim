//! Fitting and cutting, construction objects and model
//! dimensions. Lines are picked in the work plane; fitting and line cuts use
//! the plane through the picked line that contains the work plane normal.
use open_bim_commands::{Command, DeletePolicy};
use open_bim_model::{
    Construction, ConstructionKind, Detail, DetailKind, DimPoint, Element, ElementShape, Frame,
    LengthMm, ModelDimension, ObjectId, Point3, ProfileShape, Project,
};
use open_bim_renderer::{DVec2, DVec3};

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

fn p3(v: DVec3) -> Point3 {
    Point3(v.to_array())
}

/// Commands in «Подгонка и резка» and «Построения».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimTool {
    ConstructionPoint,
    PointsOnLine,
    ConstructionLine,
    ConstructionCircle,
    Dimension,
    FitEnd,
    CutLine,
    CutPart,
    CutPolygon,
    EdgeChamfer,
    Split,
    Combine,
    Extend,
    Gusset,
    Stiffener,
    AddToAssembly,
    AnalysisNode,
    RigidLink,
    MergeNodes,
    PointLoad,
    LineLoad,
    AreaLoad,
    CopyAnalysis,
    PourBreak,
    PourBreakHorizontal,
    RebarLongitudinal,
    RebarStirrup,
    RebarMesh,
    RebarGuide,
    RebarGroup,
    RebarSingle,
    FormPanels,
    FormProps,
    PlaceItem,
    PlaceFamily,
    PlaceFamilyLine,
    AddedMaterial,
}

/// What a command asks for next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Part,
    Point,
    /// Polygon points until Enter / middle button (at least 3).
    Polygon,
}

impl TrimTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::ConstructionPoint => "Точка",
            Self::PointsOnLine => "Точки на линии",
            Self::ConstructionLine => "Вспомогательная линия",
            Self::ConstructionCircle => "Вспомогательная окружность",
            Self::Dimension => "Размер в модели",
            Self::FitEnd => "Подогнать торец",
            Self::CutLine => "Разрезать линией",
            Self::CutPart => "Разрезать деталью",
            Self::CutPolygon => "Разрезать многоугольником",
            Self::EdgeChamfer => "Фаска кромки",
            Self::Split => "Разделить",
            Self::Combine => "Объединить",
            Self::Extend => "Удлинить до точки",
            Self::Gusset => "Косынка",
            Self::Stiffener => "Ребро жёсткости",
            Self::AddToAssembly => "Добавить в сборку",
            Self::AnalysisNode => "Узел",
            Self::RigidLink => "Жёсткая связь",
            Self::MergeNodes => "Объединить узлы",
            Self::PointLoad => "Сосредоточенная нагрузка",
            Self::LineLoad => "Линейная нагрузка",
            Self::AreaLoad => "Нагрузка по площади",
            Self::CopyAnalysis => "Получить расчётные свойства",
            Self::PourBreak => "Шов бетонирования",
            Self::PourBreakHorizontal => "Горизонтальный шов",
            Self::RebarLongitudinal => "Продольная арматура",
            Self::RebarStirrup => "Поперечная арматура",
            Self::RebarMesh => "Арматура по грани",
            Self::RebarGuide => "Стержень по направляющим",
            Self::RebarGroup => "Группа стержней",
            Self::RebarSingle => "Один стержень",
            Self::FormPanels => "Щиты опалубки",
            Self::FormProps => "Опоры",
            Self::PlaceItem => "Элемент",
            Self::PlaceFamily | Self::PlaceFamilyLine => "Семейство",
            Self::AddedMaterial => "Добавленный материал",
        }
    }

    /// Inputs in order.
    pub fn steps(self) -> &'static [Need] {
        use Need::*;
        match self {
            Self::ConstructionPoint | Self::AnalysisNode | Self::PointLoad => &[Point],
            Self::RigidLink
            | Self::MergeNodes
            | Self::LineLoad
            | Self::PourBreak
            | Self::PourBreakHorizontal => &[Point, Point],
            Self::AreaLoad | Self::RebarGuide => &[Polygon],
            Self::RebarLongitudinal
            | Self::RebarStirrup
            | Self::RebarMesh
            | Self::FormPanels
            | Self::FormProps => &[Part],
            Self::RebarGroup => &[Point, Point, Point],
            Self::RebarSingle => &[Point, Point],
            Self::PlaceItem | Self::PlaceFamily => &[Point],
            Self::PlaceFamilyLine => &[Point, Point],
            Self::AddedMaterial => &[Part, Polygon],
            Self::CopyAnalysis => &[Part],
            Self::PointsOnLine
            | Self::ConstructionLine
            | Self::ConstructionCircle
            | Self::Dimension => &[Point, Point],
            Self::FitEnd | Self::EdgeChamfer => &[Part, Point, Point],
            Self::CutLine => &[Part, Point, Point, Point],
            Self::CutPart | Self::Combine | Self::Gusset => &[Part, Part],
            Self::AddToAssembly => &[Part],
            Self::Stiffener => &[Part, Point],
            Self::CutPolygon => &[Part, Polygon],
            Self::Split | Self::Extend => &[Part, Point],
        }
    }

    /// Prompt for step `i`.
    pub fn prompt(self, i: usize, points: usize) -> String {
        let step = match (self, i) {
            (Self::ConstructionPoint, _) => "Укажите положение точки",
            (Self::PointsOnLine, 0) => "Укажите начало делимого отрезка",
            (Self::PointsOnLine, _) => "Укажите конец делимого отрезка",
            (Self::ConstructionLine, 0) | (Self::Dimension, 0) => "Укажите первую точку",
            (Self::ConstructionLine, _) | (Self::Dimension, _) => "Укажите вторую точку",
            (Self::ConstructionCircle, 0) => "Укажите центр окружности",
            (Self::ConstructionCircle, _) => "Укажите точку на окружности (радиус)",
            (Self::FitEnd, 0) => "Выберите деталь, торец которой подгоняется",
            (Self::FitEnd, 1) => "Укажите первую точку линии подгонки",
            (Self::FitEnd, _) => "Укажите вторую точку линии подгонки",
            (Self::CutLine, 0) => "Выберите разрезаемую деталь",
            (Self::CutLine, 1) => "Укажите первую точку линии реза",
            (Self::CutLine, 2) => "Укажите вторую точку линии реза",
            (Self::CutLine, _) => "Укажите сторону, которая удаляется",
            (Self::EdgeChamfer, 0) => "Выберите деталь, кромка которой скашивается",
            (Self::EdgeChamfer, 1) => "Укажите начало кромки (угол детали)",
            (Self::EdgeChamfer, _) => "Укажите конец кромки",
            (Self::CutPart, 0) => "Выберите разрезаемую деталь",
            (Self::CutPart, _) => "Выберите режущую деталь (она остаётся)",
            (Self::CutPolygon, 0) => "Выберите разрезаемую деталь",
            (Self::CutPolygon, _) if points < 3 => "Укажите вершины выреза в рабочей плоскости",
            (Self::CutPolygon, _) => "Следующая вершина или Enter — замкнуть вырез",
            (Self::Split, 0) => "Выберите деталь для разделения",
            (Self::Split, _) => "Укажите точку разделения",
            (Self::Combine, 0) => "Выберите первую деталь",
            (Self::Combine, _) => "Выберите вторую деталь (на одной оси, тот же профиль)",
            (Self::Extend, 0) => "Выберите деталь",
            (Self::Extend, _) => "Укажите точку, до которой удлинить ближний торец",
            (Self::Gusset, 0) => "Выберите первую деталь угла (например, колонну)",
            (Self::Gusset, _) => "Выберите вторую деталь угла (например, балку)",
            (Self::Stiffener, 0) => "Выберите двутавр, в который ставится ребро",
            (Self::Stiffener, _) => "Укажите место ребра на балке со стороны стенки",
            (Self::AddToAssembly, _) => {
                "Укажите деталь сборки, в которую добавить выбранные детали"
            }
            (Self::AnalysisNode, _) => "Укажите положение узла",
            (Self::RebarLongitudinal | Self::RebarStirrup, _) => {
                "Выберите железобетонную балку или колонну"
            }
            (Self::RebarMesh, _) => "Выберите перекрытие или панель",
            (Self::PlaceItem, _) => "Укажите точку вставки элемента",
            (Self::PlaceFamily, _) => "Укажите точку вставки семейства",
            (Self::PlaceFamilyLine, 0) => "Укажите начальную точку семейства",
            (Self::PlaceFamilyLine, _) => "Укажите конечную точку семейства",
            (Self::AddedMaterial, 0) => "Выберите деталь, к которой добавить материал",
            (Self::AddedMaterial, _) => {
                "Укажите контур добавляемого материала; Enter или средняя кнопка — завершить"
            }
            (Self::FormPanels, _) => "Выберите железобетонную деталь для опалубки",
            (Self::FormProps, _) => "Выберите перекрытие или балку для опор",
            (Self::RebarGuide, _) => "Укажите точки стержня; Enter или средняя кнопка — завершить",
            (Self::RebarGroup, 0) => "Укажите начало первого стержня",
            (Self::RebarGroup, 1) => "Укажите конец первого стержня",
            (Self::RebarGroup, _) => "Укажите, до куда раскладывать стержни",
            (Self::RebarSingle, 0) => "Укажите начало стержня",
            (Self::RebarSingle, _) => "Укажите конец стержня",
            (Self::PourBreak, 0) => "Укажите начало шва бетонирования",
            (Self::PourBreak, _) => "Укажите конец шва бетонирования",
            (Self::PourBreakHorizontal, 0) => {
                "Укажите угол области шва на его отметке (низ плиты, верх колонны)"
            }
            (Self::PourBreakHorizontal, _) => "Укажите противоположный угол области в плане",
            (Self::RigidLink, 0) => "Укажите первый узел",
            (Self::RigidLink, _) => "Укажите второй узел",
            (Self::MergeNodes, 0) => "Укажите узел, который остаётся",
            (Self::MergeNodes, _) => "Укажите узел, который к нему присоединяется",
            (Self::PointLoad, _) => "Укажите точку приложения",
            (Self::LineLoad, 0) => "Укажите начало нагрузки",
            (Self::LineLoad, _) => "Укажите конец нагрузки",
            (Self::AreaLoad, _) if points < 3 => "Укажите вершины площади загружения",
            (Self::AreaLoad, _) => "Следующая вершина или Enter — замкнуть площадь",
            (Self::CopyAnalysis, _) => {
                "Укажите деталь, расчётные свойства которой получат выбранные"
            }
        };
        format!("{}: {step} (Esc — прервать)", self.label())
    }
}

/// Pick state of a fitting/cutting/construction command.
#[derive(Clone, Debug, Default)]
pub struct TrimPicker {
    pub tool: Option<TrimTool>,
    pub parts: Vec<ObjectId>,
    pub points: Vec<DVec3>,
}

impl TrimPicker {
    pub fn start(tool: TrimTool) -> Self {
        Self {
            tool: Some(tool),
            ..Default::default()
        }
    }
    fn step(&self) -> usize {
        let Some(tool) = self.tool else { return 0 };
        let parts = tool.steps().iter().filter(|n| **n == Need::Part).count();
        if self.parts.len() < parts {
            tool.steps()
                .iter()
                .position(|n| *n == Need::Part)
                .map_or(0, |first| first + self.parts.len())
        } else {
            parts + self.points.len()
        }
    }
    /// What the next viewport click provides.
    pub fn need(&self) -> Option<Need> {
        let tool = self.tool?;
        let steps = tool.steps();
        let i = self.step().min(steps.len() - 1);
        Some(steps[i])
    }
    pub fn prompt(&self) -> Option<String> {
        self.tool.map(|t| t.prompt(self.step(), self.points.len()))
    }
    pub fn cancel(&mut self) -> bool {
        let active = self.tool.is_some();
        *self = Self::default();
        active
    }
    /// All inputs are there (polygon commands finish with Enter instead).
    pub fn complete(&self) -> bool {
        let Some(tool) = self.tool else { return false };
        let parts = tool.steps().iter().filter(|n| **n == Need::Part).count();
        let points = tool.steps().iter().filter(|n| **n == Need::Point).count();
        !tool.steps().contains(&Need::Polygon)
            && self.parts.len() == parts
            && self.points.len() == points
    }
}

/// Member start, unit axis and length.
fn axis(e: &Element) -> Result<(DVec3, DVec3, f64), String> {
    match &e.shape {
        ElementShape::Member { length, frame, .. } => Ok((
            v(frame.origin),
            DVec3::from_array(frame.axes[2]),
            length.value(),
        )),
        ElementShape::Plate { .. } => Err(format!(
            "«{}»: команда работает с балками и колоннами",
            e.name
        )),
    }
}

fn element(project: &Project, id: ObjectId) -> Result<&Element, String> {
    project
        .elements
        .get(&id)
        .ok_or_else(|| "Деталь удалена".to_string())
}

/// The member with its start/end moved to axis parameters `t0`, `t1`.
fn span(e: &Element, t0: f64, t1: f64) -> Result<Element, String> {
    let (s, dir, _) = axis(e)?;
    if t1 - t0 < 1.0 {
        return Err("Длина детали стала бы меньше 1 мм".into());
    }
    let mut out = e.clone();
    if let ElementShape::Member { length, frame, .. } = &mut out.shape {
        frame.origin = p3(s + dir * t0);
        *length = LengthMm::new(t1 - t0).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Move the member end nearer to the plane (point `on`, normal `m`) onto it.
fn end_to_plane(e: &Element, on: DVec3, m: DVec3) -> Result<Element, String> {
    let (s, dir, len) = axis(e)?;
    let along = dir.dot(m);
    if along.abs() < 1e-9 {
        return Err("Линия параллельна оси детали".into());
    }
    let t = (on - s).dot(m) / along;
    let (ds, de) = (
        ((on - s).dot(m)).abs(),
        ((on - (s + dir * len)).dot(m)).abs(),
    );
    if ds <= de {
        span(e, t, len)
    } else {
        span(e, 0., t)
    }
}

fn plane_through(a: DVec3, b: DVec3, normal: DVec3) -> Result<DVec3, String> {
    let m = (b - a).cross(normal).normalize_or_zero();
    if m == DVec3::ZERO {
        return Err("Линия должна лежать в рабочей плоскости и иметь длину".into());
    }
    Ok(m)
}

/// «Подогнать торец»: the nearer end moves onto the plane through the line
/// a–b that contains the work plane normal.
pub fn fit_end(
    project: &Project,
    part: ObjectId,
    a: DVec3,
    b: DVec3,
    normal: DVec3,
) -> Result<Vec<Command>, String> {
    let e = element(project, part)?;
    let m = plane_through(a, b, normal)?;
    Ok(vec![Command::ReplaceElement {
        element: end_to_plane(e, a, m)?,
    }])
}

/// «Удлинить до точки»: the nearer end moves to the point's projection on the axis.
pub fn extend_to(project: &Project, part: ObjectId, p: DVec3) -> Result<Vec<Command>, String> {
    let e = element(project, part)?;
    let (_, dir, _) = axis(e)?;
    Ok(vec![Command::ReplaceElement {
        element: end_to_plane(e, p, dir)?,
    }])
}

/// «Разделить»: two members meeting at the point's projection on the axis.
pub fn split(project: &Project, part: ObjectId, p: DVec3) -> Result<Vec<Command>, String> {
    let e = element(project, part)?;
    let (s, dir, len) = axis(e)?;
    let t = (p - s).dot(dir);
    if t < 1.0 || t > len - 1.0 {
        return Err("Точка разделения должна лежать внутри детали".into());
    }
    let first = span(e, 0., t)?;
    let mut second = span(e, t, len)?;
    second.id = ObjectId(project.next_id);
    Ok(vec![
        Command::ReplaceElement { element: first },
        Command::Create { element: second },
    ])
}

/// «Объединить»: two collinear members of one profile become the first one.
pub fn combine(project: &Project, a: ObjectId, b: ObjectId) -> Result<Vec<Command>, String> {
    if a == b {
        return Err("Выберите две разные детали".into());
    }
    let (ea, eb) = (element(project, a)?, element(project, b)?);
    let ((sa, da, la), (sb, db, lb)) = (axis(ea)?, axis(eb)?);
    let same_profile = matches!(
        (&ea.shape, &eb.shape),
        (ElementShape::Member { profile: pa, frame: fa, .. }, ElementShape::Member { profile: pb, frame: fb, .. })
            if pa == pb && (DVec3::from_array(fa.axes[1]).dot(DVec3::from_array(fb.axes[1])).abs() > 1. - 1e-6)
    );
    if !same_profile {
        return Err("Объединяются детали одного профиля с одинаковым поворотом".into());
    }
    let off_axis = |p: DVec3| (p - sa - da * (p - sa).dot(da)).length();
    if da.dot(db).abs() < 1. - 1e-6 || off_axis(sb) > 0.5 || off_axis(sb + db * lb) > 0.5 {
        return Err("Детали должны лежать на одной оси".into());
    }
    let ts = [0., la, (sb - sa).dot(da), (sb + db * lb - sa).dot(da)];
    let (t0, t1) = (
        ts.iter().copied().fold(f64::INFINITY, f64::min),
        ts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    Ok(vec![
        Command::ReplaceElement {
            element: span(ea, t0, t1)?,
        },
        Command::Delete {
            id: b,
            policy: DeletePolicy::Cascade,
        },
    ])
}

/// World → part-local transform pieces: (R^T, origin).
fn local(frame: &Frame) -> (impl Fn(DVec3) -> DVec3 + '_, impl Fn(DVec3) -> DVec3 + '_) {
    let axes = frame.axes.map(DVec3::from_array);
    let o = v(frame.origin);
    (
        move |d: DVec3| DVec3::new(axes[0].dot(d), axes[1].dot(d), axes[2].dot(d)),
        move |p: DVec3| {
            let d = p - o;
            DVec3::new(axes[0].dot(d), axes[1].dot(d), axes[2].dot(d))
        },
    )
}

fn detail(project: &Project, kind: DetailKind) -> Vec<Command> {
    vec![Command::CreateDetail {
        detail: Detail {
            id: ObjectId(project.next_id),
            kind,
        },
    }]
}

/// «Разрезать линией»: removes the side of the plane through a–b (with the
/// work plane normal) where `side` lies.
pub fn cut_line(
    project: &Project,
    part: ObjectId,
    a: DVec3,
    b: DVec3,
    side: DVec3,
    normal: DVec3,
) -> Result<Vec<Command>, String> {
    let e = element(project, part)?;
    let mut m = plane_through(a, b, normal)?;
    let s = (side - a).dot(m);
    if s.abs() < 1e-6 {
        return Err("Точка стороны лежит на линии реза".into());
    }
    if s < 0. {
        m = -m;
    }
    let frame = e.shape.frame();
    let (dir, _) = local(frame);
    let n = dir(m);
    Ok(detail(
        project,
        DetailKind::PlaneCut {
            part,
            normal: n.to_array(),
            offset: m.dot(a - v(frame.origin)),
        },
    ))
}

/// Solid section and length of a part, as a prism in its own frame.
fn prism_of(project: &Project, e: &Element) -> Result<(Vec<[f64; 2]>, f64), String> {
    match &e.shape {
        ElementShape::Plate {
            contour, thickness, ..
        } => Ok((contour.clone(), thickness.value())),
        ElementShape::Member {
            profile, length, ..
        } => {
            let shape = &project.profiles.get(profile).ok_or("Нет профиля")?.shape;
            let contour = match *shape {
                // Hollow sections cut by their outer outline.
                ProfileShape::CircularHollow { diameter, .. } => (0..48)
                    .map(|i| {
                        let a = i as f64 / 48. * std::f64::consts::TAU;
                        [diameter / 2. * a.cos(), diameter / 2. * a.sin()]
                    })
                    .collect(),
                ProfileShape::RectangularHollow { height, width, .. } => vec![
                    [-width / 2., -height / 2.],
                    [width / 2., -height / 2.],
                    [width / 2., height / 2.],
                    [-width / 2., height / 2.],
                ],
                _ => shape.solid_contour().map_err(|e| e.to_string())?,
            };
            Ok((contour, length.value()))
        }
    }
}

/// Point inside a polygon (even-odd rule).
fn inside_polygon(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        {
            inside = !inside;
        }
    }
    inside
}

/// «Фаска кромки»: a chamfer `a` × `b` mm along a whole edge of the part's
/// prism, picked by its two ends: a lengthwise edge at a corner of the
/// section (legs along the two sides of the corner) or an edge of an end
/// face (leg `a` on the end face, `b` along the part). Made as a prism cut.
pub fn edge_chamfer(
    project: &Project,
    part: ObjectId,
    p: DVec3,
    q: DVec3,
    a: f64,
    b: f64,
) -> Result<Vec<Command>, String> {
    for leg in [a, b] {
        if !(leg.is_finite() && (0.5..=1e4).contains(&leg)) {
            return Err("Фаска: катеты 0,5…10 000 мм".into());
        }
    }
    let e = element(project, part)?;
    let (section, len) = prism_of(project, e)?;
    let n = section.len();
    let (_, point) = local(e.shape.frame());
    let (p, q) = (point(p), point(q));
    if p.distance(q) < 1. {
        return Err("Концы кромки совпадают".into());
    }
    let tol = 1.0;
    let xy = |v: DVec3| [v.x, v.y];
    let near = |u: [f64; 2], w: [f64; 2]| (u[0] - w[0]).hypot(u[1] - w[1]) < tol;
    let d2 = |u: [f64; 2]| DVec2::from_array(u);
    let margin = 10.;
    // Triangle contour (frame XY) oriented counter-clockwise.
    let ccw = |mut c: Vec<[f64; 2]>| {
        let area: f64 = (0..c.len())
            .map(|i| {
                let (u, w) = (c[i], c[(i + 1) % c.len()]);
                u[0] * w[1] - w[0] * u[1]
            })
            .sum();
        if area < 0. {
            c.reverse();
        }
        c
    };
    // Lengthwise edge: both ends at the same section corner.
    if let Some(i) = (0..n).find(|&i| near(xy(p), section[i]) && near(xy(q), section[i])) {
        let v = d2(section[i]);
        let e1 = (d2(section[(i + n - 1) % n]) - v).normalize_or_zero();
        let e2 = (d2(section[(i + 1) % n]) - v).normalize_or_zero();
        let centre = v + (e1 * a + e2 * b) / 3.;
        if e1 == DVec2::ZERO || e2 == DVec2::ZERO || !inside_polygon(centre.to_array(), &section) {
            return Err("Фаска: угол сечения вогнутый — снять нечего".into());
        }
        // The chamfer line runs past both faces so no cut face lies on
        // a face of the part.
        let (pa, pb) = (v + e1 * a, v + e2 * b);
        let run = (pa - pb).normalize_or_zero() * margin;
        let out = -(e1 + e2).normalize_or_zero() * 3. * margin;
        let contour = ccw(vec![
            (pa + run).to_array(),
            (pb - run).to_array(),
            (v + out).to_array(),
        ]);
        return Ok(detail(
            project,
            DetailKind::PrismCut {
                part,
                frame: Frame {
                    origin: Point3([0., 0., -margin]),
                    axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
                },
                contour,
                length: len + 2. * margin,
            },
        ));
    }
    // Edge of an end face: both ends at z = 0 (or the length) on one side
    // of the section.
    let z0 = [0., len]
        .into_iter()
        .find(|z| (p.z - z).abs() < tol && (q.z - z).abs() < tol)
        .ok_or("Укажите концы одной кромки детали (углы сечения или торца)")?;
    let on_side = |j: usize, u: [f64; 2]| {
        let (s, t) = (d2(section[j]), d2(section[(j + 1) % n]));
        let d = t - s;
        let k = ((d2(u) - s).dot(d) / d.length_squared()).clamp(0., 1.);
        (s + d * k).distance(d2(u)) < tol
    };
    let j = (0..n)
        .find(|&j| on_side(j, xy(p)) && on_side(j, xy(q)))
        .ok_or("Укажите концы одной кромки детали (углы сечения или торца)")?;
    let (s, t) = (d2(section[j]), d2(section[(j + 1) % n]));
    let along = (t - s).normalize_or_zero();
    let mut normal = DVec2::new(along.y, -along.x);
    let mid = (s + t) * 0.5;
    if inside_polygon((mid + normal * 0.1).to_array(), &section) {
        normal = -normal;
    }
    // Into the part along its length from this end.
    let inward = if z0 == 0. { 1. } else { -1. };
    let x = DVec3::new(normal.x, normal.y, 0.);
    let zaxis = DVec3::new(along.x, along.y, 0.);
    let y = zaxis.cross(x);
    let origin = DVec3::new(s.x, s.y, z0) - zaxis * margin;
    let to_frame = |w: DVec3| [(w - origin).dot(x), (w - origin).dot(y)];
    let corner = DVec3::new(s.x, s.y, z0) - zaxis * margin;
    let (pa, pb) = (corner - x * a, corner + DVec3::Z * inward * b);
    let run = (pa - pb).normalize() * margin;
    let contour = ccw(vec![
        to_frame(pa + run),
        to_frame(pb - run),
        to_frame(corner + (x - DVec3::Z * inward).normalize() * 3. * margin),
    ]);
    Ok(detail(
        project,
        DetailKind::PrismCut {
            part,
            frame: Frame {
                origin: p3(origin),
                axes: [x.to_array(), y.to_array(), zaxis.to_array()],
            },
            contour,
            length: s.distance(t) + 2. * margin,
        },
    ))
}

/// «Разрезать деталью»: the cutting part's solid is removed from the target;
/// the cutting part stays.
pub fn cut_part(
    project: &Project,
    target: ObjectId,
    tool: ObjectId,
) -> Result<Vec<Command>, String> {
    if target == tool {
        return Err("Деталь не режет сама себя".into());
    }
    let (t, c) = (element(project, target)?, element(project, tool)?);
    let (contour, length) = prism_of(project, c)?;
    let tf = t.shape.frame();
    let cf = c.shape.frame();
    let (dir, point) = local(tf);
    let frame = Frame {
        origin: p3(point(v(cf.origin))),
        axes: cf.axes.map(|a| dir(DVec3::from_array(a)).to_array()),
    };
    Ok(detail(
        project,
        DetailKind::PrismCut {
            part: target,
            frame,
            contour,
            length,
        },
    ))
}

/// «Разрезать многоугольником»: a polygon in the work plane, cut through the
/// whole part along the work plane normal.
pub fn cut_polygon(
    project: &Project,
    part: ObjectId,
    polygon: &[DVec3],
    normal: DVec3,
) -> Result<Vec<Command>, String> {
    if polygon.len() < 3 {
        return Err("Нужно не меньше 3 вершин".into());
    }
    let e = element(project, part)?;
    let n = normal.normalize_or_zero();
    let x = (polygon[1] - polygon[0]).reject_from(n).normalize_or_zero();
    if n == DVec3::ZERO || x == DVec3::ZERO {
        return Err("Вершины выреза совпадают".into());
    }
    let y = n.cross(x);
    let o = polygon[0];
    let contour: Vec<[f64; 2]> = polygon
        .iter()
        .map(|p| [(*p - o).dot(x), (*p - o).dot(y)])
        .collect();
    super::polygon::check_closed(&contour)?;
    // Through the whole part: from beyond its farthest point on one side to
    // beyond it on the other.
    let (section, len) = prism_of(project, e)?;
    let corners: Vec<DVec3> = section
        .iter()
        .flat_map(|c| [0., len].map(|z| v(e.shape.frame().world(Point3([c[0], c[1], z])))))
        .collect();
    let centre = corners.iter().copied().sum::<DVec3>() / corners.len() as f64;
    let size = corners
        .iter()
        .map(|c| c.distance(centre))
        .fold(0., f64::max);
    let reach = 2. * size + (o - centre).dot(n).abs() + 100.;
    let tf = e.shape.frame();
    let (dir, point) = local(tf);
    let frame = Frame {
        origin: p3(point(o - n * reach)),
        axes: [dir(x).to_array(), dir(y).to_array(), dir(n).to_array()],
    };
    Ok(detail(
        project,
        DetailKind::PrismCut {
            part,
            frame,
            contour,
            length: 2. * reach,
        },
    ))
}

/// A construction line or circle (circle in the work plane).
pub fn construction(
    project: &Project,
    tool: TrimTool,
    a: DVec3,
    b: DVec3,
    normal: DVec3,
) -> Result<Vec<Command>, String> {
    let kind = match tool {
        TrimTool::ConstructionLine => ConstructionKind::Line { a: p3(a), b: p3(b) },
        TrimTool::ConstructionCircle => ConstructionKind::Circle {
            center: p3(a),
            normal: normal.normalize_or_zero().to_array(),
            radius: (b - a).reject_from(normal).length(),
        },
        _ => return Err("Не вспомогательный объект".into()),
    };
    let construction = Construction {
        id: ObjectId(project.next_id),
        kind,
    };
    construction.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::CreateConstruction { construction }])
}

/// Construction points: one at `a`, or with `divisions` the ends and the
/// points dividing the segment `a`–`b` into equal parts.
pub fn points(
    project: &Project,
    a: DVec3,
    b: Option<DVec3>,
    divisions: usize,
) -> Result<Vec<Command>, String> {
    let at: Vec<DVec3> = match b {
        None => vec![a],
        Some(b) => {
            if a.distance(b) < 1. {
                return Err("Концы отрезка совпадают".into());
            }
            if !(1..=1000).contains(&divisions) {
                return Err("Число частей: от 1 до 1000".into());
            }
            (0..=divisions)
                .map(|k| a.lerp(b, k as f64 / divisions as f64))
                .collect()
        }
    };
    at.iter()
        .enumerate()
        .map(|(k, p)| {
            let construction = Construction {
                id: ObjectId(project.next_id + k as u64),
                kind: ConstructionKind::Point { at: p3(*p) },
            };
            construction.validate().map_err(|e| e.to_string())?;
            Ok(Command::CreateConstruction { construction })
        })
        .collect()
}

/// A model dimension; ends picked on a member end follow that member.
pub fn dimension(project: &Project, a: DVec3, b: DVec3) -> Result<Vec<Command>, String> {
    if a.distance(b) < 1. {
        return Err("Точки размера совпадают".into());
    }
    let tie = |p: DVec3| {
        project
            .elements
            .values()
            .find_map(|e| {
                let (s, dir, len) = axis(e).ok()?;
                [(false, s), (true, s + dir * len)]
                    .into_iter()
                    .find(|(_, q)| q.distance(p) < 0.5)
                    .map(|(end, _)| DimPoint::PartEnd { part: e.id, end })
            })
            .unwrap_or(DimPoint::Fixed { point: p3(p) })
    };
    Ok(vec![Command::CreateDimension {
        dimension: ModelDimension {
            id: ObjectId(project.next_id),
            a: tie(a),
            b: tie(b),
        },
    }])
}

fn plate(
    project: &Project,
    name: &str,
    role: &str,
    material: ObjectId,
    frame: Frame,
    contour: Vec<[f64; 2]>,
    thickness: f64,
) -> Result<Vec<Command>, String> {
    let element = Element {
        id: ObjectId(project.next_id),
        name: name.into(),
        material,
        shape: ElementShape::Plate {
            contour,
            thickness: LengthMm::new(thickness).map_err(|e| e.to_string())?,
            frame,
        },
        properties: [(
            "fabrication_role".to_string(),
            open_bim_model::PropertyValue::Text(role.into()),
        )]
        .into(),
    };
    let mut candidate = project.clone();
    candidate.elements.insert(element.id, element.clone());
    candidate.next_id = element.id.0 + 1;
    candidate.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::Create { element }])
}

/// Sizes of a gusset or stiffener (mm).
#[derive(Clone, Copy, Debug)]
pub struct PlatePreset {
    pub thickness: f64,
    pub material: ObjectId,
    /// Gusset legs along both members.
    pub leg: f64,
    /// Gusset corner chamfer; stiffener corner snips at the web.
    pub corner: f64,
}

/// «Косынка» between two members: a plate in the plane of both axes, in the
/// corner between them, its edges on the members' faces, legs along each
/// member and a chamfered corner.
pub fn gusset(
    project: &Project,
    first: ObjectId,
    second: ObjectId,
    p: PlatePreset,
) -> Result<Vec<Command>, String> {
    if first == second {
        return Err("Укажите две разные детали".into());
    }
    let (e1, e2) = (element(project, first)?, element(project, second)?);
    let ((s1, d1, l1), (s2, d2, l2)) = (axis(e1)?, axis(e2)?);
    let n = d1.cross(d2);
    if n.length() < 1e-6 {
        return Err("Детали параллельны: косынка ставится в угол между ними".into());
    }
    // Joint: the closest points of the two axes.
    let r = s1 - s2;
    let (b, c, f) = (d1.dot(d2), d1.dot(r), d2.dot(r));
    let den = 1. - b * b;
    let t1 = (b * f - c) / den;
    let t2 = (f - b * c) / den;
    let joint = (s1 + d1 * t1 + s2 + d2 * t2) * 0.5;
    if (s1 + d1 * t1).distance(s2 + d2 * t2) > 1. {
        return Err("Оси деталей не пересекаются".into());
    }
    // Each leg runs from the joint along its member, towards its far end.
    let away = |s: DVec3, d: DVec3, l: f64| {
        if (joint - s).length() < (joint - (s + d * l)).length() {
            d
        } else {
            -d
        }
    };
    let (u1, u2) = (away(s1, d1, l1), away(s2, d2, l2));
    let normal = u1.cross(u2).normalize();
    // Face of a member towards the other: its section extent in that direction.
    let face = |e: &Element, toward: DVec3| -> Result<f64, String> {
        let (section, _) = prism_of(project, e)?;
        let f = e.shape.frame();
        let (ax, ay) = (DVec3::from_array(f.axes[0]), DVec3::from_array(f.axes[1]));
        Ok(section
            .iter()
            .map(|q| (ax * q[0] + ay * q[1]).dot(toward))
            .fold(f64::NEG_INFINITY, f64::max))
    };
    let w1 = u2.reject_from(u1).normalize();
    let w2 = u1.reject_from(u2).normalize();
    let corner = joint + w1 * face(e1, w1)? + w2 * face(e2, w2)?;
    let x = u1;
    let y = normal.cross(x);
    let local = |q: DVec3| [(q - corner).dot(x), (q - corner).dot(y)];
    let (leg, ch) = (p.leg, p.corner.max(0.));
    if leg <= ch + 1. {
        return Err("Катет косынки должен быть больше фаски".into());
    }
    let mut contour = Vec::new();
    if ch > 0. {
        contour.push(local(corner + u1 * ch));
    } else {
        contour.push(local(corner));
    }
    contour.push(local(corner + u1 * leg));
    contour.push(local(corner + u2 * leg));
    if ch > 0. {
        contour.push(local(corner + u2 * ch));
    }
    // Centred on the plane of both axes (the webs).
    let frame = Frame {
        origin: p3(corner - normal * p.thickness / 2.),
        axes: [x.to_array(), y.to_array(), normal.to_array()],
    };
    plate(
        project,
        "Косынка",
        "gusset",
        p.material,
        frame,
        contour,
        p.thickness,
    )
}

/// «Ребро жёсткости» in an I-beam: a plate across the beam at the picked
/// point, on the picked side of the web, touching both flanges and the web,
/// with corner snips at the web–flange fillets.
pub fn stiffener(
    project: &Project,
    beam: ObjectId,
    at: DVec3,
    p: PlatePreset,
) -> Result<Vec<Command>, String> {
    let e = element(project, beam)?;
    let (s, d, len) = axis(e)?;
    let ElementShape::Member { profile, frame, .. } = &e.shape else {
        return Err("Ребро ставится в балку или колонну".into());
    };
    let ProfileShape::ISection {
        height: h,
        width: w,
        web: tw,
        flange: tf,
    } = project.profiles.get(profile).ok_or("Нет профиля")?.shape
    else {
        return Err("Ребро жёсткости ставится в двутавр (между полками)".into());
    };
    let t = (at - s).dot(d);
    if t < p.thickness || t > len - p.thickness {
        return Err("Точка ребра должна лежать в пределах балки".into());
    }
    let ax = DVec3::from_array(frame.axes[0]);
    let side = if (at - s).dot(ax) < 0. { -1. } else { 1. };
    let (inner, snip) = (h / 2. - tf, p.corner.max(0.));
    if snip >= inner || tw / 2. + snip >= w / 2. {
        return Err("Срез угла ребра больше самого ребра".into());
    }
    let x0 = side * tw / 2.;
    let x1 = side * w / 2.;
    let xs = side * (tw / 2. + snip);
    let contour = vec![
        [xs, -inner],
        [x1, -inner],
        [x1, inner],
        [xs, inner],
        [x0, inner - snip],
        [x0, -inner + snip],
    ];
    let origin = DVec3::from_array(frame.origin.0) + d * (t - p.thickness / 2.);
    let plate_frame = Frame {
        origin: p3(origin),
        axes: frame.axes,
    };
    plate(
        project,
        "Ребро жёсткости",
        "stiffener",
        p.material,
        plate_frame,
        contour,
        p.thickness,
    )
}
