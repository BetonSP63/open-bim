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
use crate::{ApiError, Member, Plate, Query, Session};
use open_bim_model::{ObjectId, Project, PropertyValue};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    /// Model file to open; a new model when absent.
    #[serde(default)]
    pub open: Option<String>,
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
    let mut selection: Vec<ObjectId> = Vec::new();
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

fn write(path: &Path, bytes: &[u8]) -> crate::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Io(format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(path, bytes).map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))
}

fn run_step(
    session: &mut Session,
    selection: &mut Vec<ObjectId>,
    step: &Step,
    base: &Path,
) -> crate::Result<StepReport> {
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
    })
}
