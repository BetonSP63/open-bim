//! «Менеджер документов» (Ctrl+L): every drawing of the project in one table
//! with its mark, sheet, status and issue, filters, and batch commands on
//! the checked rows. Batch edits are one transaction each, so one ⌘Z undoes
//! the whole batch.
use super::{DrawingEditor, ExportFormat, safe_file_name};
use crate::tools::Editor;
use open_bim_commands::{Command, DeletePolicy};
use open_bim_drawings::Status;
use open_bim_model::{DrawingKind, ObjectId, Project};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KindFilter {
    #[default]
    All,
    GeneralData,
    General,
    Assembly,
    Part,
}

impl KindFilter {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::GeneralData,
        Self::General,
        Self::Assembly,
        Self::Part,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Все типы",
            Self::GeneralData => "Общие данные",
            Self::General => "Общие виды",
            Self::Assembly => "Сборки",
            Self::Part => "Детали",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    All,
    UpToDate,
    Outdated,
    Lost,
    Issued,
    NotIssued,
}

impl StatusFilter {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::UpToDate,
        Self::Outdated,
        Self::Lost,
        Self::Issued,
        Self::NotIssued,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Любой статус",
            Self::UpToDate => "Актуальные",
            Self::Outdated => "Устаревшие",
            Self::Lost => "Потеряны привязки",
            Self::Issued => "Выданные",
            Self::NotIssued => "Не выданные",
        }
    }
    fn accepts(self, row: &Row) -> bool {
        match self {
            Self::All => true,
            Self::UpToDate => row.status == Status::UpToDate,
            Self::Outdated => matches!(row.status, Status::Outdated { .. }),
            Self::Lost => matches!(row.status, Status::LostReferences { .. }),
            Self::Issued => row.issued.is_some(),
            Self::NotIssued => row.issued.is_none(),
        }
    }
}

/// One table row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: ObjectId,
    /// Set mark (КМ, КМД) and sheet number within the set.
    pub set: &'static str,
    pub sheet_no: usize,
    pub number: String,
    pub title: String,
    pub kind: &'static str,
    /// Part position or shipping mark of a part or assembly drawing.
    pub mark: String,
    pub format: String,
    pub scale: f64,
    pub status: Status,
    pub issued: Option<u64>,
}

/// Rows of all drawings, by set and sheet number.
pub fn rows(project: &Project, marks: &BTreeMap<ObjectId, String>) -> Vec<Row> {
    let mut rows: Vec<Row> = project
        .drawings
        .values()
        .map(|d| {
            let mark = match &d.kind {
                DrawingKind::Part { part } => marks.get(part).cloned(),
                DrawingKind::Assembly { assembly } => marks.get(assembly).cloned(),
                _ => None,
            };
            let format = if d.multiple > 1 {
                format!("{}×{}", d.sheet.label(), d.multiple)
            } else {
                d.sheet.label().to_string()
            };
            Row {
                id: d.id,
                set: open_bim_drawings::set_mark(&d.kind),
                sheet_no: open_bim_drawings::sheet_number(project, d).1,
                number: d.number.clone(),
                title: d.title.clone(),
                kind: DrawingEditor::kind_label(&d.kind),
                mark: mark.unwrap_or_default(),
                format,
                scale: d.scale,
                status: open_bim_drawings::status(project, d),
                issued: d.issued_revision,
            }
        })
        .collect();
    rows.sort_by(|a, b| (a.set, a.sheet_no, a.id).cmp(&(b.set, b.sheet_no, b.id)));
    rows
}

/// Rows that pass the type, status and text filters. The text matches the
/// designation, title, mark or type, case-insensitively.
pub fn filtered(rows: &[Row], kind: KindFilter, status: StatusFilter, text: &str) -> Vec<Row> {
    let needle = text.trim().to_lowercase();
    rows.iter()
        .filter(|r| status.accepts(r))
        .filter(|r| {
            needle.is_empty()
                || [&r.number, &r.title, &r.mark, &r.kind.to_string()]
                    .iter()
                    .any(|t| t.to_lowercase().contains(&needle))
        })
        .filter(|r| kind_of(r, kind))
        .cloned()
        .collect()
}

