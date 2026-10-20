//! Profile and material lists like Tekla's «Выбрать профиль»: the project's
//! own profiles first, then the built-in ГОСТ catalogue grouped by family,
//! with a search field. Picking a catalogue entry copies it into the project.
use eframe::egui;
use open_bim_model::library::{self, Family};
use open_bim_model::{Material, ObjectId, Profile, Project};

/// The profile (or material) a list returns: one already in the project, or
/// a catalogue record not yet stored (its id is the project's next free id).
#[derive(Clone, Debug, PartialEq)]
pub enum Pick<T> {
    Project(ObjectId),
    New(T),
}

impl<T> Pick<T> {
    pub fn id(&self) -> ObjectId
    where
        T: HasId,
    {
        match self {
            Self::Project(id) => *id,
            Self::New(v) => v.id(),
        }
    }
}

pub trait HasId {
    fn id(&self) -> ObjectId;
}
impl HasId for Profile {
    fn id(&self) -> ObjectId {
        self.id
    }
}
impl HasId for Material {
    fn id(&self) -> ObjectId {
        self.id
    }
}

/// A catalogue profile by name, reusing the project's copy when it has one.
pub fn profile_by_name(project: &Project, name: &str) -> Option<Pick<Profile>> {
    if let Some(p) = project.profiles.values().find(|p| p.name == name) {
        return Some(Pick::Project(p.id));
    }
    library::section(name).map(|e| Pick::New(e.profile(ObjectId(project.next_id))))
}

/// A catalogue steel by grade, reusing the project's copy when it has one.
pub fn material_by_grade(project: &Project, grade: &str) -> Option<Pick<Material>> {
    let m = library::steel(ObjectId(project.next_id), grade)?;
    match project.materials.values().find(|p| p.name == m.name) {
        Some(existing) => Some(Pick::Project(existing.id)),
        None => Some(Pick::New(m)),
    }
}

fn search_field(ui: &mut egui::Ui, salt: &str) -> String {
    let id = egui::Id::new((salt, "catalog search"));
    let mut query: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text("Поиск: 30Б1, ш1, труба 219")
            .desired_width(200.),
    )
    .request_focus();
    ui.data_mut(|d| d.insert_temp(id, query.clone()));
    query
}

fn matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    query
        .split_whitespace()
        .all(|w| name.contains(&w.to_lowercase()))
}

/// The body of a profile list (inside a combo box popup).
pub fn profile_menu(
    ui: &mut egui::Ui,
    salt: &str,
    project: &Project,
    current: Option<ObjectId>,
) -> Option<Pick<Profile>> {
    let query = search_field(ui, salt);
    let mut picked = None;
    egui::ScrollArea::vertical()
        .max_height(360.)
        .show(ui, |ui| {
            ui.weak("В проекте");
            for p in project
                .profiles
                .values()
                .filter(|p| matches(&p.name, &query))
            {
                if ui
                    .selectable_label(current == Some(p.id), &p.name)
                    .clicked()
                {
                    picked = Some(Pick::Project(p.id));
                }
            }
            let found = library::search(&query);
            for family in Family::ALL {
                let rows: Vec<_> = found
                    .iter()
                    .filter(|e| e.family == family)
                    .filter(|e| !project.profiles.values().any(|p| p.name == e.name))
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                egui::CollapsingHeader::new(family.label())
                    .id_salt((salt, family.label()))
                    .open((!query.trim().is_empty()).then_some(true))
                    .show(ui, |ui| {
                        for e in rows {
                            let label = format!("{}  ·  {:.1} кг/м", e.name, e.mass_kg_m())
                                .replace('.', ",");
                            if ui.selectable_label(false, label).clicked() {
                                picked = profile_by_name(project, &e.name);
                            }
                        }
                    });
            }
        });
    if picked.is_some() {
        ui.close();
    }
    picked
}

/// The body of a material list: project materials, then catalogue steels.
pub fn material_menu(
    ui: &mut egui::Ui,
    project: &Project,
    current: Option<ObjectId>,
) -> Option<Pick<Material>> {
    let mut picked = None;
    ui.weak("В проекте");
    for m in project.materials.values() {
        if ui
            .selectable_label(current == Some(m.id), &m.name)
            .clicked()
        {
            picked = Some(Pick::Project(m.id));
        }
    }
    ui.weak("Каталог сталей");
    for (grade, standard) in library::steels() {
        let name = format!("{grade} · {standard}");
        if project.materials.values().any(|m| m.name == name) {
            continue;
        }
        if ui.selectable_label(false, name).clicked() {
            picked = material_by_grade(project, grade);
        }
    }
    picked
}

/// Shown name of a picked or stored profile.
pub fn profile_name<'a>(
    project: &'a Project,
    pending: Option<&'a Profile>,
    id: Option<ObjectId>,
) -> Option<&'a str> {
    let id = id?;
    project
        .profiles
        .get(&id)
        .or(pending.filter(|p| p.id == id))
        .map(|p| p.name.as_str())
}

/// Shown name of a picked or stored material.
pub fn material_name<'a>(
    project: &'a Project,
    pending: Option<&'a Material>,
    id: Option<ObjectId>,
) -> Option<&'a str> {
    let id = id?;
    project
        .materials
        .get(&id)
        .or(pending.filter(|m| m.id == id))
        .map(|m| m.name.as_str())
}
