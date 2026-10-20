use super::{number, point};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{Detail, DetailKind, Frame, ObjectId, Project, WeldSides, WeldType};
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
    pub standard: String,
    pub grade: String,
    pub weld_size: String,
    pub weld_type: WeldType,
    pub weld_sides: WeldSides,
    /// Welded all around («по контуру»).
    pub weld_around: bool,
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
            standard: "ISO 4014".into(),
            grade: "8.8".into(),
            weld_size: "6".into(),
            weld_type: WeldType::Fillet,
            weld_sides: WeldSides::One,
            weld_around: false,
            shop: true,
            shift_x: "0".into(),
            shift_y: "0".into(),
            rotate: false,
            gusset_leg: "300".into(),
            gusset_chamfer: "30".into(),
            stiffener_snip: "20".into(),
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
            let steps = |s: &str| spacings(s);
            return open_bim_steel::bolt_commands(
                project,
                &open_bim_steel::BoltPattern {
                    parts: selected.clone(),
                    frame,
                    diameter: number(&self.diameter)?,
                    clearance: number(&self.clearance)?,
                    hole_depth: number(&self.depth)?,
                    steps_x: steps(&self.steps_x)?,
                    steps_y: steps(&self.steps_y)?,
                    standard: self.standard.clone(),
                    grade: self.grade.clone(),
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

impl DetailingUi {
    /// Weld properties: type, sides, leg, all around, shop/site, and the
    /// resulting designation.
    pub fn weld_fields(&mut self, ui: &mut egui::Ui) {
        let l = ui.label("Тип шва");
        egui::ComboBox::from_id_salt("weld type")
            .selected_text(self.weld_type.label())
            .show_ui(ui, |ui| {
                for t in WeldType::ALL {
                    ui.selectable_value(&mut self.weld_type, t, t.label());
                }
            })
            .response
            .labelled_by(l.id);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.weld_sides, WeldSides::One, "Односторонний");
            ui.selectable_value(&mut self.weld_sides, WeldSides::Both, "Двусторонний");
        });
        field(ui, "Катет / размер шва, мм", &mut self.weld_size);
        ui.checkbox(&mut self.weld_around, "По контуру");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.shop, true, "Заводской");
            ui.selectable_value(&mut self.shop, false, "Монтажный");
        });
        if let Ok(size) = number(&self.weld_size) {
            ui.small(format!(
                "Обозначение: {}",
                open_bim_model::weld_mark(
                    self.weld_type,
                    self.weld_sides,
                    size,
                    self.weld_around,
                    self.shop
                )
            ));
        }
    }

    /// Bolt group properties for the interactive bolt command.
    pub fn bolt_fields(&mut self, ui: &mut egui::Ui) {
        field(ui, "Стандарт", &mut self.standard);
        field(ui, "Класс прочности", &mut self.grade);
        field(ui, "Диаметр, мм", &mut self.diameter);
        field(ui, "Зазор отверстия, мм", &mut self.clearance);
        field(ui, "Шаги по X («3*80»)", &mut self.steps_x);
        field(ui, "Шаги по Y («80»)", &mut self.steps_y);
        field(ui, "Сдвиг по X, мм", &mut self.shift_x);
        field(ui, "Сдвиг по Y, мм", &mut self.shift_y);
        ui.checkbox(&mut self.rotate, "Повернуть на 90° (поменять X и Y)");
        match (spacings(&self.steps_x), spacings(&self.steps_y)) {
            (Ok(x), Ok(y)) => {
                ui.small(format!(
                    "Болтов в группе: {} × {} = {}",
                    x.len() + 1,
                    y.len() + 1,
                    (x.len() + 1) * (y.len() + 1)
                ));
            }
            (Err(e), _) | (_, Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let field_label = ui.label(label);
    ui.text_edit_singleline(value).labelled_by(field_label.id);
}
