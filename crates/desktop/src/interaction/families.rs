//! Families in the app: the «Семейства» window lists the families of the
//! project, of the `families` folder next to the project file and the
//! ready samples; «Разместить» places the chosen type by one or two points,
//! and the selected family object's type and instance values are changed
//! in the same window.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    Family, FamilyInstance, Frame, ObjectId, ParamKind, Placement, Point3, Project, sample_families,
};
use open_bim_renderer::DVec3;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const FOLDER: &str = "families";
const EXTENSION: &str = "family.json";
const MAX_FILE: usize = 1 << 20;

/// `families` folder next to the project file.
pub fn folder(project_path: &str) -> PathBuf {
    match Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(FOLDER),
        _ => PathBuf::from(FOLDER),
    }
}

fn file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    format!("{safe}.{EXTENSION}")
}

/// Families saved in `dir` sorted by name, and a message per file that
/// could not be read.
pub fn load_all(dir: &Path) -> (Vec<Family>, Vec<String>) {
    let suffix = format!(".{EXTENSION}");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .collect();
    files.sort();
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for file in files {
        match read(&file) {
            Ok(f) => found.push(f),
            Err(e) => errors.push(format!(
                "{}: {e}",
                file.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    (found, errors)
}

fn read(file: &Path) -> Result<Family, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|e| e.to_string())?
        .take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("файл слишком большой".into());
    }
    let family: Family = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    family.validate().map_err(|e| e.to_string())?;
    Ok(family)
}

/// Write `family` to `dir`, replacing a file of the same name.
pub fn save(dir: &Path, family: &Family) -> Result<PathBuf, String> {
    family.validate().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(file_name(&family.name));
    let json = serde_json::to_string_pretty(family).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Where a catalogue family comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Project,
    Folder,
    Sample,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "в проекте",
            Self::Folder => "папка семейств",
            Self::Sample => "образец",
        }
    }
}

/// The catalogue: project families first, then the folder's, then the
/// samples; a name shows once.
pub fn catalogue(project: &Project, folder: &[Family]) -> Vec<(Family, Source)> {
    let mut out: Vec<(Family, Source)> = project
        .families
        .values()
        .map(|f| (f.clone(), Source::Project))
        .collect();
    let mut names: BTreeSet<String> = project.families.keys().cloned().collect();
    for (f, source) in folder
        .iter()
        .cloned()
        .map(|f| (f, Source::Folder))
        .chain(sample_families().into_iter().map(|f| (f, Source::Sample)))
    {
        if names.insert(f.name.clone()) {
            out.push((f, source));
        }
    }
    out
}

/// Place `family` of `type_name` at `start` (and `end` for a two-point
/// family); a family not yet in the project is added in the same step.
pub fn place(
    project: &Project,
    family: &Family,
    type_name: &str,
    start: DVec3,
    end: Option<DVec3>,
) -> Result<Vec<Command>, String> {
    let mut out = Vec::new();
    match project.families.get(&family.name) {
        Some(f) if f != family => {
            return Err(format!(
                "В проекте уже другое семейство «{}»: обновите его в окне «Семейства»",
                family.name
            ));
        }
        Some(_) => {}
        None => out.push(Command::SetFamily {
            family: Box::new(family.clone()),
        }),
    }
    let start = Point3(start.to_array());
    let (frame, length) = match (family.placement, end) {
        (Placement::Point, _) => (
            Frame {
                origin: start,
                ..Frame::default()
            },
            0.,
        ),
        (Placement::TwoPoints, Some(end)) => {
            FamilyInstance::between(start, Point3(end.to_array())).map_err(|e| e.to_string())?
        }
        (Placement::TwoPoints, None) => return Err("Укажите вторую точку".into()),
    };
    out.push(Command::CreateFamilyInstance {
        instance: FamilyInstance {
            id: ObjectId(project.next_id),
            family: family.name.clone(),
            type_name: type_name.into(),
            frame,
            length,
            values: BTreeMap::new(),
            children: BTreeMap::new(),
        },
    });
    Ok(out)
}

