//! «Преобразовать объекты IFC»: read an IFC file, see what it holds by
//! kind, and turn the chosen kinds into native parts of the current model.
//! Objects whose IFC GUID the model already has are skipped, so a revised
//! file converts only what is new.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_ifc::GUID_PROPERTY;
use open_bim_model::{Element, ElementShape, MemberKind, ObjectId, Project, PropertyValue};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Kinds of converted objects, in the order of the window.
pub const KINDS: [&str; 4] = ["Колонны", "Балки", "Связи и элементы", "Пластины"];

pub fn kind(e: &Element) -> &'static str {
    let ifc_member = matches!(
        e.properties.get(open_bim_ifc::IFC_ENTITY_PROPERTY),
        Some(PropertyValue::Text(t)) if t.eq_ignore_ascii_case("IfcMember")
    );
    match &e.shape {
        ElementShape::Plate { .. } => KINDS[3],
        _ if ifc_member => KINDS[2],
        ElementShape::Member {
            member_kind: MemberKind::Column,
            ..
        } => KINDS[0],
        ElementShape::Member { .. } => KINDS[1],
    }
}

fn guid(e: &Element) -> Option<&str> {
    match e.properties.get(GUID_PROPERTY) {
        Some(PropertyValue::Text(g)) => Some(g),
        _ => None,
    }
}

/// Per kind: (objects in the file, of them already in the model).
pub fn census(target: &Project, imported: &Project) -> BTreeMap<&'static str, (usize, usize)> {
    let known: BTreeSet<&str> = target.elements.values().filter_map(guid).collect();
    let mut out = BTreeMap::new();
    for e in imported.elements.values() {
        let row = out.entry(kind(e)).or_insert((0, 0));
        row.0 += 1;
        if guid(e).is_some_and(|g| known.contains(g)) {
            row.1 += 1;
        }
    }
    out
}

/// `imported` reduced to the elements `keep` accepts, with their cuts and
/// the assemblies whose main part stays.
fn only(imported: &Project, keep: impl Fn(&Element) -> bool) -> Project {
    let mut p = imported.clone();
    p.elements.retain(|_, e| keep(e));
    p.details.retain(|_, d| {
        d.kind
            .references()
            .iter()
            .all(|r| p.elements.contains_key(r))
    });
    p.assemblies
        .retain(|_, a| p.elements.contains_key(&a.main_part));
    let assemblies: BTreeSet<ObjectId> = p.assemblies.keys().copied().collect();
    for a in p.assemblies.values_mut() {
        a.members.retain(|m| p.elements.contains_key(m));
        a.children.retain(|c| assemblies.contains(c));
    }
    p
}

/// What a conversion adds.
#[derive(Debug, Default)]
pub struct Conversion {
    pub commands: Vec<Command>,
    /// The new parts, to select after the commit.
    pub parts: BTreeSet<ObjectId>,
    /// Objects skipped because their GUID is already in the model.
    pub skipped: usize,
}

/// Commands converting the objects of `kinds` from `imported` into `target`.
pub fn convert(
    target: &Project,
    imported: &Project,
    kinds: &BTreeSet<&str>,
) -> Result<Conversion, String> {
    if kinds.is_empty() {
        return Err("Отметьте, что преобразовать".into());
    }
    let known: BTreeSet<&str> = target.elements.values().filter_map(guid).collect();
    let chosen = |e: &Element| kinds.contains(kind(e));
    let skipped = imported
        .elements
        .values()
        .filter(|e| chosen(e) && guid(e).is_some_and(|g| known.contains(g)))
        .count();
    let part = only(imported, |e| {
        chosen(e) && !guid(e).is_some_and(|g| known.contains(g))
    });
    if part.elements.is_empty() {
        return Err(if skipped > 0 {
            format!("Все отмеченные объекты уже в модели ({skipped})")
        } else {
            "В файле нет объектов отмеченных видов".into()
        });
    }
    let merge = open_bim_ifc::merge::merge(target, &part);
    let parts = merge.elements.iter().map(|e| e.id).collect();
    Ok(Conversion {
        commands: crate::tools::merge_commands(&merge),
        parts,
        skipped,
    })
}

/// The IFC file read into the window.
pub struct Source {
    pub path: PathBuf,
    pub project: Project,
    pub summary: String,
}

pub fn read(path: &Path) -> Result<Source, String> {
    let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if bytes > open_bim_ifc::step::MAX_FILE_BYTES as u64 {
        return Err("IFC: файл слишком большой".into());
    }
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    let (project, report) = open_bim_ifc::import(&String::from_utf8_lossy(&raw))?;
    Ok(Source {
        path: path.to_path_buf(),
        project,
        summary: report.summary(),
    })
}

#[derive(Default)]
pub struct IfcConvert {
    pub open: bool,
    pub source: Option<Source>,
    /// Kinds left unchecked (all are converted by default).
    pub off: BTreeSet<&'static str>,
    pub message: Option<String>,
}

impl IfcConvert {
    /// The window; returns the label, commands and new parts to commit.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        project_path: &str,
    ) -> Option<(String, Conversion)> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let (mut browse, mut run) = (false, false);
        egui::Window::new("Преобразовать объекты IFC")
            .id(egui::Id::new("ifc convert"))
            .open(&mut open)
            .collapsible(false)
            .default_width(440.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    browse = ui.button("Файл IFC…").clicked();
                    match &self.source {
                        Some(s) => ui.label(
                            s.path
                                .file_name()
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                        ),
                        None => ui.weak("не выбран"),
                    };
                });
                if let Some(s) = &self.source {
                    ui.small(&s.summary);
                    let census = census(project, &s.project);
                    egui::Grid::new("ifc convert grid")
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Преобразовать");
                            ui.strong("В файле");
                            ui.strong("Уже в модели");
                            ui.end_row();
                            for k in KINDS {
                                let Some((all, known)) = census.get(k) else {
                                    continue;
                                };
                                let mut on = !self.off.contains(k);
                                if ui.checkbox(&mut on, k).changed() {
                                    if on {
                                        self.off.remove(k);
                                    } else {
                                        self.off.insert(k);
                                    }
                                }
                                ui.label(all.to_string());
                                ui.label(known.to_string());
                                ui.end_row();
                            }
                        });
                    run = ui.button("Преобразовать").clicked();
                }
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        if browse {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Файл IFC для преобразования")
                .add_filter("IFC", &["ifc", "IFC"]);
            if let Some(folder) = Path::new(project_path).parent().filter(|p| p.is_dir()) {
                dialog = dialog.set_directory(folder);
            }
            if let Some(path) = dialog.pick_file() {
                match read(&path) {
                    Ok(s) => {
                        self.source = Some(s);
                        self.message = None;
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        }
        if !run {
            return None;
        }
        let s = self.source.as_ref()?;
        let kinds: BTreeSet<&str> = KINDS
            .into_iter()
            .filter(|k| !self.off.contains(k))
            .collect();
        match convert(project, &s.project, &kinds) {
            Ok(c) => {
                let mut text = format!("Преобразовано деталей: {}", c.parts.len());
                if c.skipped > 0 {
                    text.push_str(&format!(", уже были в модели: {}", c.skipped));
                }
                self.message = Some(text);
                Some(("Преобразовать объекты IFC".into(), c))
            }
            Err(e) => {
                self.message = Some(e);
                None
            }
        }
    }
}
