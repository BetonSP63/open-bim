use super::{number, point};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    Detail, DetailKind, Frame, Intermittent, ObjectId, Project, WeldGost, WeldProcess, WeldSides,
    WeldType,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailTool {
    Hole,
    Plane,
    Box,
    Bolts,
    Weld,
}
impl DetailTool {
    fn label(self) -> &'static str {
        match self {
            Self::Hole => "Отверстие",
            Self::Plane => "Срез плоскостью",
            Self::Box => "Вырез",
            Self::Bolts => "Болтовая группа",
            Self::Weld => "Шов",
        }
    }
}
/// «Форма» of a bolt group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoltShape {
    /// Rows and columns by the X and Y spacings.
    #[default]
    Array,
    /// `circle_count` bolts on a circle of `circle_diameter` around the origin.
    Circle,
    /// Bolt coordinates from the origin: X list and Y list, bolt by bolt.
    List,
}
impl BoltShape {
    pub const ALL: [Self; 3] = [Self::Array, Self::Circle, Self::List];
    pub fn label(self) -> &'static str {
        match self {
            Self::Array => "Массив",
            Self::Circle => "Окружность",
            Self::List => "Список",
        }
    }
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.label() == label)
    }
}
pub struct DetailingUi {
    pub kind: DetailTool,
    pub start: [String; 3],
    pub end: [String; 3],
    pub size: [String; 3],
    pub diameter: String,
    pub depth: String,
    pub offset: String,
    pub axis: usize,
    pub clearance: String,
    pub steps_x: String,
    pub steps_y: String,
    pub bolt_shape: BoltShape,
    pub circle_count: String,
    pub circle_diameter: String,
    pub standard: String,
    pub grade: String,
    pub weld_size: String,
    pub weld_type: WeldType,
    /// Welding process; it gives the standard in the weld mark.
    pub weld_process: WeldProcess,
    pub weld_sides: WeldSides,
    /// Welded all around («по контуру»).
    pub weld_around: bool,
    /// Joint designation picked from the standard; empty — by type and sides.
    pub weld_joint: String,
    /// Welding method of the standard («УП», «АФ»); empty — not shown.
    pub weld_method: String,
    /// Intermittent weld: welded length and pitch, staggered.
    pub weld_intermittent: bool,
    pub weld_segment: String,
    pub weld_pitch: String,
    pub weld_staggered: bool,
    /// Seen from the reverse side (designation under the shelf).
    pub weld_back: bool,
    pub shop: bool,
    /// Bolt group shift along its X and Y from the picked start point, mm.
    pub shift_x: String,
    pub shift_y: String,
    /// Swap the X and Y spacings («Повернуть» by 90°).
    pub rotate: bool,
    /// Gusset legs and corner chamfer, stiffener corner snips, mm.
    pub gusset_leg: String,
    pub gusset_chamfer: String,
    pub stiffener_snip: String,
    /// «Точки на линии»: number of equal parts.
    pub point_divisions: String,
    /// «Фаска кромки»: legs, mm (the second empty = equal legs).
    pub edge_chamfer: [String; 2],
    /// «Ниша»: depth from the face, mm.
    pub recess_depth: String,
}
impl Default for DetailingUi {
    fn default() -> Self {
        Self {
            kind: DetailTool::Hole,
            start: ["50".into(), "50".into(), "-1".into()],
            end: ["150".into(), "50".into(), "0".into()],
            size: ["20".into(), "20".into(), "22".into()],
            diameter: "20".into(),
            depth: "42".into(),
            offset: "150".into(),
            axis: 2,
            clearance: "2".into(),
            steps_x: "50".into(),
            steps_y: "50".into(),
            bolt_shape: BoltShape::Array,
            circle_count: "6".into(),
            circle_diameter: "200".into(),
            standard: open_bim_connections::norms::DEFAULT_BOLT_STANDARD.into(),
            grade: "8.8".into(),
            weld_size: "6".into(),
            weld_type: WeldType::Fillet,
            weld_process: WeldProcess::Manual,
            weld_sides: WeldSides::One,
            weld_around: false,
            weld_joint: String::new(),
            weld_method: String::new(),
            weld_intermittent: false,
            weld_segment: "50".into(),
            weld_pitch: "150".into(),
            weld_staggered: false,
            weld_back: false,
            shop: true,
            shift_x: "0".into(),
            shift_y: "0".into(),
            rotate: false,
            gusset_leg: "300".into(),
            gusset_chamfer: "30".into(),
            stiffener_snip: "20".into(),
            point_divisions: "4".into(),
            edge_chamfer: ["10".into(), String::new()],
            recess_depth: "100".into(),
        }
    }
}
impl DetailingUi {
    pub fn commands(
        &self,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Result<Vec<Command>, String> {
        project.validate().map_err(|e| e.to_string())?;
        if selected.is_empty() || selected.iter().any(|id| !project.elements.contains_key(id)) {
            return Err("Выберите детали модели".into());
        }
        let start = point(&self.start)?;
        let axes = match self.axis {
            0 => [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
            1 => [[1., 0., 0.], [0., 0., -1.], [0., 1., 0.]],
            _ => Frame::default().axes,
        };
        let frame = Frame {
            origin: start,
            axes,
        };
        if self.kind == DetailTool::Bolts {
            let (steps_x, steps_y, points) = self.bolt_layout()?;
            return open_bim_steel::bolt_commands(
                project,
                &open_bim_steel::BoltPattern {
                    parts: selected.clone(),
                    frame,
                    diameter: number(&self.diameter)?,
                    clearance: number(&self.clearance)?,
                    hole_depth: number(&self.depth)?,
                    steps_x,
                    steps_y,
                    standard: self.standard.clone(),
                    grade: self.grade.clone(),
                    points,
                },
            )
            .map_err(|e| e.to_string());
        }
        let mut candidate = project.clone();
        let mut commands = Vec::new();
        let targets: Vec<_> = if self.kind == DetailTool::Weld {
            vec![*selected.first().unwrap()]
        } else {
            selected.iter().copied().collect()
        };
        for part in targets {
            let kind = match self.kind {
                DetailTool::Hole => DetailKind::Hole {
                    part,
                    frame,
                    diameter: number(&self.diameter)?,
                    depth: number(&self.depth)?,
                },
                DetailTool::Plane => DetailKind::PlaneCut {
                    part,
                    normal: axes[2],
                    offset: number(&self.offset)?,
                },
                DetailTool::Box => DetailKind::BoxCut {
                    part,
                    origin: start,
                    size: point(&self.size)?.0,
                },
                DetailTool::Weld => DetailKind::Weld {
                    parts: selected.clone(),
                    weld_type: self.weld_type,
                    size: number(&self.weld_size)?,
                    path: vec![start, point(&self.end)?],
                    shop: self.shop,
                    sides: self.weld_sides,
                    around: self.weld_around,
                    process: self.weld_process,
                    gost: self.weld_gost()?,
                },
                DetailTool::Bolts => unreachable!(),
            };
            let detail = Detail {
                id: candidate.allocate_id().map_err(|e| e.to_string())?,
                kind,
            };
            candidate.details.insert(detail.id, detail.clone());
            commands.push(Command::CreateDetail { detail });
        }
        candidate.validate().map_err(|e| e.to_string())?;
        Ok(commands)
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<Result<Vec<Command>, String>> {
        let mut action = None;
        ui.collapsing("Деталировка стали", |ui| {
            ui.horizontal_wrapped(|ui| {
                for kind in [
                    DetailTool::Hole,
                    DetailTool::Plane,
                    DetailTool::Box,
                    DetailTool::Bolts,
                    DetailTool::Weld,
                ] {
                    ui.selectable_value(&mut self.kind, kind, kind.label());
                }
            });
            ui.label(
                if matches!(self.kind, DetailTool::Bolts | DetailTool::Weld) {
                    "Мировые координаты, мм"
                } else {
                    "Локальные координаты каждой выбранной детали, мм"
                },
            );
            super::ui::vector_fields(ui, "detail start", &mut self.start);
            if self.kind != DetailTool::Weld {
                ui.horizontal(|ui| {
                    ui.label("Направление");
                    for (i, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                        ui.selectable_value(&mut self.axis, i, label);
                    }
                });
            }
            match self.kind {
                DetailTool::Hole | DetailTool::Bolts => {
                    field(ui, "Диаметр, мм", &mut self.diameter);
                    field(ui, "Глубина, мм", &mut self.depth);
                    if self.kind == DetailTool::Bolts {
                        field(ui, "Зазор отверстия, мм", &mut self.clearance);
                        field(ui, "Шаги X через ;", &mut self.steps_x);
                        field(ui, "Шаги Y через ;", &mut self.steps_y);
                        field(ui, "Стандарт", &mut self.standard);
                        field(ui, "Класс прочности", &mut self.grade);
                    }
                }
                DetailTool::Plane => {
                    field(ui, "Оставить координаты ≤, мм", &mut self.offset);
                }
                DetailTool::Box => {
                    ui.label("Размеры выреза, мм");
                    super::ui::vector_fields(ui, "cut size", &mut self.size);
                }
                DetailTool::Weld => {
                    ui.label("Конец шва, мм");
                    super::ui::vector_fields(ui, "weld end", &mut self.end);
                    self.weld_fields(ui);
                }
            }
            if ui
                .add_enabled(
                    !selected.is_empty(),
                    egui::Button::new("Создать деталировку"),
                )
                .clicked()
            {
                action = Some(self.commands(project, selected));
            }
            for detail in project
                .details
                .values()
                .filter(|d| !d.kind.references().is_disjoint(selected))
            {
                let label = match &detail.kind {
                    DetailKind::Hole { .. } => "Отверстие",
                    DetailKind::PlaneCut { .. } => "Срез",
                    DetailKind::BoxCut { .. } => "Вырез",
                    DetailKind::PrismCut { .. } => "Вырез по контуру",
                    DetailKind::BoltGroup { .. } => "Болты",
                    DetailKind::Weld { .. } => "Шов",
                };
                ui.label(format!("{} · {}", detail.id, label));
            }
        });
        action
    }
}
/// Spacings «80 80», «80;80» or «3*80» → [80, 80, 80].
pub fn spacings(text: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for token in text.split([';', ' ', ',']).filter(|t| !t.trim().is_empty()) {
        match token.split_once('*') {
            Some((n, step)) => {
                let n: usize = n
                    .trim()
                    .parse()
                    .map_err(|_| format!("«{token}»: число повторов"))?;
                if n == 0 || n > 200 {
                    return Err(format!("«{token}»: 1…200 повторов"));
                }
                out.extend(std::iter::repeat_n(number(step)?, n));
            }
            None => out.push(number(token)?),
        }
    }
    if out.iter().any(|v| *v <= 0.) {
        return Err("Шаги должны быть положительными".into());
    }
    Ok(out)
}

/// X steps, Y steps (array) and bolt points (circle, list) of a bolt group.
pub type BoltLayout = (Vec<f64>, Vec<f64>, Vec<[f64; 2]>);

/// Coordinates «75 175 250», «-50 0 2*50» (repeats allowed, any sign).
pub fn coordinates(text: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for token in text.split([';', ' ', ',']).filter(|t| !t.trim().is_empty()) {
        match token.split_once('*') {
            Some((n, value)) => {
                let n: usize = n
                    .trim()
                    .parse()
                    .map_err(|_| format!("«{token}»: число повторов"))?;
                if n == 0 || n > 200 {
                    return Err(format!("«{token}»: 1…200 повторов"));
                }
                out.extend(std::iter::repeat_n(number(value)?, n));
            }
            None => out.push(number(token)?),
        }
    }
    Ok(out)
}

impl DetailingUi {
    /// Steps of an array group, or bolt points of a circle or list group,
    /// in the group frame («Повернуть» swaps X and Y).
    pub fn bolt_layout(&self) -> Result<BoltLayout, String> {
        let (mut steps_x, mut steps_y, mut points) = match self.bolt_shape {
            BoltShape::Array => (spacings(&self.steps_x)?, spacings(&self.steps_y)?, vec![]),
            BoltShape::Circle => {
                let count: usize = self
                    .circle_count
                    .trim()
                    .parse()
                    .map_err(|_| "Число болтов на окружности — целое число")?;
                if !(2..=360).contains(&count) {
                    return Err("На окружности от 2 до 360 болтов".into());
                }
                let diameter = number(&self.circle_diameter)?;
                if diameter <= 0. {
                    return Err("Диаметр окружности болтов должен быть больше 0".into());
                }
                (
                    vec![],
                    vec![],
                    open_bim_model::circle_points(count, diameter),
                )
            }
            BoltShape::List => {
                let xs = coordinates(&self.steps_x)?;
                let mut ys = coordinates(&self.steps_y)?;
                if ys.len() == 1 && xs.len() > 1 {
                    ys = vec![ys[0]; xs.len()];
                }
                if xs.is_empty() || xs.len() != ys.len() {
                    return Err(format!(
                        "Координат X ({}) и Y ({}) должно быть поровну",
                        xs.len(),
                        ys.len()
                    ));
                }
                let points = xs.into_iter().zip(ys).map(|(x, y)| [x, y]).collect();
                (vec![], vec![], points)
            }
        };
        if self.rotate {
            std::mem::swap(&mut steps_x, &mut steps_y);
            for p in &mut points {
                p.swap(0, 1);
            }
        }
        Ok((steps_x, steps_y, points))
    }

    /// Designation details of ГОСТ 2.312 from the fields.
    pub fn weld_gost(&self) -> Result<WeldGost, String> {
        let intermittent = if self.weld_intermittent {
            let length = number(&self.weld_segment)?;
            let pitch = number(&self.weld_pitch)?;
            if length <= 0. || pitch <= length {
                return Err("Шаг прерывистого шва должен быть больше длины участка".into());
            }
            Some(Intermittent {
                length,
                pitch,
                staggered: self.weld_staggered,
            })
        } else {
            None
        };
        Ok(WeldGost {
            joint: self.weld_joint.trim().to_string(),
            method: self.weld_method.trim().to_string(),
            intermittent,
            back: self.weld_back,
        })
    }

    /// Fill the weld fields from an existing weld.
    pub fn load_weld(&mut self, kind: &DetailKind) {
        let DetailKind::Weld {
            weld_type,
            size,
            shop,
            sides,
            around,
            process,
            gost,
            ..
        } = kind
        else {
            return;
        };
        self.weld_type = *weld_type;
        self.weld_size = mm(*size);
        self.shop = *shop;
        self.weld_sides = *sides;
        self.weld_around = *around;
        self.weld_process = *process;
        self.weld_joint = gost.joint.clone();
        self.weld_method = gost.method.clone();
        self.weld_back = gost.back;
        self.weld_intermittent = gost.intermittent.is_some();
        if let Some(i) = gost.intermittent {
            self.weld_segment = mm(i.length);
            self.weld_pitch = mm(i.pitch);
            self.weld_staggered = i.staggered;
        }
    }

    /// The weld `kind` with the properties of the fields; path and parts stay.
    pub fn apply_weld(&self, kind: &DetailKind) -> Result<DetailKind, String> {
        let DetailKind::Weld { parts, path, .. } = kind else {
            return Err("Это не сварной шов".into());
        };
        let size = number(&self.weld_size)?;
        if size <= 0. {
            return Err("Катет / размер шва должен быть больше 0".into());
        }
        Ok(DetailKind::Weld {
            parts: parts.clone(),
            weld_type: self.weld_type,
            size,
            path: path.clone(),
            shop: self.shop,
            sides: self.weld_sides,
            around: self.weld_around,
            process: self.weld_process,
            gost: self.weld_gost()?,
        })
    }

    /// Weld properties by the Russian standards: process (standard), joint
    /// type and sides or a designation from the standard, method, leg,
    /// intermittent weld, signs of ГОСТ 2.312, and the resulting designation
    /// with what does not fit.
    pub fn weld_fields(&mut self, ui: &mut egui::Ui) {
        let l = ui.label("Способ сварки (стандарт)");
        let before = self.weld_process;
        egui::ComboBox::from_id_salt("weld-process")
            .width(ui.available_width())
            .selected_text(format!(
                "{} ({})",
                self.weld_process.label(),
                self.weld_process.standard()
            ))
            .show_ui(ui, |ui| {
                for p in WeldProcess::ALL {
                    ui.selectable_value(
                        &mut self.weld_process,
                        p,
                        format!("{} ({})", p.label(), p.standard()),
                    )
                    .on_hover_text(p.title());
                }
            })
            .response
            .labelled_by(l.id);
        if before != self.weld_process {
            // Another standard: its own methods and joint numbering.
            if !self
                .weld_process
                .methods()
                .iter()
                .any(|(m, _)| *m == self.weld_method)
            {
                self.weld_method.clear();
            }
            if !self.weld_process.angled() {
                self.weld_joint.clear();
            }
        }
        let l = ui.label("Тип соединения");
        egui::ComboBox::from_id_salt("weld type")
            .width(ui.available_width())
            .selected_text(self.weld_type.label())
            .show_ui(ui, |ui| {
                for t in WeldType::ALL {
                    if ui
                        .selectable_value(&mut self.weld_type, t, t.label())
                        .changed()
                    {
                        self.weld_joint.clear();
                    }
                }
            })
            .response
            .labelled_by(l.id);
        ui.horizontal(|ui| {
            for (s, caption) in [
                (WeldSides::One, "Односторонний"),
                (WeldSides::Both, "Двусторонний"),
            ] {
                if ui
                    .selectable_value(&mut self.weld_sides, s, caption)
                    .changed()
                {
                    self.weld_joint.clear();
                }
            }
        });
        // Designation from the standard's table, or typed by hand (joints
        // under angles are numbered by the angle in their standards).
        let catalog = self.weld_process.catalog();
        let auto = self.weld_type.joint(self.weld_sides, self.weld_process);
        let l = ui.label("Обозначение соединения по стандарту");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("weld-joint")
                .selected_text(if self.weld_joint.is_empty() {
                    if auto.is_empty() {
                        "—".to_string()
                    } else {
                        format!("{auto} (по типу)")
                    }
                } else {
                    self.weld_joint.clone()
                })
                .show_ui(ui, |ui| {
                    if !auto.is_empty() {
                        ui.selectable_value(
                            &mut self.weld_joint,
                            String::new(),
                            format!("{auto} — по типу и сторонам"),
                        );
                    }
                    let mut kind = None;
                    for j in &catalog {
                        if kind != Some(j.weld_type.kind()) {
                            kind = Some(j.weld_type.kind());
                            ui.label(egui::RichText::new(j.weld_type.kind().label()).strong());
                        }
                        let caption = format!("{} — {}, {}", j.code, j.edges, j.sides.label());
                        if ui
                            .selectable_label(self.weld_joint == j.code, caption)
                            .clicked()
                        {
                            self.weld_joint = j.code.into();
                            self.weld_type = j.weld_type;
                            self.weld_sides = j.sides;
                        }
                    }
                })
                .response
                .labelled_by(l.id);
            ui.add(
                egui::TextEdit::singleline(&mut self.weld_joint)
                    .hint_text("своё")
                    .desired_width(60.),
            )
            .on_hover_text("Обозначение из таблицы стандарта, например «Т8» или «У4»");
        });
        let methods = self.weld_process.methods();
        if !methods.is_empty() {
            let l = ui.label("Условное обозначение способа сварки");
            egui::ComboBox::from_id_salt("weld-method")
                .width(ui.available_width())
                .selected_text(if self.weld_method.is_empty() {
                    "не указывать".to_string()
                } else {
                    self.weld_method.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.weld_method, String::new(), "не указывать");
                    for (code, name) in methods {
                        ui.selectable_value(
                            &mut self.weld_method,
                            (*code).to_string(),
                            format!("{code} — {name}"),
                        );
                    }
                })
                .response
                .labelled_by(l.id);
        }
        field(
            ui,
            if self.weld_type.has_leg() {
                "Катет шва k, мм"
            } else {
                "Размер шва, мм"
            },
            &mut self.weld_size,
        );
        ui.checkbox(&mut self.weld_intermittent, "Прерывистый шов");
        if self.weld_intermittent {
            egui::Grid::new("weld intermittent")
                .num_columns(2)
                .show(ui, |ui| {
                    for (caption, value) in [
                        ("Длина участка l, мм", &mut self.weld_segment),
                        ("Шаг t, мм", &mut self.weld_pitch),
                    ] {
                        let l = ui.label(caption);
                        ui.add(egui::TextEdit::singleline(value).desired_width(60.))
                            .labelled_by(l.id);
                        ui.end_row();
                    }
                });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.weld_staggered, false, "Цепной ( / )");
                ui.selectable_value(&mut self.weld_staggered, true, "Шахматный ( Z )");
            });
        }
        ui.checkbox(&mut self.weld_around, "По замкнутой линии (знак ○)");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.shop, true, "Заводской");
            ui.selectable_value(&mut self.shop, false, "Монтажный (знак ┐)");
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.weld_back, false, "Лицевая сторона");
            ui.selectable_value(&mut self.weld_back, true, "Оборотная");
        })
        .response
        .on_hover_text("Шов с лицевой стороны — обозначение над полкой, с оборотной — под полкой");
        match (number(&self.weld_size), self.weld_gost()) {
            (Ok(size), Ok(gost)) => {
                let mark = open_bim_model::weld_mark(
                    self.weld_process,
                    self.weld_type,
                    self.weld_sides,
                    size,
                    &gost,
                ) + &open_bim_model::weld_notes(self.weld_around, self.shop);
                ui.label(format!("Обозначение: {mark}"));
                for advice in open_bim_model::weld_advice(
                    self.weld_process,
                    self.weld_type,
                    self.weld_sides,
                    &gost,
                ) {
                    ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {advice}"));
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                ui.colored_label(ui.visuals().warn_fg_color, e);
            }
        }
    }

    /// Bolt group properties for the interactive bolt command.
    pub fn bolt_fields(&mut self, ui: &mut egui::Ui) {
        field(ui, "Стандарт", &mut self.standard);
        field(ui, "Класс прочности", &mut self.grade);
        field(ui, "Диаметр, мм", &mut self.diameter);
        field(ui, "Зазор отверстия, мм", &mut self.clearance);
        let l = ui.label("Форма группы");
        egui::ComboBox::from_id_salt("bolt-shape")
            .selected_text(self.bolt_shape.label())
            .show_ui(ui, |ui| {
                for s in BoltShape::ALL {
                    ui.selectable_value(&mut self.bolt_shape, s, s.label());
                }
            })
            .response
            .labelled_by(l.id);
        match self.bolt_shape {
            BoltShape::Array => {
                field(ui, "Шаги по X («3*80»)", &mut self.steps_x);
                field(ui, "Шаги по Y («80»)", &mut self.steps_y);
            }
            BoltShape::Circle => {
                field(ui, "Число болтов", &mut self.circle_count);
                field(ui, "Диаметр окружности, мм", &mut self.circle_diameter);
            }
            BoltShape::List => {
                field(ui, "Координаты X («75 175 250»)", &mut self.steps_x);
                field(ui, "Координаты Y («75 -50 0»)", &mut self.steps_y);
            }
        }
        field(ui, "Сдвиг по X, мм", &mut self.shift_x);
        field(ui, "Сдвиг по Y, мм", &mut self.shift_y);
        ui.checkbox(&mut self.rotate, "Повернуть на 90° (поменять X и Y)");
        match (self.bolt_shape, self.bolt_layout()) {
            (BoltShape::Array, Ok((x, y, _))) => {
                ui.small(format!(
                    "Болтов в группе: {} × {} = {}",
                    x.len() + 1,
                    y.len() + 1,
                    (x.len() + 1) * (y.len() + 1)
                ));
            }
            (_, Ok((_, _, points))) => {
                ui.small(format!("Болтов в группе: {}", points.len()));
            }
            (_, Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
    }
}

/// A length for a text field: «6», «4.5».
fn mm(v: f64) -> String {
    if v.fract().abs() < 1e-9 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let field_label = ui.label(label);
    ui.text_edit_singleline(value).labelled_by(field_label.id);
}
