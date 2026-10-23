//! Drawings through the API, with the rules of an open drawing:
//! * a drawing is created by its type (part, assembly, general arrangement,
//!   general data) and gets its views from the drawing engine;
//! * its sheet content (designation, title, format, scale, notes) changes
//!   only while it is open; closing with saving stages the changes as one
//!   replacement, closing without saving drops them;
//! * while a drawing is open no other drawing is created or opened, and the
//!   open drawing is neither deleted, updated nor issued;
//! * update and issue work on closed drawings, the way the document manager
//!   handles whole sets.
use crate::{ApiError, Result, Session, invalid, take_id};
use open_bim_commands::{Command, DeletePolicy};
use open_bim_drawings::{Status, wizard};
use open_bim_model::{Drawing, DrawingKind, ObjectId, SheetSize, TextNote};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// A drawing as listed: identity, kind, sheet and state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DrawingInfo {
    pub id: ObjectId,
    pub kind: DrawingKind,
    pub number: String,
    pub title: String,
    pub sheet: SheetSize,
    pub scale: f64,
    /// «Актуален», «УСТАРЕЛ: …», «ПОТЕРЯНЫ ПРИВЯЗКИ: …».
    pub status: String,
    pub up_to_date: bool,
    pub issued: bool,
}

/// Changes to the open drawing; absent fields stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DrawingEdit {
    pub number: Option<String>,
    pub title: Option<String>,
    pub sheet: Option<SheetSize>,
    /// Scale denominator from the ГОСТ 2.302 series (1:N).
    pub scale: Option<f64>,
    /// Notes added to the sheet.
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub text: String,
    /// Paper mm from the lower-left corner of the sheet.
    pub position: [f64; 2],
    #[serde(default = "note_height")]
    pub height: f64,
}

fn note_height() -> f64 {
    3.5
}

/// Rules of the drawing wizard for batch runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DrawingRules {
    pub assemblies: bool,
    pub parts: bool,
    pub part_rule: wizard::PartRule,
    pub general_data: bool,
    /// Only the given parts (and their assemblies); empty = whole model.
    pub only: Vec<ObjectId>,
}

impl Default for DrawingRules {
    fn default() -> Self {
        let s = wizard::Settings::default();
        Self {
            assemblies: s.assemblies,
            parts: s.parts,
            part_rule: s.part_rule,
            general_data: s.general_data,
            only: Vec::new(),
        }
    }
}

/// The drawing open for editing and whether it has unsaved changes.
#[derive(Clone, Debug)]
pub(crate) struct OpenDrawing {
    pub drawing: Drawing,
    pub changed: bool,
}

impl Session {
    fn drawing(&self, id: ObjectId) -> Result<&Drawing> {
        self.working.drawings.get(&id).ok_or(ApiError::NotFound(id))
    }

    fn refuse_open(&self, id: ObjectId, action: &str) -> Result<()> {
        match &self.open_drawing {
            Some(open) if open.drawing.id == id => invalid(format!(
                "Чертёж {} открыт: {action} после его закрытия",
                open.drawing.number
            )),
            _ => Ok(()),
        }
    }

    pub(crate) fn refuse_any_open(&self, action: &str) -> Result<()> {
        match &self.open_drawing {
            Some(open) => invalid(format!(
                "Открыт чертёж {}: закройте его, чтобы {action}",
                open.drawing.number
            )),
            None => Ok(()),
        }
    }

    /// All drawings, ascending by ID.
    pub fn drawings(&self) -> Vec<DrawingInfo> {
        let p = &self.working;
        p.drawings
            .values()
            .map(|d| {
                let status = open_bim_drawings::status(p, d);
                DrawingInfo {
                    id: d.id,
                    kind: d.kind.clone(),
                    number: d.number.clone(),
                    title: d.title.clone(),
                    sheet: d.sheet,
                    scale: d.scale,
                    up_to_date: status == Status::UpToDate,
                    status: status.label(),
                    issued: d.issued_revision.is_some(),
                }
            })
            .collect()
    }

