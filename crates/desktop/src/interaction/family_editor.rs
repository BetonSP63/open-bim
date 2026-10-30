//! «Редактор семейств»: parameters, type sizes, reference planes and bodies
//! of a family in one window, with views from the front, the top and the
//! side that redraw on every change, and «Проверить», which builds every
//! type size and reports what fails.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    Axis, BodyArray, BodyShape, Family, FamilyBody, FamilyParameter, FamilyType, LocalShape,
    ParamKind, Placement, Point3, RefPlane, SpecMode,
};
use std::collections::BTreeMap;

/// A new family to start from: one plate on its own parameters.
pub fn blank(name: &str) -> Family {
    let s = |v: &str| v.to_string();
    let length = |name: &str, label: &str, value: &str| FamilyParameter {
        name: s(name),
        label: s(label),
        kind: ParamKind::Length,
        group: s("Размеры"),
        of_type: true,
        value: s(value),
        formula: String::new(),
        min: Some(1.),
        max: None,
    };
    Family {
        name: s(name),
        nested: Vec::new(),
        description: String::new(),
        placement: Placement::Point,
        spec: SpecMode::Assembly,
        parameters: vec![
            FamilyParameter {
                name: s("Материал"),
                label: s("Сталь"),
                kind: ParamKind::Material,
                group: s("Материалы"),
                of_type: true,
                value: s("С255"),
                formula: String::new(),
                min: None,
                max: None,
            },
            length("B", "Ширина", "200"),
            length("H", "Высота", "300"),
            length("t", "Толщина", "10"),
        ],
        types: vec![FamilyType {
            name: s("Тип 1"),
            values: BTreeMap::new(),
        }],
        planes: vec![
            RefPlane {
                name: s("Право"),
                axis: Axis::X,
                at: s("X0 + B"),
            },
            RefPlane {
                name: s("Верх"),
                axis: Axis::Z,
                at: s("Z0 + H"),
            },
        ],
        bodies: vec![FamilyBody {
            name: s("Пластина"),
            material: s("Материал"),
            visible: String::new(),
            array: None,
            shape: BodyShape::Plate {
                normal: Axis::Y,
                at: s("Y0"),
                contour: vec![
                    [s("X0"), s("Z0")],
                    [s("Право"), s("Z0")],
                    [s("Право"), s("Верх")],
                    [s("X0"), s("Верх")],
                ],
                thickness: s("t"),
            },
        }],
    }
}

/// Every type size (or the defaults without types) built once: the
/// problems found, empty when the family is sound.
pub fn check(family: &Family, length: f64) -> Vec<String> {
    if let Err(e) = family.validate() {
        return vec![e.to_string()];
    }
    let mut names: Vec<&str> = family.types.iter().map(|t| t.name.as_str()).collect();
    if names.is_empty() {
        names.push("");
    }
    let mut out = Vec::new();
    for name in names {
        let label = if name.is_empty() {
            "По умолчанию"
        } else {
            name
        };
        let built = family
            .values(name, &BTreeMap::new(), length, &BTreeMap::new())
            .and_then(|v| family.evaluate(&v));
        if let Err(e) = built {
            out.push(format!("{label}: {e}"));
        }
    }
    out
}

/// A view of the family: which local axes run right and up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Front,
    Top,
    Side,
}

impl View {
    pub const ALL: [Self; 3] = [Self::Front, Self::Top, Self::Side];
    pub fn label(self) -> &'static str {
        match self {
            Self::Front => "Спереди",
            Self::Top => "Сверху",
            Self::Side => "Сбоку",
        }
    }
    fn axes(self) -> (usize, usize) {
        match self {
            Self::Front => (0, 2),
            Self::Top => (0, 1),
            Self::Side => (1, 2),
        }
    }
}

/// What a view draws, in mm of the view: part edges and the reference
/// planes seen edge-on (name, axis is the view's right (`true`) or up,
/// position).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sketch {
    pub edges: Vec<[[f64; 2]; 2]>,
    pub planes: Vec<(String, bool, f64)>,
}

fn edges_3d(shape: &LocalShape) -> Vec<[[f64; 3]; 2]> {
    match shape {
        LocalShape::Plate {
            contour,
            thickness,
            frame,
        } => {
            let at = |p: [f64; 2], z: f64| frame.world(Point3([p[0], p[1], z])).0;
            let n = contour.len();
            let mut out = Vec::new();
            for i in 0..n {
                let (a, b) = (contour[i], contour[(i + 1) % n]);
                out.push([at(a, 0.), at(b, 0.)]);
                out.push([at(a, *thickness), at(b, *thickness)]);
                out.push([at(a, 0.), at(a, *thickness)]);
            }
            out
        }
        LocalShape::Member { start, end, .. } => vec![[start.0, end.0]],
    }
}

/// The view of `family` of `type_name` (placed with length `length`).
pub fn sketch(family: &Family, type_name: &str, length: f64, view: View) -> Result<Sketch, String> {
    let values = family.values(type_name, &BTreeMap::new(), length, &BTreeMap::new())?;
    let parts = family.evaluate(&values)?;
    let (u, v) = view.axes();
    let mut out = Sketch::default();
    for part in &parts {
        for [a, b] in edges_3d(&part.shape) {
            let (p, q) = ([a[u], a[v]], [b[u], b[v]]);
            if (p[0] - q[0]).abs() + (p[1] - q[1]).abs() > 1e-9 {
                out.edges.push([p, q]);
            }
        }
    }
    let index = |a: Axis| match a {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    };
    for plane in &family.planes {
        let k = index(plane.axis);
        if k == u || k == v {
            out.planes
                .push((plane.name.clone(), k == u, values.numbers[&plane.name]));
        }
    }
    Ok(out)
}

