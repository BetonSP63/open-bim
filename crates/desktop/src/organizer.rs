//! «Органайзер» of the model tree: parts grouped into categories (by type,
//! mark, profile, level or assembly) with the count and mass of each.
//! Categories are computed from the model, never stored, so they are always
//! current.
use open_bim_model::{ElementShape, MemberKind, ObjectId, Project};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// Flat list of parts and grid lines (the plain model tree).
    #[default]
    None,
    Kind,
    Mark,
    Profile,
    Level,
    Assembly,
}

impl Grouping {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Kind,
        Self::Mark,
        Self::Profile,
        Self::Level,
        Self::Assembly,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "без группировки",
            Self::Kind => "по типу",
            Self::Mark => "по марке",
            Self::Profile => "по профилю",
            Self::Level => "по уровню",
            Self::Assembly => "по сборке",
        }
    }
}

/// One category: name, parts and their total mass.
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub name: String,
    pub parts: Vec<ObjectId>,
    pub mass_kg: f64,
}

/// Part type as engineers name it.
pub fn kind_name(project: &Project, id: ObjectId) -> &'static str {
    let Some(element) = project.elements.get(&id) else {
        return "Прочее";
    };
    match &element.shape {
        ElementShape::Member {
            member_kind: MemberKind::Column,
            ..
        } => "Колонны",
        ElementShape::Member {
            member_kind: MemberKind::Beam,
            ..
        } => "Балки",
        ElementShape::Plate { .. } => "Пластины",
    }
}

/// Lowest point of a part, mm (member ends along the local Z axis, or the
/// plate frame origin).
fn bottom_z(project: &Project, id: ObjectId) -> Option<f64> {
    let element = project.elements.get(&id)?;
    let frame = element.shape.frame();
    let z0 = frame.origin.0[2];
    Some(match &element.shape {
        ElementShape::Member { length, .. } => z0.min(
            frame
                .world(open_bim_model::Point3([0., 0., length.value()]))
                .0[2],
        ),
        ElementShape::Plate { .. } => z0,
    })
}

/// Name of the highest level at or below the part's bottom (1 mm tolerance).
pub fn level_name(project: &Project, id: ObjectId) -> String {
    let Some(z) = bottom_z(project, id) else {
        return "Без уровня".into();
    };
    project
        .levels
        .values()
        .filter(|l| l.elevation_mm <= z + 1.)
        .max_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm))
        .map_or_else(|| "Ниже уровней".into(), |l| l.name.clone())
}

fn profile_name(project: &Project, id: ObjectId) -> String {
    match project.elements.get(&id).map(|e| &e.shape) {
        Some(ElementShape::Member { profile, .. }) => project
            .profiles
            .get(profile)
            .map_or_else(|| "Профиль не найден".into(), |p| p.name.clone()),
        Some(ElementShape::Plate { thickness, .. }) => format!("Лист {}", thickness.value()),
        None => "Прочее".into(),
    }
}

/// Categories of every part, sorted by name («Без сборки» and similar
/// catch-all names last). Marks come from the current numbering.
pub fn categories(project: &Project, grouping: Grouping) -> Vec<Category> {
    if grouping == Grouping::None {
        return Vec::new();
    }
    let numbering = match grouping {
        Grouping::Mark | Grouping::Assembly => open_bim_numbering::number(project).ok(),
        _ => None,
    };
    let assembly_of: BTreeMap<ObjectId, ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.members.iter().map(move |m| (*m, a.id)))
        .collect();
    let mut groups: BTreeMap<(bool, String), Vec<ObjectId>> = BTreeMap::new();
    for id in project.elements.keys().copied() {
        let (rest, name) = match grouping {
            Grouping::None => unreachable!("returned above"),
            Grouping::Kind => (false, kind_name(project, id).to_string()),
            Grouping::Profile => (false, profile_name(project, id)),
            Grouping::Level => {
                let name = level_name(project, id);
                (name == "Ниже уровней" || name == "Без уровня", name)
            }
            Grouping::Mark => match numbering.as_ref().and_then(|n| n.parts.get(&id)) {
                Some(mark) => (false, format!("поз. {mark}")),
                None => (true, "Без позиции".into()),
            },
            Grouping::Assembly => match assembly_of.get(&id) {
                Some(a) => {
                    let mark = numbering
                        .as_ref()
                        .and_then(|n| n.assemblies.get(a))
                        .cloned()
                        .unwrap_or_default();
                    let name = &project.assemblies[a].name;
                    (false, format!("{mark} {name}").trim().to_string())
                }
                None => (true, "Без сборки".into()),
            },
        };
        groups.entry((rest, name)).or_default().push(id);
    }
    let mut out: Vec<Category> = groups
        .into_iter()
        .map(|((_, name), parts)| Category {
            mass_kg: parts
                .iter()
                .filter_map(|id| project.element_mass_kg(*id).ok())
                .sum(),
            name,
            parts,
        })
        .collect();
    // Natural order for marks and levels written with numbers («поз. 2»
    // before «поз. 10»); the catch-all groups keep their place at the end.
    if grouping == Grouping::Mark {
        let key = |c: &Category| {
            let digits: String = c.name.chars().filter(char::is_ascii_digit).collect();
            (
                c.name == "Без позиции",
                digits.parse::<u64>().unwrap_or(u64::MAX),
                c.name.clone(),
            )
        };
        out.sort_by_key(key);
    }
    if grouping == Grouping::Level {
        let elevation = |c: &Category| {
            project
                .levels
                .values()
                .find(|l| l.name == c.name)
                .map_or(f64::INFINITY, |l| l.elevation_mm)
        };
        out.sort_by(|a, b| elevation(a).total_cmp(&elevation(b)));
    }
    out
}

/// Shared list of categories (the panel keeps it while drawing).
pub type Categories = std::sync::Arc<Vec<Category>>;

/// Cached categories keyed by (revision, grouping).
#[derive(Default)]
pub struct Organizer {
    pub grouping: Grouping,
    cache: Option<((u64, Grouping), Categories)>,
}

impl Organizer {
    pub fn new(grouping: Grouping) -> Self {
        Self {
            grouping,
            cache: None,
        }
    }

    pub fn categories(&mut self, project: &Project) -> Categories {
        let key = (project.revision, self.grouping);
        if let Some((k, c)) = &self.cache
            && *k == key
        {
            return c.clone();
        }
        let c = std::sync::Arc::new(categories(project, self.grouping));
        self.cache = Some((key, c.clone()));
        c
    }
}

/// Mass for labels: «1 234,5» (kg, one decimal, comma, thin groups).
pub fn kg(mass: f64) -> String {
    let text = format!("{mass:.1}");
    let (int, frac) = text.split_once('.').unwrap_or((&text, "0"));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 && c != '-' {
            grouped.push('\u{202f}');
        }
        grouped.push(c);
    }
    format!("{grouped},{frac}")
}
