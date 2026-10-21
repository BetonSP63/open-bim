//! Object groups and object representation: a group is a list of rules
//! («Имя», «Профиль», a property… compared with a value) joined by «И» and
//! «ИЛИ»; the representation gives each group a colour and a visibility
//! (visible, see-through, hidden). The first matching row wins, the rest of
//! the model keeps its own colours. Groups also filter the selection.
use super::selection::wildcard;
use open_bim_model::{ElementShape, MemberKind, ObjectId, Project, PropertyValue};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Profile,
    Material,
    Class,
    /// «Балка», «Колонна», «Пластина».
    Kind,
    /// Member length or the longer plate side, mm.
    Length,
    /// Any part property by its key.
    Property(String),
}

impl Field {
    pub const FIXED: [Self; 6] = [
        Self::Name,
        Self::Profile,
        Self::Material,
        Self::Class,
        Self::Kind,
        Self::Length,
    ];
    pub fn label(&self) -> String {
        match self {
            Self::Name => "Имя".into(),
            Self::Profile => "Профиль".into(),
            Self::Material => "Материал".into(),
            Self::Class => "Класс".into(),
            Self::Kind => "Тип детали".into(),
            Self::Length => "Длина".into(),
            Self::Property(key) => format!("Свойство «{key}»"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    /// Wildcards `*` and `?`, case ignored.
    Equals,
    NotEquals,
    Contains,
    StartsWith,
    Greater,
    Less,
}

impl Condition {
    pub const ALL: [Self; 6] = [
        Self::Equals,
        Self::NotEquals,
        Self::Contains,
        Self::StartsWith,
        Self::Greater,
        Self::Less,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Equals => "равно",
            Self::NotEquals => "не равно",
            Self::Contains => "содержит",
            Self::StartsWith => "начинается с",
            Self::Greater => "больше",
            Self::Less => "меньше",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Join {
    #[default]
    And,
    Or,
}

impl Join {
    pub fn label(self) -> &'static str {
        match self {
            Self::And => "И",
            Self::Or => "ИЛИ",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// How this rule joins the previous one (ignored for the first).
    pub join: Join,
    pub field: Field,
    pub condition: Condition,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub rules: Vec<Rule>,
}

fn text(v: &PropertyValue) -> String {
    match v {
        PropertyValue::Text(t) => t.clone(),
        PropertyValue::Number(n) => open_bim_reports::fmt(*n),
        PropertyValue::Boolean(b) => if *b { "да" } else { "нет" }.into(),
    }
}

/// The value of `field` on a part; `None` when the part has no such value.
fn field_value(project: &Project, id: ObjectId, field: &Field) -> Option<String> {
    let e = project.elements.get(&id)?;
    Some(match field {
        Field::Name => e.name.clone(),
        Field::Profile => match &e.shape {
            ElementShape::Member { profile, .. } => project.profiles.get(profile)?.name.clone(),
            ElementShape::Plate { thickness, .. } => format!("—{}", thickness.value()),
        },
        Field::Material => project.materials.get(&e.material)?.name.clone(),
        Field::Class => text(e.properties.get("class")?),
        Field::Kind => match &e.shape {
            ElementShape::Member {
                member_kind: MemberKind::Beam,
                ..
            } => "Балка".into(),
            ElementShape::Member { .. } => "Колонна".into(),
            ElementShape::Plate { .. } => "Пластина".into(),
        },
        Field::Length => open_bim_reports::fmt(open_bim_reports::section(project, id).ok()?.1),
        Field::Property(key) => text(e.properties.get(key)?),
    })
}

impl Rule {
    pub fn matches(&self, project: &Project, id: ObjectId) -> bool {
        let value = field_value(project, id, &self.field);
        let want = self.value.trim();
        let number = |s: &str| s.replace(',', ".").trim().parse::<f64>().ok();
        match self.condition {
            Condition::Equals => value.is_some_and(|v| equals(want, &v)),
            Condition::NotEquals => !value.is_some_and(|v| equals(want, &v)),
            Condition::Contains => {
                value.is_some_and(|v| v.to_lowercase().contains(&want.to_lowercase()))
            }
            Condition::StartsWith => {
                value.is_some_and(|v| v.to_lowercase().starts_with(&want.to_lowercase()))
            }
            Condition::Greater | Condition::Less => {
                let (Some(v), Some(w)) = (value.as_deref().and_then(number), number(want)) else {
                    return false;
                };
                if self.condition == Condition::Greater {
                    v > w
                } else {
                    v < w
                }
            }
        }
    }
}

/// Whole-text match with `*` / `?`; a typed hyphen matches the plate «—».
fn equals(pattern: &str, value: &str) -> bool {
    let whole = |p: &str| {
        if p.contains(['*', '?']) {
            wildcard(p, value)
        } else {
            p.to_lowercase() == value.to_lowercase()
        }
    };
    whole(pattern) || whole(&pattern.replacen('-', "—", 1))
}

impl Group {
    /// «И» binds before «ИЛИ»: `a И b ИЛИ c` is `(a И b) ИЛИ c`. An empty
    /// group matches every part. Objects other than parts never match.
    pub fn matches(&self, project: &Project, id: ObjectId) -> bool {
        if !project.elements.contains_key(&id) {
            return false;
        }
        let mut any = false;
        let mut all = true;
        for (i, rule) in self.rules.iter().enumerate() {
            if i > 0 && rule.join == Join::Or {
                any |= all;
                all = true;
            }
            all = all && rule.matches(project, id);
        }
        any || all
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Visible,
    /// See-through, percent 10…90.
    Transparent(u8),
    Hidden,
}

impl Visibility {
    pub fn label(self) -> String {
        match self {
            Self::Visible => "Видимый".into(),
            Self::Transparent(p) => format!("Прозрачный {p} %"),
            Self::Hidden => "Скрытый".into(),
        }
    }
    pub const CHOICES: [Self; 5] = [
        Self::Visible,
        Self::Transparent(50),
        Self::Transparent(70),
        Self::Transparent(90),
        Self::Hidden,
    ];
}

/// One row of the representation: a group, its colour (none keeps the
/// part's own) and visibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationRow {
    pub group: usize,
    pub color: Option<[u8; 3]>,
    pub visibility: Visibility,
}

/// What the rows give the viewport: colour and alpha per part, and the
/// parts hidden by the representation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub appearance: BTreeMap<ObjectId, open_bim_renderer::Appearance>,
    pub hidden: BTreeSet<ObjectId>,
}

pub fn apply(project: &Project, groups: &[Group], rows: &[RepresentationRow]) -> Applied {
    let mut out = Applied::default();
    for id in project.elements.keys() {
        let Some(row) = rows
            .iter()
            .find(|r| groups.get(r.group).is_some_and(|g| g.matches(project, *id)))
        else {
            continue;
        };
        let alpha = match row.visibility {
            Visibility::Hidden => {
                out.hidden.insert(*id);
                continue;
            }
            Visibility::Visible => 1.0,
            Visibility::Transparent(p) => 1.0 - f32::from(p.clamp(10, 90)) / 100.0,
        };
        out.appearance.insert(
            *id,
            open_bim_renderer::Appearance {
                color: row.color.map(|c| c.map(|v| f32::from(v) / 255.0)),
                alpha,
            },
        );
    }
    out
}

/// Starting groups of a new session.
pub fn default_groups() -> Vec<Group> {
    let kind = |name: &str, value: &str| Group {
        name: name.into(),
        rules: vec![Rule {
            join: Join::And,
            field: Field::Kind,
            condition: Condition::Equals,
            value: value.into(),
        }],
    };
    vec![
        kind("Колонны", "Колонна"),
        kind("Балки", "Балка"),
        kind("Пластины", "Пластина"),
    ]
}