fn kind_of(row: &Row, kind: KindFilter) -> bool {
    row.kind
        == match kind {
            KindFilter::All => return true,
            KindFilter::GeneralData => "Общие данные",
            KindFilter::General => "Общий вид",
            KindFilter::Assembly => "Сборка",
            KindFilter::Part => "Деталь",
        }
}

/// Drawings of the checked rows in table order.
fn ordered<'a>(project: &'a Project, ids: &BTreeSet<ObjectId>) -> Vec<&'a open_bim_model::Drawing> {
    rows(project, &BTreeMap::new())
        .into_iter()
        .filter(|r| ids.contains(&r.id))
        .filter_map(|r| project.drawings.get(&r.id))
        .collect()
}

/// Result of a batch: how many drawings were changed and why others were not.
#[derive(Debug, Default, PartialEq)]
pub struct Batch {
    pub done: usize,
    pub skipped: Vec<String>,
}

impl Batch {
    pub fn message(&self, verb: &str) -> String {
        let mut text = format!("{verb}: {}", self.done);
        if !self.skipped.is_empty() {
            text.push_str(&format!(
                "; пропущено {}: {}",
                self.skipped.len(),
                self.skipped.join("; ")
            ));
        }
        text
    }
}

/// «Обновить по модели» for each drawing. Drawings whose dimensions point to
/// deleted objects are skipped: their dimensions need a manual fix first.
pub fn update(editor: &mut Editor, ids: &BTreeSet<ObjectId>) -> Result<Batch, String> {
    let project = editor.document.project();
    let mut batch = Batch::default();
    let mut commands = Vec::new();
    for d in ordered(project, ids) {
        let status = open_bim_drawings::status(project, d);
        if status == Status::UpToDate {
            continue;
        }
        if let Status::LostReferences { missing } = &status
            && d.dimensions
                .iter()
                .any(|dim| dim.anchors.iter().any(|a| missing.contains(&a.object)))
        {
            batch
                .skipped
                .push(format!("{}: размеры с потерянными привязками", d.number));
            continue;
        }
        commands.push(Command::ReplaceDrawing {
            drawing: open_bim_drawings::updated(project, d),
        });
    }
    batch.done = commands.len();
    if !commands.is_empty() {
        editor.commit_data(&format!("Обновить чертежи ({})", batch.done), &commands)?;
    }
    Ok(batch)
}

/// «Выдать» each up-to-date drawing that is not issued yet.
pub fn issue(editor: &mut Editor, ids: &BTreeSet<ObjectId>) -> Result<Batch, String> {
    let project = editor.document.project();
    let mut batch = Batch::default();
    let mut commands = Vec::new();
    // Already issued and unchanged since (an update clears the issue).
    for d in ordered(project, ids)
        .into_iter()
        .filter(|d| d.issued_revision.is_none())
    {
        match open_bim_drawings::issue(project, d) {
            Ok(issued) if issued != *d => {
                commands.push(Command::ReplaceDrawing { drawing: issued })
            }
            Ok(_) => {}
            Err(e) => batch.skipped.push(format!("{}: {e}", d.number)),
        }
    }
    batch.done = commands.len();
    if !commands.is_empty() {
        editor.commit_data(&format!("Выдать чертежи ({})", batch.done), &commands)?;
    }
    Ok(batch)
}

/// Delete the drawings (model objects are not touched).
pub fn delete(editor: &mut Editor, ids: &BTreeSet<ObjectId>) -> Result<Batch, String> {
    let project = editor.document.project();
    let commands: Vec<Command> = ordered(project, ids)
        .into_iter()
        .map(|d| Command::Delete {
            id: d.id,
            policy: DeletePolicy::RejectDependencies,
        })
        .collect();
    if !commands.is_empty() {
        editor.commit_data(&format!("Удалить чертежи ({})", commands.len()), &commands)?;
    }
    Ok(Batch {
        done: commands.len(),
        skipped: Vec::new(),
    })
}

