//! «Свойства чертежа»: designation, title, sheet format and multiple,
//! orientation and scale of the open drawing, or the format and scale of
//! every drawing checked in the document manager, in one undo step.
use crate::tools::Editor;
use eframe::egui;
use open_bim_commands::Command;
use open_bim_drawings::SCALES;
use open_bim_model::{Drawing, DrawingKind, ObjectId, Project, SheetSize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct Props {
    pub number: String,
    pub title: String,
    pub sheet: SheetSize,
    /// 1 = basic format.
    pub multiple: u8,
    pub portrait: bool,
    pub scale: f64,
}

impl Props {
    pub fn of(d: &Drawing) -> Self {
        Self {
            number: d.number.clone(),
            title: d.title.clone(),
            sheet: d.sheet,
            multiple: d.multiple,
            portrait: d.portrait,
            scale: d.scale,
        }
    }
}

/// Keep views, notes and sheet objects inside a new paper size.
fn fit_to_sheet(d: &mut Drawing) {
    let [w, h] = d.sheet_mm();
    for view in &mut d.views {
        view.position = [
            view.position[0].clamp(25., (w - 10.).max(25.)),
            view.position[1].clamp(10., (h - 10.).max(10.)),
        ];
    }
    for note in &mut d.texts {
        note.position = [
            note.position[0].clamp(25., (w - 10.).max(25.)),
            note.position[1].clamp(10., (h - 10.).max(10.)),
        ];
    }
}

/// Commands applying `props` to the drawings `ids`. The designation and
/// title change only when one drawing is edited.
pub fn apply(
    project: &Project,
    ids: &BTreeSet<ObjectId>,
    props: &Props,
) -> Result<Vec<Command>, String> {
    if ids.is_empty() {
        return Err("Откройте чертёж или отметьте чертежи в диспетчере документов".into());
    }
    let mut commands = Vec::new();
    for id in ids {
        let mut d = project.drawings.get(id).ok_or("Чертёж не найден")?.clone();
        if ids.len() == 1 {
            d.number = props.number.trim().to_string();
            d.title = props.title.trim().to_string();
        }
        d.sheet = props.sheet;
        d.multiple = props.multiple;
        d.portrait = props.portrait;
        // «Общие данные» are not drawn to a scale.
        if !matches!(d.kind, DrawingKind::GeneralData { .. }) {
            d.scale = props.scale;
        }
        fit_to_sheet(&mut d);
        d.validate()
            .map_err(|e| format!("{}: {e}", d.number.clone()))?;
        if project.drawings.get(id) != Some(&d) {
            commands.push(Command::ReplaceDrawing { drawing: d });
        }
    }
    Ok(commands)
}

#[derive(Default)]
pub struct PropertiesWindow {
    pub open: bool,
    pub props: Option<Props>,
    /// Drawings the fields were loaded from.
    loaded: BTreeSet<ObjectId>,
    pub message: Option<String>,
}

impl PropertiesWindow {
    /// The window for `targets` (checked drawings, else the open one).
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor, targets: BTreeSet<ObjectId>) {
        if !self.open {
            return;
        }
        let project = editor.document.project();
        if self.loaded != targets || self.props.is_none() {
            self.loaded = targets.clone();
            self.props = targets
                .iter()
                .next()
                .and_then(|id| project.drawings.get(id))
                .map(Props::of);
        }
        let mut open = true;
        let mut apply_now = false;
        egui::Window::new("Свойства чертежа")
            .id(egui::Id::new("drawing properties"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let Some(p) = &mut self.props else {
                    ui.label("Откройте чертёж или отметьте чертежи в диспетчере документов");
                    return;
                };
                if targets.len() > 1 {
                    ui.label(format!(
                        "Чертежей: {} — меняются формат, ориентация и масштаб",
                        targets.len()
                    ));
                }
                egui::Grid::new("drawing properties grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        if targets.len() == 1 {
                            let l = ui.label("Обозначение");
                            ui.text_edit_singleline(&mut p.number).labelled_by(l.id);
                            ui.end_row();
                            let l = ui.label("Наименование");
                            ui.text_edit_singleline(&mut p.title).labelled_by(l.id);
                            ui.end_row();
                        }
                        let l = ui.label("Формат");
                        egui::ComboBox::from_id_salt("drawing properties sheet")
                            .selected_text(p.sheet.label())
                            .show_ui(ui, |ui| {
                                for s in [
                                    SheetSize::A4,
                                    SheetSize::A3,
                                    SheetSize::A2,
                                    SheetSize::A1,
                                    SheetSize::A0,
                                ] {
                                    ui.selectable_value(&mut p.sheet, s, s.label());
                                }
                            })
                            .response
                            .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Кратность");
                        let multiples = std::iter::once(1).chain(p.sheet.multiples());
                        if p.multiple != 1 && !p.sheet.multiples().contains(&p.multiple) {
                            p.multiple = 1;
                        }
                        egui::ComboBox::from_id_salt("drawing properties multiple")
                            .selected_text(if p.multiple == 1 {
                                "основной".to_string()
                            } else {
                                format!("×{}", p.multiple)
                            })
                            .show_ui(ui, |ui| {
                                for m in multiples {
                                    let text = if m == 1 {
                                        "основной".to_string()
                                    } else {
                                        format!("×{m}")
                                    };
                                    ui.selectable_value(&mut p.multiple, m, text);
                                }
                            })
                            .response
                            .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Ориентация");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut p.portrait, false, "Альбомная");
                            ui.radio_value(&mut p.portrait, true, "Книжная");
                        })
                        .response
                        .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Масштаб");
                        egui::ComboBox::from_id_salt("drawing properties scale")
                            .selected_text(format!("1:{}", p.scale))
                            .show_ui(ui, |ui| {
                                for s in SCALES {
                                    ui.selectable_value(&mut p.scale, s, format!("1:{s}"));
                                }
                            })
                            .response
                            .labelled_by(l.id);
                        ui.end_row();
                    });
                apply_now = ui.button("Изменить").clicked();
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        if !apply_now {
            return;
        }
        let Some(props) = self.props.clone() else {
            return;
        };
        let result = apply(editor.document.project(), &targets, &props).and_then(|commands| {
            if commands.is_empty() {
                return Ok(0);
            }
            let n = commands.len();
            editor.commit_data(&format!("Свойства чертежа: {n}"), &commands)?;
            Ok(n)
        });
        self.message = Some(match result {
            Ok(0) => "Свойства не изменились".into(),
            Ok(n) => format!("Изменено чертежей: {n}"),
            Err(e) => e,
        });
    }
}
