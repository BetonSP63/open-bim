use super::{number, point};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{Detail, DetailKind, Frame, ObjectId, Project, WeldType};
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
    pub butt: bool,
    pub shop: bool,
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
            butt: false,
            shop: true,
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
            let steps = |s: &str| -> Result<Vec<f64>, String> {
                if s.trim().is_empty() {
                    Ok(vec![])
                } else {
                    s.split(';').map(number).collect()
                }
            };
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
                    weld_type: if self.butt {
                        WeldType::Butt
                    } else {
                        WeldType::Fillet
                    },
                    size: number(&self.weld_size)?,
                    path: vec![start, point(&self.end)?],
                    shop: self.shop,
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
                    field(ui, "Размер шва, мм", &mut self.weld_size);
                    ui.checkbox(&mut self.butt, "Стыковой (иначе угловой)");
                    ui.checkbox(&mut self.shop, "Заводской шов");
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
                    DetailKind::BoltGroup { .. } => "Болты",
                    DetailKind::Weld { .. } => "Шов",
                };
                ui.label(format!("{} · {}", detail.id, label));
            }
        });
        action
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let field_label = ui.label(label);
    ui.text_edit_singleline(value).labelled_by(field_label.id);
}