/// One file per drawing, named by its designation.
pub fn export(
    project: &Project,
    ids: &BTreeSet<ObjectId>,
    format: ExportFormat,
    directory: &Path,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for d in ordered(project, ids) {
        let sheet = open_bim_drawings::generate(project, d)?;
        let path = directory.join(format!(
            "{}.{}",
            safe_file_name(&d.number),
            format.extension()
        ));
        let bytes = match format {
            ExportFormat::Pdf => open_bim_drawing_export::pdf::render(&[sheet], &d.number)?,
            ExportFormat::Svg => open_bim_drawing_export::svg(&sheet).into_bytes(),
            ExportFormat::Dxf => open_bim_drawing_export::dxf::render(&sheet).into_bytes(),
        };
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        files.push(path);
    }
    Ok(files)
}

/// All checked sheets in one PDF, in set and sheet order.
pub fn export_set_pdf(
    project: &Project,
    ids: &BTreeSet<ObjectId>,
    directory: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    let drawings = ordered(project, ids);
    if drawings.is_empty() {
        return Err("Отметьте чертежи".into());
    }
    let sheets = drawings
        .iter()
        .map(|d| open_bim_drawings::generate(project, d))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}.pdf", safe_file_name(name)));
    std::fs::write(&path, open_bim_drawing_export::pdf::render(&sheets, name)?)
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// Window state of the manager.
#[derive(Default)]
pub struct Manager {
    pub open: bool,
    pub text: String,
    pub kind: KindFilter,
    pub status: StatusFilter,
    pub checked: BTreeSet<ObjectId>,
    pub message: Option<String>,
    /// Marks by numbering, cached per model revision.
    marks: Option<(u64, BTreeMap<ObjectId, String>)>,
}

impl Manager {
    pub fn marks(&mut self, project: &Project) -> &BTreeMap<ObjectId, String> {
        if self
            .marks
            .as_ref()
            .is_none_or(|(r, _)| *r != project.revision)
        {
            let marks = open_bim_numbering::number(project)
                .map(|n| open_bim_numbering::stored_marks(&n))
                .unwrap_or_default();
            self.marks = Some((project.revision, marks));
        }
        &self.marks.as_ref().expect("filled above").1
    }

    /// Rows shown with the current filters.
    pub fn visible(&mut self, project: &Project) -> Vec<Row> {
        let marks = self.marks(project).clone();
        filtered(&rows(project, &marks), self.kind, self.status, &self.text)
    }

    /// Checked drawings that still exist.
    pub fn targets(&mut self, project: &Project) -> BTreeSet<ObjectId> {
        self.checked.retain(|id| project.drawings.contains_key(id));
        self.checked.clone()
    }
}