/// `formula` as `head ± tail`, split at its last `+` or `-` outside
/// brackets that is not a sign (the minus of `B * -2`).
fn last_term(formula: &str) -> Option<(&str, f64, &str)> {
    let mut depth = 0i32;
    let mut prev: Option<char> = None;
    let mut found = None;
    for (i, c) in formula.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '+' | '-'
                if depth == 0 && prev.is_some_and(|p| p.is_alphanumeric() || "_.)".contains(p)) =>
            {
                found = Some((i, if c == '+' { 1. } else { -1. }));
            }
            _ => {}
        }
        if !c.is_whitespace() {
            prev = Some(c);
        }
    }
    let (i, sign) = found?;
    Some((formula[..i].trim_end(), sign, formula[i + 1..].trim()))
}

/// `v` written briefly: whole millimetres without a fraction, at most
/// three decimals.
fn number(v: f64) -> String {
    let v = (v * 1000.).round() / 1000.;
    format!("{}", if v == 0. { 0. } else { v })
}

/// `head ± v` with the sign chosen by `v`.
fn plus(head: &str, v: f64) -> String {
    if v < 0. {
        format!("{head} - {}", number(-v))
    } else {
        format!("{head} + {}", number(v))
    }
}

/// Move plane `name` of `family` to `target` (mm along its axis) as the
/// views of type `type_name` and length `length` show it, the way an
/// engineer would write it: a plain number is replaced, a number at the
/// end of the formula («Низ + 150») changes, a length parameter at the end
/// («X0 + B») takes the new size (in the type size when the parameter is a
/// type one), and otherwise the formula gets an offset («… + 50»). Every
/// variant is built and kept only if the plane lands at `target` and the
/// bodies still build. Returns the edited family and what changed.
pub fn drag_plane(
    family: &Family,
    type_name: &str,
    length: f64,
    name: &str,
    target: f64,
) -> Result<(Family, String), String> {
    let none = BTreeMap::new();
    let position = |f: &Family| -> Result<f64, String> {
        let values = f.values(type_name, &none, length, &BTreeMap::new())?;
        f.evaluate(&values)?;
        Ok(values.numbers[name])
    };
    let index = family
        .planes
        .iter()
        .position(|p| p.name == name)
        .ok_or_else(|| format!("Нет плоскости «{name}»"))?;
    let delta = target - position(family)?;
    let formula = family.planes[index].at.trim().to_string();
    let with_formula = |at: String| {
        let mut f = family.clone();
        f.planes[index].at = at;
        let note = format!("{name} = {}", f.planes[index].at);
        (f, note)
    };
    let mut variants = Vec::new();
    if formula.replace(',', ".").parse::<f64>().is_ok() {
        variants.push(with_formula(number(target)));
    }
    if let Some((head, sign, tail)) = last_term(&formula) {
        if let Ok(v) = tail.parse::<f64>() {
            variants.push(with_formula(plus(head, sign * v + delta)));
        } else if let Some(p) = family
            .parameter(tail)
            .filter(|p| p.kind == ParamKind::Length && p.formula.is_empty())
        {
            let in_type = p.of_type && !type_name.is_empty();
            let current = if in_type {
                family
                    .family_type(type_name)
                    .and_then(|t| t.values.get(tail))
                    .unwrap_or(&p.value)
            } else {
                &p.value
            };
            if let Ok(v) = current.trim().replace(',', ".").parse::<f64>() {
                let value = number(v + sign * delta);
                let mut f = family.clone();
                if in_type {
                    let t = f.types.iter_mut().find(|t| t.name == type_name);
                    t.expect("type checked above")
                        .values
                        .insert(tail.to_string(), value.clone());
                } else if let Some(p) = f.parameters.iter_mut().find(|p| p.name == tail) {
                    p.value = value.clone();
                }
                let note = if in_type {
                    format!("{tail} = {value} в типоразмере «{type_name}»")
                } else {
                    format!("{tail} = {value}")
                };
                variants.push((f, note));
            }
        }
    }
    if delta.abs() > 1e-9 {
        variants.push(with_formula(plus(&formula, delta)));
        variants.push(with_formula(plus(&format!("({formula})"), delta)));
    }
    let mut last = None;
    for (f, note) in variants {
        match position(&f) {
            Ok(v) if (v - target).abs() < 1e-3 => return Ok((f, note)),
            Ok(_) => {}
            Err(e) => last = Some(e),
        }
    }
    if delta.abs() <= 1e-9 {
        return Ok((family.clone(), format!("{name} = {formula}")));
    }
    Err(last.unwrap_or_else(|| format!("Плоскость {name} не сдвигается")))
}

/// What the editor asks for.
pub enum EditorAction {
    /// Store the family in the project (its objects rebuild).
    Commit(String, Vec<Command>),
    /// Save the family to the families folder.
    Save(Family),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    General,
    Parameters,
    Types,
    Planes,
    Bodies,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Parameters,
        Self::Types,
        Self::Planes,
        Self::Bodies,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::General => "Общие",
            Self::Parameters => "Параметры",
            Self::Types => "Типоразмеры",
            Self::Planes => "Плоскости",
            Self::Bodies => "Тела",
        }
    }
}

pub struct FamilyEditor {
    pub open: bool,
    pub family: Family,
    page: Page,
    body: usize,
    /// Type size and `L` the views are drawn for.
    view_type: String,
    view_length: String,
    problems: Option<Vec<String>>,
    /// Result of the last CSV import or export.
    note: Option<String>,
    /// The plane being dragged in a view.
    drag: Option<Drag>,
    /// What the last drag changed (or why it stopped).
    drag_note: Option<String>,
    /// Where each view was drawn in the last frame.
    fits: Vec<(View, Fit)>,
}

