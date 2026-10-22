//! Batch runs without the interface: a JSON macro opens (or creates) a
//! model, stages object changes, commits, exports and saves.
//!
//! ```json
//! { "open": "frame.obim",
//!   "steps": [
//!     { "insert_member": { "profile": "30Б1 (Р 57837)", "material": "С255",
//!                          "start": [0, 0, 3000], "end": [6000, 0, 3000] } },
//!     { "copy": { "vector": [0, 6000, 0], "count": 3 } },
//!     { "commit": "Балки перекрытия" },
//!     { "export_ifc": "frame.ifc" },
//!     { "save": "frame.obim" } ] }
//! ```
//!
//! Every insert, copy and `select` replaces the current selection; steps
//! that take a `target` act on it unless another target is given. Relative
//! paths are taken from the macro's folder. The first failing step stops
//! the run; nothing staged after the last commit is saved.
use crate::{ApiError, DrawingEdit, DrawingRules, Member, Plate, Query, Session};
use open_bim_model::{DrawingKind, ObjectId, Project, PropertyValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    /// Model file to open; a new model when absent.
    #[serde(default)]
    pub open: Option<String>,
    /// Folders with plugins; `plugins` next to the macro when empty.
    #[serde(default)]
    pub plugins: Vec<String>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    /// Result of the last insert, copy or select.
    #[default]
    Selection,
    Ids(Vec<ObjectId>),
    Query(Box<Query>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    InsertMember(Member),
    InsertPlate(Plate),
    ModifyMember(Member),
    ModifyPlate(Plate),
    Select(Query),
    Delete {
        #[serde(default)]
        target: Target,
        #[serde(default)]
        cascade: bool,
    },
    SetProperty {
        #[serde(default)]
        target: Target,
        key: String,
        /// Absent removes the property.
        #[serde(default)]
        value: Option<PropertyValue>,
    },
    Move {
        #[serde(default)]
        target: Target,
        vector: [f64; 3],
    },
    /// `count` copies, each shifted by `vector` from the previous one.
    Copy {
        #[serde(default)]
        target: Target,
        vector: [f64; 3],
        #[serde(default = "one")]
        count: u32,
    },
    Commit(String),
    Rollback,
    Undo,
    /// Save to the given file, or back to the opened one.
    Save(Option<String>),
    ExportIfc(String),
    /// Folder for one `.nc1` file per part mark.
    ExportNc(String),
    /// Folder for the part, assembly and fastener lists (CSV).
    ExportReports(String),
    /// A drawing of one type; it becomes the current drawing selection.
    CreateDrawing(NewDrawing),
    /// Drawings by the wizard rules, one per mark without a drawing.
    CreateDrawings(DrawingRules),
    /// Open a drawing for editing; without an ID, the last created one.
    OpenDrawing(Option<ObjectId>),
    /// Change the open drawing.
    EditDrawing(DrawingEdit),
    /// Close the open drawing, saving its changes (`true`) or not.
    CloseDrawing(bool),
    DeleteDrawing(ObjectId),
    /// Regenerate drawings from the model (empty = all outdated).
    UpdateDrawings(Vec<ObjectId>),
    /// Issue drawings (empty = all up-to-date, not yet issued).
    IssueDrawings(Vec<ObjectId>),
    /// One PDF of the drawings (empty `drawings` = all).
    ExportPdf {
        path: String,
        #[serde(default)]
        drawings: Vec<ObjectId>,
    },
    /// Run a plugin by name with picked inputs and attribute values;
    /// the created objects become the selection.
    RunPlugin {
        plugin: String,
        #[serde(default)]
        inputs: BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        attributes: BTreeMap<String, serde_json::Value>,
    },
    /// Folder with one DXF per drawing (empty `drawings` = all).
    ExportDxf {
        dir: String,
        #[serde(default)]
        drawings: Vec<ObjectId>,
    },
}

/// A drawing of one type: exactly one of `part`, `assembly`, `general`
/// (parts of a general arrangement, empty = whole model) or `general_data`
/// (set mark, «КМД» or «КМ»).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NewDrawing {
    pub part: Option<ObjectId>,
    pub assembly: Option<ObjectId>,
    pub general: Option<Vec<ObjectId>>,
    pub general_data: Option<String>,
    pub number: Option<String>,
}