impl Manager {
    /// The window; returns a drawing to open in the drawing editor.
    pub fn show(
        &mut self,
        ctx: &eframe::egui::Context,
        editor: &mut Editor,
        export_dir: &str,
    ) -> Option<ObjectId> {
        use eframe::egui;
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut opened = None;
        egui::Window::new("Менеджер документов")
            .id(egui::Id::new("document manager"))
            .open(&mut open)
            .default_size([760., 460.])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let search = ui.label("Поиск");
                    ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(160.))
                        .labelled_by(search.id);
                    egui::ComboBox::from_id_salt("manager kind")
                        .selected_text(self.kind.label())
                        .show_ui(ui, |ui| {
                            for k in KindFilter::ALL {
                                ui.selectable_value(&mut self.kind, k, k.label());
                            }
                        });
                    egui::ComboBox::from_id_salt("manager status")
                        .selected_text(self.status.label())
                        .show_ui(ui, |ui| {
                            for s in StatusFilter::ALL {
                                ui.selectable_value(&mut self.status, s, s.label());
                            }
                        });
                });
                let rows = self.visible(editor.document.project());
                let total = editor.document.project().drawings.len();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "Показано {} из {total}, отмечено {}",
                        rows.len(),
                        self.checked.len()
                    ));
                    if ui.button("Отметить показанные").clicked() {
                        self.checked.extend(rows.iter().map(|r| r.id));
                    }
                    if ui.button("Снять отметки").clicked() {
                        self.checked.clear();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let any = !self.checked.is_empty();
                    let run = |ui: &mut egui::Ui, label: &str, tip: &str| {
                        ui.add_enabled(any, egui::Button::new(label))
                            .on_hover_text(tip)
                            .on_disabled_hover_text("Отметьте чертежи")
                            .clicked()
                    };
                    let dir = PathBuf::from(export_dir);
                    if run(ui, "Обновить по модели", "Перестроить устаревшие чертежи")
                    {
                        let ids = self.targets(editor.document.project());
                        self.message = Some(match update(editor, &ids) {
                            Ok(b) => b.message("Обновлено"),
                            Err(e) => e,
                        });
                    }
                    if run(ui, "Выдать", "Только актуальные чертежи") {
                        let ids = self.targets(editor.document.project());
                        self.message = Some(match issue(editor, &ids) {
                            Ok(b) => b.message("Выдано"),
                            Err(e) => e,
                        });
                    }
                    for (label, format) in [("PDF", ExportFormat::Pdf), ("DXF", ExportFormat::Dxf)]
                    {
                        if run(ui, label, "Файл на каждый чертёж") {
                            let ids = self.targets(editor.document.project());
                            self.message = Some(
                                match export(editor.document.project(), &ids, format, &dir) {
                                    Ok(files) => format!(
                                        "Файлов {label}: {} в {}",
                                        files.len(),
                                        dir.display()
                                    ),
                                    Err(e) => e,
                                },
                            );
                        }
                    }
                    if run(ui, "PDF комплекта", "Все отмеченные листы в одном файле")
                    {
                        let ids = self.targets(editor.document.project());
                        let project = editor.document.project();
                        let name = format!("{} комплект", project.name);
                        self.message = Some(match export_set_pdf(project, &ids, &dir, &name) {
                            Ok(path) => format!("Комплект: {}", path.display()),
                            Err(e) => e,
                        });
                    }
                    if run(ui, "Удалить", "Удалить отмеченные чертежи (⌘Z — вернуть)")
                    {
                        let ids = self.targets(editor.document.project());
                        self.message = Some(match delete(editor, &ids) {
                            Ok(b) => b.message("Удалено"),
                            Err(e) => e,
                        });
                        self.checked.clear();
                    }
                });
                if let Some(m) = &self.message {
                    ui.label(m);
                }
                ui.separator();
                let rows = self.visible(editor.document.project());
                if rows.is_empty() {
                    ui.label(if total == 0 {
                        "Чертежей нет: создайте их в режиме «Чертежи» или мастером"
                    } else {
                        "Нет чертежей для выбранных фильтров"
                    });
                }
                egui::ScrollArea::both()
                    .id_salt("manager rows")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("manager grid")
                            .striped(true)
                            .num_columns(10)
                            .show(ui, |ui| {
                                for title in [
                                    "",
                                    "Компл.",
                                    "Лист",
                                    "Обозначение",
                                    "Наименование",
                                    "Тип",
                                    "Марка",
                                    "Формат",
                                    "Масштаб",
                                    "Статус",
                                ] {
                                    ui.strong(title);
                                }
                                ui.end_row();
                                for row in &rows {
                                    let mut checked = self.checked.contains(&row.id);
                                    let toggle = ui.add(egui::Checkbox::without_text(&mut checked));
                                    toggle.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::WidgetType::Checkbox,
                                            true,
                                            checked,
                                            format!("Отметить {}", row.number),
                                        )
                                    });
                                    if toggle.changed() {
                                        if checked {
                                            self.checked.insert(row.id);
                                        } else {
                                            self.checked.remove(&row.id);
                                        }
                                    }
                                    ui.label(row.set);
                                    ui.label(row.sheet_no.to_string());
                                    if ui
                                        .link(&row.number)
                                        .on_hover_text("Открыть чертёж")
                                        .clicked()
                                    {
                                        opened = Some(row.id);
                                    }
                                    ui.label(&row.title);
                                    ui.label(row.kind);
                                    ui.label(&row.mark);
                                    ui.label(&row.format);
                                    ui.label(format!("1:{}", row.scale));
                                    let mut status = row.status.label();
                                    if let Some(r) = row.issued {
                                        status.push_str(&format!(" · выдан (рев. {r})"));
                                    }
                                    ui.colored_label(
                                        super::panel::status_color(&row.status, ui.visuals()),
                                        status,
                                    );
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.open = open;
        opened
    }
}