/// A plane dragged with the mouse: the family as it was when the drag
/// began (every move is applied to it afresh, Esc returns it) and the
/// view's scale, kept so that the view does not refit under the mouse.
struct Drag {
    view: View,
    plane: String,
    start: Family,
    fit: Fit,
}

/// How a view maps its mm to the screen.
#[derive(Clone, Copy, Debug)]
struct Fit {
    rect: egui::Rect,
    scale: f64,
    centre: [f64; 2],
}

impl Fit {
    /// `s` fitted into `rect`, y up; `None` when there is nothing to draw.
    fn new(rect: egui::Rect, s: &Sketch) -> Option<Self> {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for p in s.edges.iter().flatten() {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        for (_, right, at) in &s.planes {
            let k = usize::from(!*right);
            lo[k] = lo[k].min(*at);
            hi[k] = hi[k].max(*at);
        }
        if !lo[0].is_finite() || !lo[1].is_finite() {
            return None;
        }
        let pad = 16.;
        let w = (hi[0] - lo[0]).max(1.);
        let h = (hi[1] - lo[1]).max(1.);
        let scale =
            ((rect.width() - 2. * pad) as f64 / w).min((rect.height() - 2. * pad) as f64 / h);
        Some(Self {
            rect,
            scale,
            centre: [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.],
        })
    }
    fn screen(&self, p: [f64; 2]) -> egui::Pos2 {
        egui::pos2(
            self.rect.center().x + ((p[0] - self.centre[0]) * self.scale) as f32,
            self.rect.center().y - ((p[1] - self.centre[1]) * self.scale) as f32,
        )
    }
    fn model(&self, p: egui::Pos2) -> [f64; 2] {
        [
            self.centre[0] + (p.x - self.rect.center().x) as f64 / self.scale,
            self.centre[1] - (p.y - self.rect.center().y) as f64 / self.scale,
        ]
    }
    /// Screen distance from `p` to a plane seen edge-on.
    fn distance(&self, p: egui::Pos2, right: bool, at: f64) -> f32 {
        let q = self.screen([at, at]);
        if right {
            (p.x - q.x).abs()
        } else {
            (p.y - q.y).abs()
        }
    }
    /// Rounding of dragged positions: the smallest of 1, 5, 10, 50… mm
    /// that is at least 4 px on the screen.
    fn step(&self) -> f64 {
        [1., 5., 10., 50., 100., 500., 1000.]
            .into_iter()
            .find(|s| s * self.scale >= 4.)
            .unwrap_or(5000.)
    }
}

/// The plane of `s` nearest to `p` within grabbing distance.
fn plane_near<'a>(fit: &Fit, s: &'a Sketch, p: egui::Pos2) -> Option<&'a (String, bool, f64)> {
    s.planes
        .iter()
        .map(|plane| (fit.distance(p, plane.1, plane.2), plane))
        .filter(|(d, _)| *d <= 5.)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, plane)| plane)
}

impl Default for FamilyEditor {
    fn default() -> Self {
        Self {
            open: false,
            family: blank("Новое семейство"),
            page: Page::default(),
            body: 0,
            view_type: String::new(),
            view_length: "3000".into(),
            problems: None,
            note: None,
            drag: None,
            drag_note: None,
            fits: Vec::new(),
        }
    }
}

fn text(ui: &mut egui::Ui, value: &mut String, width: f32) -> bool {
    ui.add(egui::TextEdit::singleline(value).desired_width(width))
        .changed()
}

fn axis_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, axis: &mut Axis) {
    egui::ComboBox::from_id_salt(id)
        .width(48.)
        .selected_text(format!("{axis:?}"))
        .show_ui(ui, |ui| {
            for a in Axis::ALL {
                ui.selectable_value(axis, a, format!("{a:?}"));
            }
        });
}

fn optional(ui: &mut egui::Ui, value: &mut Option<f64>) {
    let mut s = value.map(|v| v.to_string()).unwrap_or_default();
    if text(ui, &mut s, 50.) {
        *value = s.trim().replace(',', ".").parse().ok();
    }
}

/// Up, down and delete buttons of row `i` of `rows`.
fn row_buttons<T>(ui: &mut egui::Ui, rows: &mut Vec<T>, i: usize) -> bool {
    if ui.small_button("↑").clicked() && i > 0 {
        rows.swap(i, i - 1);
        return true;
    }
    if ui.small_button("↓").clicked() && i + 1 < rows.len() {
        rows.swap(i, i + 1);
        return true;
    }
    if ui.small_button("✕").clicked() {
        rows.remove(i);
        return true;
    }
    false
}

impl FamilyEditor {
    /// Edit a copy of `family`.
    pub fn edit(&mut self, family: Family) {
        self.view_type = family
            .types
            .first()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        self.family = family;
        self.body = 0;
        self.problems = None;
        self.drag = None;
        self.drag_note = None;
        self.open = true;
    }

    /// Where plane `name` crosses the middle of `view` on the screen, as
    /// drawn in the last frame.
    pub fn plane_on_screen(&self, view: View, name: &str) -> Option<egui::Pos2> {
        let (_, fit) = self.fits.iter().find(|(v, _)| *v == view)?;
        let length = self.view_length.trim().parse().unwrap_or(3000.);
        let s = sketch(&self.family, &self.view_type, length, view).ok()?;
        let (_, right, at) = s.planes.iter().find(|p| p.0 == name)?;
        let q = fit.screen([*at, *at]);
        let c = fit.rect.center();
        Some(if *right {
            egui::pos2(q.x, c.y)
        } else {
            egui::pos2(c.x, q.y)
        })
    }