impl NewDrawing {
    pub fn kind(&self) -> crate::Result<DrawingKind> {
        let kinds = [
            self.part.map(|part| DrawingKind::Part { part }),
            self.assembly
                .map(|assembly| DrawingKind::Assembly { assembly }),
            self.general
                .clone()
                .map(|objects| DrawingKind::General { objects }),
            self.general_data
                .clone()
                .map(|mark| DrawingKind::GeneralData { mark }),
        ];
        let mut set = kinds.into_iter().flatten();
        match (set.next(), set.next()) {
            (Some(kind), None) => Ok(kind),
            _ => Err(ApiError::Invalid(
                "Чертёж: укажите ровно один тип — part, assembly, general или general_data".into(),
            )),
        }
    }
}

fn one() -> u32 {
    1
}

/// Outcome of one step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepReport {
    pub step: usize,
    pub action: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<ObjectId>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    pub steps: Vec<StepReport>,
    /// Committed model revision at the end.
    pub revision: u64,
    pub parts: usize,
    /// Changes left staged at the end (not saved).
    pub staged: usize,
}

/// Failure of a run: the step (from 1) and why.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchError {
    pub step: usize,
    pub error: ApiError,
}

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.step == 0 {
            write!(f, "{}", self.error)
        } else {
            write!(f, "Шаг {}: {}", self.step, self.error)
        }
    }
}

impl std::error::Error for BatchError {}

pub fn parse(text: &str) -> Result<Macro, BatchError> {
    serde_json::from_str(text).map_err(|e| BatchError {
        step: 0,
        error: ApiError::Invalid(format!("Макрос: {e}")),
    })
}

/// Run a macro; relative paths are resolved against `base`.
pub fn run(script: &Macro, base: &Path) -> Result<(Session, Report), BatchError> {
    let at = |step: usize| move |error: ApiError| BatchError { step, error };
    let mut session = match &script.open {
        Some(path) => Session::open(resolve(base, path)).map_err(at(0))?,
        None => Session::new(Project::default()).map_err(at(0))?,
    };
    let dirs: Vec<PathBuf> = if script.plugins.is_empty() {
        vec![base.join("plugins")]
    } else {
        script.plugins.iter().map(|d| resolve(base, d)).collect()
    };
    let mut selection = Selection {
        plugins: crate::plugins::discover(&dirs).0,
        ..Default::default()
    };
    let mut steps = Vec::new();
    for (index, step) in script.steps.iter().enumerate() {
        let report = run_step(&mut session, &mut selection, step, base).map_err(at(index + 1))?;
        steps.push(StepReport {
            step: index + 1,
            ..report
        });
    }
    let report = Report {
        steps,
        revision: session.committed().revision,
        parts: session.committed().elements.len(),
        staged: session.staged(),
    };
    Ok((session, report))
}

fn resolve(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn targets(
    session: &Session,
    selection: &[ObjectId],
    target: &Target,
) -> crate::Result<Vec<ObjectId>> {
    let ids = match target {
        Target::Selection => selection.to_vec(),
        Target::Ids(ids) => ids.clone(),
        Target::Query(query) => session.query(query)?,
    };
    if ids.is_empty() {
        return Err(ApiError::Invalid("Нет выбранных объектов".into()));
    }
    Ok(ids)
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> crate::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Io(format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(path, bytes).map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))
}

/// Current parts and the drawings created last.
#[derive(Default)]
struct Selection {
    parts: Vec<ObjectId>,
    drawings: Vec<ObjectId>,
    plugins: Vec<crate::plugins::Plugin>,
}

/// Steps a plugin answered with, on a fresh selection; returns the IDs of
/// what they created.
pub(crate) fn run_plugin_steps(
    session: &mut Session,
    steps: &[Step],
    base: &Path,
) -> crate::Result<Vec<ObjectId>> {
    let mut current = Selection::default();
    let mut created = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let report = run_step(session, &mut current, step, base).map_err(|e| match e {
            ApiError::Invalid(m) => ApiError::Invalid(format!("шаг {}: {m}", index + 1)),
            other => other,
        })?;
        if matches!(
            step,
            Step::InsertMember(_)
                | Step::InsertPlate(_)
                | Step::Copy { .. }
                | Step::CreateDrawing(_)
        ) {
            created.extend(report.ids);
        }
    }
    Ok(created)
}