    /// Stage a new drawing of the given type; `number` defaults to
    /// «КМД-<марка>» for part and assembly drawings.
    pub fn create_drawing(&mut self, kind: DrawingKind, number: Option<&str>) -> Result<ObjectId> {
        self.refuse_any_open("создать другой чертёж")?;
        let p = &self.working;
        let mut next = p.next_id;
        let id = take_id(&mut next);
        let mark = || -> Option<String> {
            let numbering = open_bim_numbering::number(p).ok()?;
            match &kind {
                DrawingKind::Part { part } => numbering.parts.get(part).cloned(),
                DrawingKind::Assembly { assembly } => numbering.assemblies.get(assembly).cloned(),
                _ => None,
            }
        };
        let number = match number.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => n.to_string(),
            None => format!("КМД-{}", mark().unwrap_or_else(|| id.0.to_string())),
        };
        let drawing = match kind {
            DrawingKind::Part { part } => open_bim_drawings::part_drawing(p, id, part, &number),
            DrawingKind::Assembly { assembly } => {
                open_bim_drawings::assembly_drawing(p, id, assembly, &number)
            }
            DrawingKind::General { objects } => {
                open_bim_drawings::general_drawing(p, id, objects, &number, SheetSize::A1)
            }
            DrawingKind::GeneralData { mark } => {
                open_bim_drawings::general_data_drawing(p, id, &mark)
            }
            DrawingKind::Multi { objects } => {
                open_bim_drawings::multi_drawing(p, id, objects, &number)
            }
        }
        .map_err(ApiError::Invalid)?;
        self.stage(vec![Command::CreateDrawing { drawing }])?;
        Ok(id)
    }

    /// The wizard: one drawing per part or assembly mark that has none yet.
    pub fn create_drawings(&mut self, rules: &DrawingRules) -> Result<Vec<ObjectId>> {
        self.refuse_any_open("создавать чертежи")?;
        let settings = wizard::Settings {
            selection_only: !rules.only.is_empty(),
            assemblies: rules.assemblies,
            parts: rules.parts,
            part_rule: rules.part_rule,
            general_data: rules.general_data,
            assembly_template: None,
            part_template: None,
        };
        let selected: BTreeSet<ObjectId> = rules.only.iter().copied().collect();
        let p = &self.working;
        let plan = wizard::plan(p, &selected, &settings).map_err(ApiError::Invalid)?;
        let drawings = wizard::build(p, &plan, None, None, p.next_id).map_err(ApiError::Invalid)?;
        if drawings.is_empty() {
            return invalid(wizard::empty_reason(&plan));
        }
        let ids = drawings.iter().map(|d| d.id).collect();
        self.stage(
            drawings
                .into_iter()
                .map(|drawing| Command::CreateDrawing { drawing })
                .collect(),
        )?;
        Ok(ids)
    }

    /// Open a drawing for editing (one at a time).
    pub fn open_drawing(&mut self, id: ObjectId) -> Result<()> {
        if self
            .open_drawing
            .as_ref()
            .is_some_and(|o| o.drawing.id == id)
        {
            return Ok(());
        }
        self.refuse_any_open("открыть другой")?;
        let drawing = self.drawing(id)?.clone();
        self.open_drawing = Some(OpenDrawing {
            drawing,
            changed: false,
        });
        Ok(())
    }

    pub fn active_drawing(&self) -> Option<ObjectId> {
        self.open_drawing.as_ref().map(|o| o.drawing.id)
    }

    /// Change the open drawing; refused when no drawing is open.
    pub fn edit_drawing(&mut self, edit: &DrawingEdit) -> Result<()> {
        let Some(open) = &self.open_drawing else {
            return invalid("Нет открытого чертежа: изменять можно только открытый чертёж");
        };
        let mut next = open.drawing.clone();
        if let Some(number) = &edit.number {
            next.number = number.clone();
        }
        if let Some(title) = &edit.title {
            next.title = title.clone();
        }
        if let Some(sheet) = edit.sheet {
            next.sheet = sheet;
            next.multiple = 1;
        }
        if let Some(scale) = edit.scale {
            next.scale = scale;
        }
        for note in &edit.notes {
            let id = next.texts.iter().map(|t| t.id).max().map_or(1, |m| m + 1);
            next.texts.push(TextNote {
                id,
                position: note.position,
                text: note.text.clone(),
                height: note.height,
            });
        }
        next.validate()?;
        let changed = next != open.drawing;
        if let Some(open) = &mut self.open_drawing {
            open.changed |= changed;
            open.drawing = next;
        }
        Ok(())
    }

    /// Close the open drawing; with `save` its changes are staged as one
    /// replacement. Returns whether anything was staged.
    pub fn close_drawing(&mut self, save: bool) -> Result<bool> {
        let open = self
            .open_drawing
            .take()
            .ok_or_else(|| ApiError::Invalid("Нет открытого чертежа".into()))?;
        if !save || !open.changed {
            return Ok(false);
        }
        let result = self.stage(vec![Command::ReplaceDrawing {
            drawing: open.drawing.clone(),
        }]);
        if result.is_err() {
            self.open_drawing = Some(open);
        }
        result.map(|()| true)
    }

    pub fn delete_drawing(&mut self, id: ObjectId) -> Result<()> {
        self.drawing(id)?;
        self.refuse_open(id, "удалить его можно")?;
        self.stage(vec![Command::Delete {
            id,
            policy: DeletePolicy::RejectDependencies,
        }])
    }

    fn drawing_targets(&self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
        if ids.is_empty() {
            return Ok(self.working.drawings.keys().copied().collect());
        }
        for id in ids {
            self.drawing(*id)?;
        }
        Ok(ids.to_vec())
    }

    /// Regenerate the given drawings (empty = all outdated ones) from the
    /// current model; returns the updated IDs.
    pub fn update_drawings(&mut self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
        let all = ids.is_empty();
        let mut commands = Vec::new();
        let mut updated = Vec::new();
        for id in self.drawing_targets(ids)? {
            let d = &self.working.drawings[&id];
            if all && open_bim_drawings::status(&self.working, d) == Status::UpToDate {
                continue;
            }
            self.refuse_open(id, "обновить его можно")?;
            commands.push(Command::ReplaceDrawing {
                drawing: open_bim_drawings::updated(&self.working, d),
            });
            updated.push(id);
        }
        if !commands.is_empty() {
            self.stage(commands)?;
        }
        Ok(updated)
    }

    /// Issue the given drawings (empty = all up-to-date, not yet issued);
    /// an outdated drawing or one with lost bindings is refused.
    pub fn issue_drawings(&mut self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
        let all = ids.is_empty();
        let mut commands = Vec::new();
        let mut issued = Vec::new();
        for id in self.drawing_targets(ids)? {
            let d = &self.working.drawings[&id];
            if all
                && (d.issued_revision.is_some()
                    || open_bim_drawings::status(&self.working, d) != Status::UpToDate)
            {
                continue;
            }
            self.refuse_open(id, "выдать его можно")?;
            commands.push(Command::ReplaceDrawing {
                drawing: open_bim_drawings::issue(&self.working, d).map_err(ApiError::Invalid)?,
            });
            issued.push(id);
        }
        if !commands.is_empty() {
            self.stage(commands)?;
        }
        Ok(issued)
    }

    fn sheets(&self, ids: &[ObjectId]) -> Result<Vec<(String, open_bim_drawings::Sheet)>> {
        let p = self.for_export()?;
        let ids = if ids.is_empty() {
            p.drawings.keys().copied().collect()
        } else {
            ids.to_vec()
        };
        if ids.is_empty() {
            return invalid("В модели нет чертежей");
        }
        ids.iter()
            .map(|id| {
                let d = p.drawings.get(id).ok_or(ApiError::NotFound(*id))?;
                let sheet = open_bim_drawings::generate(p, d).map_err(ApiError::Invalid)?;
                Ok((d.number.clone(), sheet))
            })
            .collect()
    }

    /// One PDF with the given drawings (empty = all) of the committed model.
    pub fn export_pdf(&self, path: &Path, ids: &[ObjectId]) -> Result<usize> {
        let sheets: Vec<_> = self.sheets(ids)?.into_iter().map(|(_, s)| s).collect();
        let bytes = open_bim_drawing_export::pdf::render(&sheets, &self.committed().name)
            .map_err(ApiError::Io)?;
        crate::batch::write(path, &bytes)?;
        Ok(sheets.len())
    }

    /// One DXF per drawing, named by its designation.
    pub fn export_dxf(&self, dir: &Path, ids: &[ObjectId]) -> Result<usize> {
        let sheets = self.sheets(ids)?;
        for (number, sheet) in &sheets {
            let name: String = number
                .chars()
                .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
                .collect();
            crate::batch::write(
                &dir.join(format!("{name}.dxf")),
                open_bim_drawing_export::dxf::render(sheet).as_bytes(),
            )?;
        }
        Ok(sheets.len())
    }
}