    /// `library`: families that can be nested («+ вложенное»).
    pub fn show(&mut self, ctx: &egui::Context, library: &[Family]) -> Option<EditorAction> {
        if !self.open {
            return None;
        }
        let mut action = None;
        let mut open = true;
        egui::Window::new("Редактор семейств")
            .id(egui::Id::new("family-editor"))
            .open(&mut open)
            .default_size([900., 600.])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for p in Page::ALL {
                        ui.selectable_value(&mut self.page, p, p.label());
                    }
                });
                ui.separator();
                ui.columns(2, |cols| {
                    egui::ScrollArea::both()
                        .id_salt("family-editor-page")
                        .max_height(440.)
                        .show(&mut cols[0], |ui| match self.page {
                            Page::General => self.general(ui),
                            Page::Parameters => self.parameters(ui),
                            Page::Types => self.types(ui),
                            Page::Planes => self.planes(ui),
                            Page::Bodies => self.bodies(ui, library),
                        });
                    self.views(&mut cols[1]);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Проверить").clicked() {
                        let length = self.view_length.trim().parse().unwrap_or(3000.);
                        self.problems = Some(check(&self.family, length));
                    }
                    if ui.button("Сохранить в папку").clicked() {
                        action = Some(EditorAction::Save(self.family.clone()));
                    }
                    if ui
                        .button("Сохранить в проект")
                        .on_hover_text("Объекты этого семейства перестроятся")
                        .clicked()
                    {
                        action = Some(EditorAction::Commit(
                            format!("Семейство «{}»", self.family.name),
                            vec![Command::SetFamily {
                                family: Box::new(self.family.clone()),
                            }],
                        ));
                    }
                });
                match &self.problems {
                    Some(p) if p.is_empty() => {
                        ui.colored_label(
                            egui::Color32::from_rgb(40, 140, 60),
                            "Все типоразмеры строятся",
                        );
                    }
                    Some(p) => {
                        for e in p {
                            ui.colored_label(egui::Color32::from_rgb(190, 60, 40), e);
                        }
                    }
                    None => {}
                }
            });
        self.open = open;
        action
    }

    fn general(&mut self, ui: &mut egui::Ui) {
        let f = &mut self.family;
        egui::Grid::new("family-general")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Имя");
                text(ui, &mut f.name, 260.);
                ui.end_row();
                ui.label("Описание");
                ui.add(egui::TextEdit::multiline(&mut f.description).desired_width(260.));
                ui.end_row();
                ui.label("Размещение");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.placement, Placement::Point, "в точке");
                    ui.radio_value(&mut f.placement, Placement::TwoPoints, "по двум точкам (L)");
                });
                ui.end_row();
                ui.label("В ведомостях");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.spec, SpecMode::Assembly, "сборка");
                    ui.radio_value(&mut f.spec, SpecMode::Item, "изделие");
                });
                ui.end_row();
            });
        ui.label(
            "Базовые плоскости X0, Y0, Z0 проходят через точку вставки. \
             У семейства по двум точкам ось X идёт от первой точки ко второй, \
             L — расстояние между ними.",
        );
    }

    fn parameters(&mut self, ui: &mut egui::Ui) {
        let rows = &mut self.family.parameters;
        egui::Grid::new("family-parameters")
            .num_columns(10)
            .striped(true)
            .show(ui, |ui| {
                for h in [
                    "Имя",
                    "Подпись",
                    "Вид",
                    "Группа",
                    "Тип",
                    "Значение",
                    "Формула",
                    "Мин",
                    "Макс",
                    "",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                let mut i = 0;
                while i < rows.len() {
                    let p = &mut rows[i];
                    text(ui, &mut p.name, 70.);
                    text(ui, &mut p.label, 110.);
                    egui::ComboBox::from_id_salt(("param-kind", i))
                        .width(90.)
                        .selected_text(p.kind.label())
                        .show_ui(ui, |ui| {
                            for k in ParamKind::ALL {
                                ui.selectable_value(&mut p.kind, k, k.label());
                            }
                        });
                    text(ui, &mut p.group, 70.);
                    ui.checkbox(&mut p.of_type, "")
                        .on_hover_text("Параметр типа: значение из типоразмера");
                    text(ui, &mut p.value, 70.);
                    text(ui, &mut p.formula, 110.);
                    optional(ui, &mut p.min);
                    optional(ui, &mut p.max);
                    let changed = ui.horizontal(|ui| row_buttons(ui, rows, i)).inner;
                    ui.end_row();
                    if !changed {
                        i += 1;
                    }
                }
            });
        if ui.button("Добавить параметр").clicked() {
            let n = (1..)
                .map(|k| format!("p{k}"))
                .find(|n| self.family.parameter(n).is_none())
                .unwrap_or_default();
            self.family.parameters.push(FamilyParameter {
                name: n,
                label: String::new(),
                kind: ParamKind::Length,
                group: "Размеры".into(),
                of_type: false,
                value: "100".into(),
                formula: String::new(),
                min: None,
                max: None,
            });
        }
        ui.label("Формула параметра видит базовые плоскости, L и параметры выше.");
    }

    fn types(&mut self, ui: &mut egui::Ui) {
        let f = &mut self.family;
        let columns: Vec<(String, String)> = f
            .parameters
            .iter()
            .filter(|p| p.of_type)
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect();
        egui::Grid::new("family-types")
            .num_columns(columns.len() + 2)
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Типоразмер");
                for (name, _) in &columns {
                    ui.strong(name);
                }
                ui.label("");
                ui.end_row();
                let mut i = 0;
                while i < f.types.len() {
                    let t = &mut f.types[i];
                    text(ui, &mut t.name, 70.);
                    for (name, default) in &columns {
                        let mut v = t.values.get(name).cloned().unwrap_or_default();
                        let edit = egui::TextEdit::singleline(&mut v)
                            .hint_text(default.as_str())
                            .desired_width(60.);
                        if ui.add(edit).changed() {
                            if v.trim().is_empty() {
                                t.values.remove(name);
                            } else {
                                t.values.insert(name.clone(), v);
                            }
                        }
                    }
                    let changed = ui.horizontal(|ui| row_buttons(ui, &mut f.types, i)).inner;
                    ui.end_row();
                    if !changed {
                        i += 1;
                    }
                }
            });
        // Values of parameters no longer of the type would fail validation.
        for t in &mut f.types {
            t.values
                .retain(|k, _| columns.iter().any(|(name, _)| name == k));
        }
        if ui.button("Добавить типоразмер").clicked() {
            let base = f.types.last().cloned();
            let name = (f.types.len() + 1..)
                .map(|k| format!("Тип {k}"))
                .find(|n| f.family_type(n).is_none())
                .unwrap_or_default();
            f.types.push(FamilyType {
                name,
                values: base.map(|t| t.values).unwrap_or_default(),
            });
        }
        ui.label("Пустая ячейка — значение параметра по умолчанию (серым).");
        ui.horizontal(|ui| {
            if ui.button("Экспорт CSV").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title("Типоразмеры в CSV")
                    .set_file_name(format!("{}.csv", f.name))
                    .add_filter("CSV", &["csv"])
                    .save_file()
            {
                self.note = Some(match std::fs::write(&path, f.types_csv()) {
                    Ok(()) => format!("Типоразмеры записаны: {}", path.display()),
                    Err(e) => format!("Не удалось записать: {e}"),
                });
            }
            if ui.button("Импорт CSV").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title("Типоразмеры из CSV")
                    .add_filter("CSV", &["csv", "txt"])
                    .pick_file()
            {
                self.note = Some(
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|csv| f.read_types_csv(&csv))
                    {
                        Ok(()) => format!("Типоразмеров: {}", f.types.len()),
                        Err(e) => format!("Не удалось прочитать CSV: {e}"),
                    },
                );
            }
        });
        if let Some(n) = &self.note {
            ui.label(n);
        }
    }

    fn planes(&mut self, ui: &mut egui::Ui) {
        let rows = &mut self.family.planes;
        egui::Grid::new("family-planes")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                for h in ["Имя", "Поперёк оси", "Положение", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                let mut i = 0;
                while i < rows.len() {
                    let p = &mut rows[i];
                    text(ui, &mut p.name, 90.);
                    axis_combo(ui, ("plane-axis", i), &mut p.axis);
                    text(ui, &mut p.at, 180.);
                    let changed = ui.horizontal(|ui| row_buttons(ui, rows, i)).inner;
                    ui.end_row();
                    if !changed {
                        i += 1;
                    }
                }
            });
        if ui.button("Добавить плоскость").clicked() {
            let n = (1..)
                .map(|k| format!("Плоскость{k}"))
                .find(|n| !rows.iter().any(|p| &p.name == n))
                .unwrap_or_default();
            rows.push(RefPlane {
                name: n,
                axis: Axis::X,
                at: "X0 + 100".into(),
            });
        }
        ui.label(
            "Положение — формула: «Низ + H» ставит плоскость на H выше плоскости «Низ». \
             Тело стоит на плоскости, когда называет её в координатах.",
        );
    }

    fn bodies(&mut self, ui: &mut egui::Ui, library: &[Family]) {
        let f = &mut self.family;
        ui.horizontal_wrapped(|ui| {
            for (i, b) in f.bodies.iter().enumerate() {
                ui.selectable_value(&mut self.body, i, &b.name);
            }
            if ui.button("+ пластина").clicked() {
                f.bodies.push(FamilyBody {
                    name: format!("Пластина {}", f.bodies.len() + 1),
                    material: "С255".into(),
                    visible: String::new(),
                    array: None,
                    shape: BodyShape::Plate {
                        normal: Axis::Z,
                        at: "Z0".into(),
                        contour: [["0", "0"], ["100", "0"], ["100", "100"], ["0", "100"]]
                            .map(|[a, b]| [a.to_string(), b.to_string()])
                            .to_vec(),
                        thickness: "10".into(),
                    },
                });
                self.body = f.bodies.len() - 1;
            }
            if ui.button("+ стержень").clicked() {
                f.bodies.push(FamilyBody {
                    name: format!("Стержень {}", f.bodies.len() + 1),
                    material: "С255".into(),
                    visible: String::new(),
                    array: None,
                    shape: BodyShape::Member {
                        profile: "L50×5".into(),
                        start: ["X0", "Y0", "Z0"].map(String::from),
                        end: ["X0 + 1000", "Y0", "Z0"].map(String::from),
                        rotation: String::new(),
                    },
                });
                self.body = f.bodies.len() - 1;
            }
            let mut pick = None;
            egui::ComboBox::from_id_salt("family-nest")
                .selected_text("+ вложенное")
                .show_ui(ui, |ui| {
                    for g in library.iter().filter(|g| g.name != f.name) {
                        if ui.selectable_label(false, &g.name).clicked() {
                            pick = Some(g.clone());
                        }
                    }
                });
            if let Some(g) = pick {
                let type_name = g.types.first().map(|t| t.name.clone()).unwrap_or_default();
                f.bodies.push(FamilyBody {
                    name: g.name.clone(),
                    material: "С255".into(),
                    visible: String::new(),
                    array: None,
                    shape: BodyShape::Nested {
                        family: g.name.clone(),
                        type_name,
                        at: ["X0", "Y0", "Z0"].map(String::from),
                        rotation: String::new(),
                        values: BTreeMap::new(),
                    },
                });
                if f.nested_family(&g.name).is_none() {
                    f.nested.push(g);
                }
                self.body = f.bodies.len() - 1;
            }
        });
        if f.bodies.is_empty() {
            return;
        }
        self.body = self.body.min(f.bodies.len() - 1);
        let i = self.body;
        ui.separator();
        let nested = f.nested.clone();
        let b = &mut f.bodies[i];
        egui::Grid::new("family-body")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Имя");
                text(ui, &mut b.name, 200.);
                ui.end_row();
                ui.label("Материал");
                text(ui, &mut b.material, 200.);
                ui.end_row();
                ui.label("Видимость");
                ui.add(
                    egui::TextEdit::singleline(&mut b.visible)
                        .hint_text("всегда; например «n > 0»")
                        .desired_width(200.),
                );
                ui.end_row();
                let mut arrayed = b.array.is_some();
                ui.label("Массив");
                if ui.checkbox(&mut arrayed, "").changed() {
                    b.array = arrayed.then(|| BodyArray {
                        count: "2".into(),
                        step: ["100", "0", "0"].map(String::from),
                    });
                }
                ui.end_row();
                if let Some(a) = &mut b.array {
                    ui.label("Число копий");
                    text(ui, &mut a.count, 200.);
                    ui.end_row();
                    ui.label("Шаг x, y, z");
                    ui.horizontal(|ui| {
                        for s in &mut a.step {
                            text(ui, s, 90.);
                        }
                    });
                    ui.end_row();
                }
                match &mut b.shape {
                    BodyShape::Plate {
                        normal,
                        at,
                        contour,
                        thickness,
                    } => {
                        ui.label("Поперёк оси");
                        axis_combo(ui, "body-normal", normal);
                        ui.end_row();
                        ui.label("Грань на");
                        text(ui, at, 200.);
                        ui.end_row();
                        ui.label("Толщина");
                        text(ui, thickness, 200.);
                        ui.end_row();
                        let (a, c) = match normal {
                            Axis::X => ("y", "z"),
                            Axis::Y => ("x", "z"),
                            Axis::Z => ("x", "y"),
                        };
                        ui.label(format!("Контур ({a}, {c})"));
                        ui.vertical(|ui| {
                            let mut k = 0;
                            while k < contour.len() {
                                let changed = ui
                                    .horizontal(|ui| {
                                        ui.label(format!("{}", k + 1));
                                        text(ui, &mut contour[k][0], 110.);
                                        text(ui, &mut contour[k][1], 110.);
                                        contour.len() > 3 && row_buttons(ui, contour, k)
                                    })
                                    .inner;
                                if !changed {
                                    k += 1;
                                }
                            }
                            if ui.small_button("+ точка").clicked() {
                                let last = contour.last().cloned().unwrap_or_default();
                                contour.push(last);
                            }
                        });
                        ui.end_row();
                    }
                    BodyShape::Member {
                        profile,
                        start,
                        end,
                        rotation,
                    } => {
                        ui.label("Профиль");
                        text(ui, profile, 200.);
                        ui.end_row();
                        for (label, p) in [("Начало x, y, z", start), ("Конец x, y, z", end)]
                        {
                            ui.label(label);
                            ui.horizontal(|ui| {
                                for s in p.iter_mut() {
                                    text(ui, s, 90.);
                                }
                            });
                            ui.end_row();
                        }
                        ui.label("Поворот, °");
                        text(ui, rotation, 200.);
                        ui.end_row();
                    }
                    BodyShape::Nested {
                        family,
                        type_name,
                        at,
                        rotation,
                        values,
                    } => {
                        let child = nested.iter().find(|n| n.name == *family);
                        ui.label("Семейство");
                        ui.label(family.as_str());
                        ui.end_row();
                        if let Some(child) = child.filter(|c| !c.types.is_empty()) {
                            ui.label("Типоразмер");
                            egui::ComboBox::from_id_salt("nested-type")
                                .selected_text(type_name.as_str())
                                .show_ui(ui, |ui| {
                                    for t in &child.types {
                                        ui.selectable_value(type_name, t.name.clone(), &t.name);
                                    }
                                });
                            ui.end_row();
                        }
                        ui.label("Точка вставки x, y, z");
                        ui.horizontal(|ui| {
                            for s in at.iter_mut() {
                                text(ui, s, 90.);
                            }
                        });
                        ui.end_row();
                        ui.label("Поворот вокруг Z, °");
                        text(ui, rotation, 200.);
                        ui.end_row();
                        let names: Vec<(String, String)> = child
                            .map(|c| {
                                let mut v: Vec<(String, String)> = c
                                    .parameters
                                    .iter()
                                    .filter(|p| p.formula.is_empty())
                                    .map(|p| (p.name.clone(), p.value.clone()))
                                    .collect();
                                if c.placement == Placement::TwoPoints {
                                    v.push(("L".into(), "0".into()));
                                }
                                v
                            })
                            .unwrap_or_default();
                        for (name, default) in names {
                            ui.label(format!("= {name}"));
                            let mut v = values.get(&name).cloned().unwrap_or_default();
                            let edit = egui::TextEdit::singleline(&mut v)
                                .hint_text(default.as_str())
                                .desired_width(200.);
                            if ui.add(edit).changed() {
                                if v.trim().is_empty() {
                                    values.remove(&name);
                                } else {
                                    values.insert(name, v);
                                }
                            }
                            ui.end_row();
                        }
                    }
                }
            });
        let mut remove = false;
        ui.horizontal(|ui| {
            if ui.button("Копировать тело").clicked() {
                let mut copy = f.bodies[i].clone();
                copy.name.push_str(" (копия)");
                f.bodies.insert(i + 1, copy);
                self.body = i + 1;
            }
            remove = ui.button("Удалить тело").clicked();
        });
        if remove {
            f.bodies.remove(i);
            // A nested family no body uses any more leaves the definition.
            let used: Vec<String> = f
                .bodies
                .iter()
                .filter_map(|b| match &b.shape {
                    BodyShape::Nested { family, .. } => Some(family.clone()),
                    _ => None,
                })
                .collect();
            f.nested.retain(|n| used.contains(&n.name));
            self.body = i.saturating_sub(1);
        }
        ui.label(
            "Материал — марка стали или параметр вида «Материал»; профиль — имя из \
             каталога или параметр вида «Профиль».",
        );
    }

    fn views(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let f = &self.family;
            if !f.types.is_empty() {
                if f.family_type(&self.view_type).is_none() {
                    self.view_type = f.types[0].name.clone();
                }
                egui::ComboBox::from_id_salt("family-view-type")
                    .selected_text(self.view_type.as_str())
                    .show_ui(ui, |ui| {
                        for t in &f.types {
                            ui.selectable_value(&mut self.view_type, t.name.clone(), &t.name);
                        }
                    });
            } else {
                self.view_type.clear();
            }
            if f.placement == Placement::TwoPoints {
                ui.label("L");
                text(ui, &mut self.view_length, 60.);
            }
        });
        let length = self.view_length.trim().parse().unwrap_or(3000.);
        if self.drag.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let d = self.drag.take().expect("checked above");
            self.family = d.start;
            self.drag_note = Some(format!("Плоскость {} на прежнем месте", d.plane));
        }
        self.fits.clear();
        for view in View::ALL {
            ui.label(view.label());
            let size = egui::vec2(ui.available_width(), 130.);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 2., ui.visuals().extreme_bg_color);
            let s = match sketch(&self.family, &self.view_type, length, view) {
                Ok(s) => s,
                Err(e) => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        e,
                        egui::FontId::proportional(11.),
                        egui::Color32::from_rgb(190, 60, 40),
                    );
                    continue;
                }
            };
            let fit = match &self.drag {
                Some(d) if d.view == view => Some(d.fit),
                _ => Fit::new(rect, &s),
            };
            let Some(fit) = fit else { continue };
            self.fits.push((view, fit));
            let pointer = response.interact_pointer_pos().or(response.hover_pos());
            let near = pointer.and_then(|p| plane_near(&fit, &s, p)).cloned();
            if response.drag_started() {
                // The plane under the press, not where the mouse is now.
                let origin = ui.input(|i| i.pointer.press_origin());
                let grabbed = origin.and_then(|p| plane_near(&fit, &s, p));
                if let Some((name, ..)) = grabbed {
                    self.drag = Some(Drag {
                        view,
                        plane: name.clone(),
                        start: self.family.clone(),
                        fit,
                    });
                }
            }
            let mut hot = near.as_ref().map(|p| p.0.clone());
            if let (Some(d), Some(p)) = (&self.drag, pointer)
                && d.view == view
            {
                hot = Some(d.plane.clone());
                let right = s.planes.iter().find(|q| q.0 == d.plane).map(|q| q.1);
                if let (true, Some(right)) = (response.dragged(), right) {
                    let m = fit.model(p);
                    let raw = if right { m[0] } else { m[1] };
                    let step = fit.step();
                    let target = (raw / step).round() * step;
                    match drag_plane(&d.start, &self.view_type, length, &d.plane, target) {
                        Ok((f, note)) => {
                            self.family = f;
                            self.drag_note = Some(note);
                        }
                        Err(e) => self.drag_note = Some(e),
                    }
                }
            }
            if response.drag_stopped() {
                self.drag = None;
            }
            if let Some((_, right, _)) = &near {
                ui.ctx().set_cursor_icon(if *right {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                });
            }
            // Drawn from the family as the drag left it.
            let s = sketch(&self.family, &self.view_type, length, view).unwrap_or(s);
            draw(
                &painter,
                &fit,
                &s,
                ui.visuals().text_color(),
                hot.as_deref(),
            );
        }
        ui.label(
            "Плоскость можно тащить мышью в виде: меняется число в конце её формулы \
             или размер, от которого она отложена. Esc — вернуть.",
        );
        if let Some(n) = &self.drag_note {
            ui.label(n);
        }
    }
}