/// The family object one of the selected objects belongs to.
pub fn instance_of<'a>(
    project: &'a Project,
    selected: &BTreeSet<ObjectId>,
) -> Option<&'a FamilyInstance> {
    project
        .family_instances
        .values()
        .find(|i| selected.contains(&i.id) || i.children.values().any(|id| selected.contains(id)))
}

/// Commands for a family object with a new type and instance values;
/// values equal to the family default are dropped.
pub fn change(
    project: &Project,
    id: ObjectId,
    type_name: &str,
    values: &BTreeMap<String, String>,
) -> Result<Vec<Command>, String> {
    let mut instance = project
        .family_instances
        .get(&id)
        .ok_or("Объект семейства не найден")?
        .clone();
    let family = project
        .families
        .get(&instance.family)
        .ok_or("Семейство не найдено")?;
    instance.type_name = type_name.into();
    instance.values = values
        .iter()
        .filter(|(k, v)| {
            family
                .parameter(k)
                .is_some_and(|p| !p.of_type && p.formula.is_empty() && p.value != **v)
        })
        .map(|(k, v)| (k.clone(), v.trim().to_string()))
        .collect();
    Ok(vec![Command::ReplaceFamilyInstance { instance }])
}

/// What the «Семейства» window asks for.
pub enum FamiliesAction {
    Commit(String, Vec<Command>),
    /// Pick the point (or two points) of the current family.
    Place {
        two_points: bool,
    },
    Message(String),
    /// Open a copy in «Редактор семейств».
    Edit(Family),
}

/// Values being edited for the selected family object.
struct Editing {
    id: ObjectId,
    type_name: String,
    values: BTreeMap<String, String>,
}

#[derive(Default)]
pub struct FamiliesWindow {
    pub open: bool,
    /// Family and type placed by «Разместить».
    pub current: Option<(Family, String)>,
    folder: Vec<Family>,
    errors: Vec<String>,
    loaded_from: Option<PathBuf>,
    editing: Option<Editing>,
}

impl FamiliesWindow {
    /// Families of the catalogue as last shown (project, folder, samples).
    pub fn library(&self, project: &Project) -> Vec<Family> {
        catalogue(project, &self.folder)
            .into_iter()
            .map(|(f, _)| f)
            .collect()
    }