fn run_step(
    session: &mut Session,
    current: &mut Selection,
    step: &Step,
    base: &Path,
) -> crate::Result<StepReport> {
    let selection = &mut current.parts;
    let report = |action: &'static str, ids: Vec<ObjectId>, message: String| StepReport {
        step: 0,
        action,
        ids,
        message,
    };
    Ok(match step {
        Step::InsertMember(member) => {
            let id = session.insert_member(&mut member.clone())?;
            *selection = vec![id];
            report("insert_member", vec![id], String::new())
        }
        Step::InsertPlate(plate) => {
            let id = session.insert_plate(&mut plate.clone())?;
            *selection = vec![id];
            report("insert_plate", vec![id], String::new())
        }
        Step::ModifyMember(member) => {
            session.modify_member(member)?;
            report(
                "modify_member",
                member.id.into_iter().collect(),
                String::new(),
            )
        }
        Step::ModifyPlate(plate) => {
            session.modify_plate(plate)?;
            report(
                "modify_plate",
                plate.id.into_iter().collect(),
                String::new(),
            )
        }
        Step::Select(query) => {
            *selection = session.query(query)?;
            report("select", selection.clone(), String::new())
        }
        Step::Delete { target, cascade } => {
            let ids = targets(session, selection, target)?;
            for id in &ids {
                session.delete(*id, *cascade)?;
            }
            selection.retain(|id| !ids.contains(id));
            report("delete", ids, String::new())
        }
        Step::SetProperty { target, key, value } => {
            let ids = targets(session, selection, target)?;
            for id in &ids {
                session.set_property(*id, key, value.clone())?;
            }
            report("set_property", ids, String::new())
        }
        Step::Move { target, vector } => {
            let ids = targets(session, selection, target)?;
            session.move_by(&ids, *vector)?;
            report("move", ids, String::new())
        }
        Step::Copy {
            target,
            vector,
            count,
        } => {
            if !(1..=1000).contains(count) {
                return Err(ApiError::Invalid("Число копий: от 1 до 1000".into()));
            }
            let ids = targets(session, selection, target)?;
            let mut copies = Vec::new();
            for n in 1..=*count {
                let shift = vector.map(|v| v * f64::from(n));
                copies.extend(session.copy_by(&ids, shift)?);
            }
            *selection = copies.clone();
            report("copy", copies, String::new())
        }
        Step::Commit(label) => {
            let message = match session.commit(label)? {
                Some(event) => format!("Ревизия {}", event.revision),
                None => "Нет изменений".into(),
            };
            report("commit", Vec::new(), message)
        }
        Step::Rollback => {
            let n = session.rollback();
            selection.retain(|id| session.project().contains_id(*id));
            report("rollback", Vec::new(), format!("Отменено изменений: {n}"))
        }
        Step::Undo => {
            let event = session.undo()?;
            selection.retain(|id| session.project().contains_id(*id));
            report("undo", Vec::new(), format!("Ревизия {}", event.revision))
        }
        Step::Save(path) => {
            let path = path.as_deref().map(|p| resolve(base, p));
            let written = session.save(path.as_deref())?;
            report("save", Vec::new(), written.display().to_string())
        }
        Step::ExportIfc(path) => {
            let (text, summary) =
                open_bim_ifc::export(session.for_export()?).map_err(ApiError::Io)?;
            let path = resolve(base, path);
            write(&path, text.as_bytes())?;
            report(
                "export_ifc",
                Vec::new(),
                format!(
                    "{}: балок {}, колонн {}, пластин {}",
                    path.display(),
                    summary.beams,
                    summary.columns,
                    summary.plates
                ),
            )
        }
        Step::ExportNc(dir) => {
            let files = open_bim_nc::export(session.for_export()?).map_err(ApiError::Io)?;
            let dir = resolve(base, dir);
            for file in &files {
                write(&dir.join(&file.name), file.text.as_bytes())?;
            }
            report(
                "export_nc",
                Vec::new(),
                format!("{}: файлов {}", dir.display(), files.len()),
            )
        }
        Step::ExportReports(dir) => {
            use open_bim_reports::csv;
            let reports =
                open_bim_reports::generate(session.for_export()?).map_err(ApiError::Io)?;
            let dir = resolve(base, dir);
            for (name, text) in [
                ("parts.csv", csv::parts(&reports)),
                ("assemblies.csv", csv::assemblies(&reports)),
                ("fasteners.csv", csv::fasteners(&reports)),
            ] {
                write(&dir.join(name), text.as_bytes())?;
            }
            report(
                "export_reports",
                Vec::new(),
                format!(
                    "{}: масса деталей {} кг",
                    dir.display(),
                    open_bim_reports::fmt(reports.total_part_mass_kg)
                ),
            )
        }
        Step::CreateDrawing(new) => {
            let id = session.create_drawing(new.kind()?, new.number.as_deref())?;
            current.drawings = vec![id];
            report("create_drawing", vec![id], String::new())
        }
        Step::CreateDrawings(rules) => {
            let ids = session.create_drawings(rules)?;
            current.drawings = ids.clone();
            report("create_drawings", ids, String::new())
        }
        Step::OpenDrawing(id) => {
            let id = match id {
                Some(id) => *id,
                None => *current.drawings.last().ok_or_else(|| {
                    ApiError::Invalid("Не указан чертёж и нет созданных в этом макросе".into())
                })?,
            };
            session.open_drawing(id)?;
            report("open_drawing", vec![id], String::new())
        }
        Step::EditDrawing(edit) => {
            session.edit_drawing(edit)?;
            report(
                "edit_drawing",
                session.active_drawing().into_iter().collect(),
                String::new(),
            )
        }
        Step::CloseDrawing(save) => {
            let id = session.active_drawing();
            let staged = session.close_drawing(*save)?;
            let message = if staged {
                "Изменения чертежа в очереди"
            } else {
                "Без изменений"
            };
            report("close_drawing", id.into_iter().collect(), message.into())
        }
        Step::DeleteDrawing(id) => {
            session.delete_drawing(*id)?;
            current.drawings.retain(|d| d != id);
            report("delete_drawing", vec![*id], String::new())
        }
        Step::UpdateDrawings(ids) => {
            let ids = session.update_drawings(ids)?;
            report("update_drawings", ids, String::new())
        }
        Step::IssueDrawings(ids) => {
            let ids = session.issue_drawings(ids)?;
            report("issue_drawings", ids, String::new())
        }
        Step::ExportPdf { path, drawings } => {
            let path = resolve(base, path);
            let n = session.export_pdf(&path, drawings)?;
            report(
                "export_pdf",
                Vec::new(),
                format!("{}: листов {n}", path.display()),
            )
        }
        Step::ExportDxf { dir, drawings } => {
            let dir = resolve(base, dir);
            let n = session.export_dxf(&dir, drawings)?;
            report(
                "export_dxf",
                Vec::new(),
                format!("{}: файлов {n}", dir.display()),
            )
        }
        Step::RunPlugin {
            plugin,
            inputs,
            attributes,
        } => {
            let found = current
                .plugins
                .iter()
                .find(|p| p.manifest.name == *plugin)
                .cloned()
                .ok_or_else(|| ApiError::Invalid(format!("Плагин «{plugin}» не найден")))?;
            let mut picked = BTreeMap::new();
            for def in &found.manifest.inputs {
                if let Some(value) = inputs.get(&def.name) {
                    picked.insert(
                        def.name.clone(),
                        crate::plugins::InputValue::from_json(def.kind, value)?,
                    );
                }
            }
            if let Some(extra) = inputs.keys().find(|k| !picked.contains_key(*k)) {
                return Err(ApiError::Invalid(format!(
                    "Плагин «{plugin}» не знает входа «{extra}»"
                )));
            }
            let ids = session.run_plugin(&found, &picked, attributes)?;
            *selection = ids.clone();
            report("run_plugin", ids, String::new())
        }
    })
}