/// Draw `s` as `fit` maps it, plane `hot` (under the mouse or dragged)
/// thicker and with its position.
fn draw(painter: &egui::Painter, fit: &Fit, s: &Sketch, color: egui::Color32, hot: Option<&str>) {
    let rect = fit.rect;
    let at = |p: [f64; 2]| fit.screen(p);
    for (name, right, v) in &s.planes {
        let is_hot = hot == Some(name.as_str());
        let (plane, width) = if is_hot {
            (egui::Color32::from_rgb(230, 120, 30), 2.)
        } else {
            (egui::Color32::from_rgb(70, 130, 200), 1.)
        };
        let (a, b) = if *right {
            (
                egui::pos2(at([*v, 0.]).x, rect.top() + 2.),
                egui::pos2(at([*v, 0.]).x, rect.bottom() - 2.),
            )
        } else {
            (
                egui::pos2(rect.left() + 2., at([0., *v]).y),
                egui::pos2(rect.right() - 2., at([0., *v]).y),
            )
        };
        painter.extend(egui::Shape::dashed_line(
            &[a, b],
            egui::Stroke::new(width, plane),
            6.,
            4.,
        ));
        let label = if is_hot {
            format!("{name} = {}", number(*v))
        } else {
            name.clone()
        };
        painter.text(
            a + egui::vec2(3., 2.),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(10.),
            plane,
        );
    }
    for [p, q] in &s.edges {
        painter.line_segment([at(*p), at(*q)], egui::Stroke::new(1.2, color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_bim_model::sample_families;

    #[test]
    fn the_blank_and_the_samples_pass_the_check() {
        assert!(check(&blank("Новое"), 3000.).is_empty());
        for f in sample_families() {
            assert!(check(&f, 3000.).is_empty(), "{}", f.name);
        }
    }

    #[test]
    fn the_check_names_the_failing_type() {
        let mut f = sample_families().remove(0);
        f.types[1].values.insert("t".into(), "-5".into());
        let problems = check(&f, 0.);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("С-2"), "{problems:?}");
    }

    #[test]
    fn views_project_edges_and_planes_seen_edge_on() {
        let f = blank("Новое");
        let front = sketch(&f, "Тип 1", 0., View::Front).unwrap();
        // A 200 × 300 plate in the XZ plane: four contour edges on each face.
        assert!(front.edges.len() >= 4);
        assert!(front.planes.contains(&("Право".into(), true, 200.)));
        assert!(front.planes.contains(&("Верх".into(), false, 300.)));
        let top = sketch(&f, "Тип 1", 0., View::Top).unwrap();
        assert!(!top.planes.iter().any(|p| p.0 == "Верх"));
    }

    #[test]
    fn dragging_a_plane_changes_the_size_it_is_laid_off_by() {
        let f = blank("Новое");
        // «Право» = X0 + B: B of the type size takes the new width.
        let (g, note) = drag_plane(&f, "Тип 1", 0., "Право", 250.).unwrap();
        assert_eq!(g.types[0].values["B"], "250");
        assert_eq!(g.parameters[1].value, "200");
        assert!(note.contains("B = 250"), "{note}");
        // Without a type size the default value changes.
        let (g, _) = drag_plane(&f, "", 0., "Верх", 280.).unwrap();
        assert_eq!(g.parameters[2].value, "280");
        let front = sketch(&g, "", 0., View::Front).unwrap();
        assert!(front.planes.contains(&("Верх".into(), false, 280.)));
    }

    #[test]
    fn dragging_a_plane_edits_numbers_or_adds_an_offset() {
        let mut f = blank("Новое");
        let plane = |at: &str| RefPlane {
            name: "Ось".into(),
            axis: Axis::X,
            at: at.into(),
        };
        f.planes.push(plane("Право - 150"));
        let moved = |f: &Family, to| drag_plane(f, "Тип 1", 0., "Ось", to).unwrap().0;
        assert_eq!(moved(&f, 120.).planes[2].at, "Право - 80");
        assert_eq!(moved(&f, 260.).planes[2].at, "Право + 60");
        f.planes[2] = plane("75");
        assert_eq!(moved(&f, 90.).planes[2].at, "90");
        // B is in the formula twice: changing it would move the plane
        // twice as far, so an offset is added instead.
        f.planes[2] = plane("B + B");
        assert_eq!(moved(&f, 450.).planes[2].at, "B + B + 50");
        f.planes[2] = plane("max(B, 100) * 2");
        assert_eq!(moved(&f, 380.).planes[2].at, "max(B, 100) * 2 - 20");
        f.planes[2] = plane("B * -1 + 500");
        assert_eq!(moved(&f, 310.).planes[2].at, "B * -1 + 510");
    }

    #[test]
    fn a_drag_that_breaks_a_body_is_refused() {
        let f = blank("Новое");
        // «Право» on the left of X0 below the smallest B and with a
        // contour turned inside out: nothing builds, the family stays.
        let r = drag_plane(&f, "Тип 1", 0., "Право", 0.);
        assert!(r.is_err(), "{r:?}");
        assert!(drag_plane(&f, "Тип 1", 0., "Нет", 0.).is_err());
    }

    #[test]
    fn a_plane_is_dragged_with_the_mouse_and_esc_returns_it() {
        let ctx = egui::Context::default();
        let mut e = FamilyEditor::default();
        e.edit(blank("Новое"));
        let run = |ctx: &egui::Context, e: &mut FamilyEditor, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 900.),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                e.show(ui.ctx(), &[]);
            });
            out.textures_delta.clear();
        };
        run(&ctx, &mut e, vec![]);
        run(&ctx, &mut e, vec![]);
        let at = e.plane_on_screen(View::Front, "Право").unwrap();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let to = at + egui::vec2(-40., 0.);
        run(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(at), button(at, true)],
        );
        for k in 1..=4 {
            let p = at + egui::vec2(-10. * k as f32, 0.);
            run(&ctx, &mut e, vec![egui::Event::PointerMoved(p)]);
        }
        let b: f64 = e.family.types[0].values["B"].parse().unwrap();
        assert!(b < 200. && b > 1., "B = {b}");
        // Esc while dragging puts the plane back.
        run(
            &ctx,
            &mut e,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(!e.family.types[0].values.contains_key("B"));
        run(&ctx, &mut e, vec![button(to, false)]);
        // A second drag that is let go keeps the new width.
        let at = e.plane_on_screen(View::Front, "Право").unwrap();
        run(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(at), button(at, true)],
        );
        for k in 1..=4 {
            let p = at + egui::vec2(-10. * k as f32, 0.);
            run(&ctx, &mut e, vec![egui::Event::PointerMoved(p)]);
        }
        run(&ctx, &mut e, vec![button(at + egui::vec2(-40., 0.), false)]);
        run(&ctx, &mut e, vec![]);
        assert!(e.family.types[0].values.contains_key("B"));
        assert!(e.drag.is_none());
    }
}