    /// Read the folder again on the next show.
    pub fn reload(&mut self) {
        self.loaded_from = None;
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        project_path: &str,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<FamiliesAction> {
        if !self.open {
            return None;
        }
        let dir = folder(project_path);
        if self.loaded_from.as_ref() != Some(&dir) {
            (self.folder, self.errors) = load_all(&dir);
            self.loaded_from = Some(dir.clone());
        }
        let list = catalogue(project, &self.folder);
        let picked = self.current.as_ref().map(|(f, _)| f.name.clone());
        match list.iter().find(|(f, _)| Some(&f.name) == picked.as_ref()) {
            Some((f, _)) => {
                if let Some((current, type_name)) = &mut self.current {
                    *current = f.clone();
                    if f.family_type(type_name).is_none() {
                        *type_name = default_type(f);
                    }
                }
            }
            None => {
                self.current = list.first().map(|(f, _)| (f.clone(), default_type(f)));
            }
        }
        let mut action = None;
        let mut open = true;
        egui::Window::new("Семейства")
            .id(egui::Id::new("families"))
            .open(&mut open)
            .default_width(420.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Каталог");
                    if ui.small_button("Новое семейство").clicked() {
                        let name = (1..)
                            .map(|k| format!("Семейство {k}"))
                            .find(|n| !list.iter().any(|(f, _)| &f.name == n))
                            .unwrap_or_default();
                        action = Some(FamiliesAction::Edit(super::family_editor::blank(&name)));
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("families-list")
                    .max_height(180.)
                    .show(ui, |ui| {
                        for (f, source) in &list {
                            let on = picked.as_deref() == Some(f.name.as_str());
                            let text = format!("{} — {}", f.name, source.label());
                            if ui.selectable_label(on, text).clicked() {
                                self.current = Some((f.clone(), default_type(f)));
                            }
                        }
                    });
                for e in &self.errors {
                    ui.colored_label(egui::Color32::from_rgb(190, 60, 40), e);
                }
                if let Some((f, type_name)) = &mut self.current {
                    ui.separator();
                    if !f.description.is_empty() {
                        ui.label(&f.description);
                    }
                    if !f.types.is_empty() {
                        egui::ComboBox::from_label("Типоразмер")
                            .selected_text(type_name.as_str())
                            .show_ui(ui, |ui| {
                                for t in &f.types {
                                    ui.selectable_value(type_name, t.name.clone(), &t.name);
                                }
                            });
                    }
                    let source = list.iter().find(|(g, _)| g.name == f.name).map(|(_, s)| *s);
                    ui.horizontal(|ui| {
                        let two = f.placement == Placement::TwoPoints;
                        let hint = if two {
                            "Разместить по двум точкам"
                        } else {
                            "Разместить в точке"
                        };
                        if ui.button(hint).clicked() {
                            action = Some(FamiliesAction::Place { two_points: two });
                        }
                        if ui.button("Редактировать").clicked() {
                            action = Some(FamiliesAction::Edit(f.clone()));
                        }
                        if ui
                            .button("Сохранить в папку")
                            .on_hover_text(dir.display().to_string())
                            .clicked()
                        {
                            action = Some(FamiliesAction::Message(match save(&dir, f) {
                                Ok(p) => format!("Семейство сохранено: {}", p.display()),
                                Err(e) => format!("Не удалось сохранить семейство: {e}"),
                            }));
                            self.loaded_from = None;
                        }
                    });
                    if source == Some(Source::Project) {
                        let newer = self
                            .folder
                            .iter()
                            .chain(&sample_families())
                            .find(|g| g.name == f.name && *g != f)
                            .cloned();
                        ui.horizontal(|ui| {
                            if let Some(newer) = newer
                                && ui
                                    .button("Обновить в проекте из папки")
                                    .on_hover_text(
                                        "Все объекты семейства перестроятся по новому определению",
                                    )
                                    .clicked()
                            {
                                action = Some(FamiliesAction::Commit(
                                    format!("Обновить семейство «{}»", f.name),
                                    vec![Command::SetFamily {
                                        family: Box::new(newer),
                                    }],
                                ));
                            }
                            let used = project
                                .family_instances
                                .values()
                                .any(|i| i.family == f.name);
                            if ui
                                .add_enabled(!used, egui::Button::new("Убрать из проекта"))
                                .on_disabled_hover_text("Семейство размещено в модели")
                                .clicked()
                            {
                                action = Some(FamiliesAction::Commit(
                                    format!("Убрать семейство «{}»", f.name),
                                    vec![Command::RemoveFamily {
                                        name: f.name.clone(),
                                    }],
                                ));
                            }
                        });
                    }
                }
                if let Some(a) = self.selected_ui(ui, project, selected) {
                    action = Some(a);
                }
            });
        self.open = open;
        action
    }

    fn selected_ui(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<FamiliesAction> {
        let Some(instance) = instance_of(project, selected) else {
            self.editing = None;
            return None;
        };
        let family = project.families.get(&instance.family)?;
        if self.editing.as_ref().is_none_or(|e| e.id != instance.id) {
            self.editing = Some(Editing {
                id: instance.id,
                type_name: instance.type_name.clone(),
                values: family
                    .parameters
                    .iter()
                    .filter(|p| !p.of_type && p.formula.is_empty())
                    .map(|p| {
                        let v = instance.values.get(&p.name).unwrap_or(&p.value);
                        (p.name.clone(), v.clone())
                    })
                    .collect(),
            });
        }
        let e = self.editing.as_mut()?;
        ui.separator();
        ui.strong(format!("Выбранный объект: {}", family.name));
        if !family.types.is_empty() {
            egui::ComboBox::from_id_salt("family-instance-type")
                .selected_text(e.type_name.as_str())
                .show_ui(ui, |ui| {
                    for t in &family.types {
                        ui.selectable_value(&mut e.type_name, t.name.clone(), &t.name);
                    }
                });
        }
        let mut group = "";
        egui::Grid::new("family-instance-values")
            .num_columns(2)
            .show(ui, |ui| {
                for p in &family.parameters {
                    if p.group != group && !p.group.is_empty() {
                        group = &p.group;
                        ui.strong(group);
                        ui.end_row();
                    }
                    let label = if p.label.is_empty() {
                        p.name.clone()
                    } else {
                        format!("{} ({})", p.label, p.name)
                    };
                    ui.label(label);
                    match e.values.get_mut(&p.name) {
                        Some(v) if p.kind == ParamKind::YesNo => {
                            let mut on = v.trim() != "0" && v.trim() != "нет";
                            if ui.checkbox(&mut on, "").changed() {
                                *v = if on { "1" } else { "0" }.into();
                            }
                        }
                        Some(v) => {
                            ui.add(egui::TextEdit::singleline(v).desired_width(140.));
                        }
                        None => {
                            // Type and formula values: shown, not edited.
                            let shown = family
                                .values(
                                    &e.type_name,
                                    &instance.values,
                                    instance.length,
                                    &project.parametric.values().unwrap_or_default(),
                                )
                                .ok()
                                .and_then(|v| {
                                    v.numbers
                                        .get(&p.name)
                                        .map(|n| format!("{n}"))
                                        .or_else(|| v.texts.get(&p.name).cloned())
                                })
                                .unwrap_or_default();
                            ui.weak(shown);
                        }
                    }
                    ui.end_row();
                }
            });
        if ui.button("Применить").clicked() {
            return Some(match change(project, e.id, &e.type_name, &e.values) {
                Ok(c) => FamiliesAction::Commit(format!("Изменить «{}»", family.name), c),
                Err(m) => FamiliesAction::Message(m),
            });
        }
        None
    }
}

fn default_type(f: &Family) -> String {
    f.types.first().map(|t| t.name.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_bim_commands::Document;

    #[test]
    fn placing_a_sample_adds_it_and_changing_values_rebuilds() {
        let mut d = Document::new(Project::default()).unwrap();
        let seat = sample_families()
            .into_iter()
            .find(|f| f.name == "Столик опорный")
            .unwrap();
        let c = place(d.project(), &seat, "С-1", DVec3::new(0., 0., 1000.), None).unwrap();
        assert_eq!(c.len(), 2);
        d.transact("Семейство", &c).unwrap();
        let p = d.project();
        let catalogue = catalogue(p, &[]);
        assert_eq!(catalogue[0].1, Source::Project);
        assert_eq!(catalogue.len(), sample_families().len());
        let part = *p
            .family_instances
            .values()
            .next()
            .unwrap()
            .children
            .values()
            .next()
            .unwrap();
        let instance = instance_of(p, &BTreeSet::from([part])).unwrap().clone();
        let values = BTreeMap::from([
            ("n".to_string(), "1".to_string()),
            ("tr".to_string(), "12".to_string()),
        ]);
        let c = change(p, instance.id, "С-2", &values).unwrap();
        let Command::ReplaceFamilyInstance { instance: changed } = &c[0] else {
            panic!()
        };
        // tr equals the default and is not stored.
        assert_eq!(changed.values.len(), 1);
        d.transact("Изменить", &c).unwrap();
        assert_eq!(d.project().family_instances[&instance.id].children.len(), 3);
        // Placing again does not add the family a second time.
        let c = place(d.project(), &seat, "С-1", DVec3::new(500., 0., 0.), None).unwrap();
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn two_point_families_need_both_points_and_files_round_trip() {
        let ladder = sample_families()
            .into_iter()
            .find(|f| f.name == "Стремянка")
            .unwrap();
        let p = Project::default();
        assert!(place(&p, &ladder, "Ш600", DVec3::ZERO, None).is_err());
        let c = place(
            &p,
            &ladder,
            "Ш600",
            DVec3::ZERO,
            Some(DVec3::new(0., 0., 2400.)),
        )
        .unwrap();
        let Command::CreateFamilyInstance { instance } = &c[1] else {
            panic!()
        };
        assert_eq!(instance.length, 2400.);
        let dir = std::env::temp_dir().join(format!("obim-families-{}", std::process::id()));
        save(&dir, &ladder).unwrap();
        std::fs::write(dir.join("плохое.family.json"), "{").unwrap();
        let (found, errors) = load_all(&dir);
        assert_eq!(found, vec![ladder]);
        assert_eq!(errors.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
