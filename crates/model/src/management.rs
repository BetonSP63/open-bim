//! Project management lists: phases («Стадии»), lots («Партии») and
//! sequences («Последовательности»). Objects carry their phase, lot,
//! sequence numbers and lock as properties under the keys below.
use serde::{Deserialize, Serialize};

/// Phase number of a part (Number); a part without it is in phase 1.
pub const PHASE_PROPERTY: &str = "phase";
/// Lot number of a part (Number).
pub const LOT_PROPERTY: &str = "lot";
/// Locked part (Boolean): changes and deletion are refused until unlocked.
pub const LOCK_PROPERTY: &str = "locked";
/// Task number of a part (Number), for the schedule and the project status.
pub const TASK_PROPERTY: &str = "task";
/// Building storey of a part (Number), for the building hierarchy.
pub const STOREY_PROPERTY: &str = "storey";
/// Prefix of the property holding a part's number in a sequence.
pub const SEQUENCE_PREFIX: &str = "sequence:";

/// Property key of a part's number in the sequence `name`.
pub fn sequence_key(name: &str) -> String {
    format!("{SEQUENCE_PREFIX}{name}")
}

/// A numbered phase or lot with a name and a comment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListEntry {
    pub number: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

/// A scheduled task («Задания»): dates as `ГГГГ-ММ-ДД`, end included.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub number: u32,
    pub name: String,
    pub start: String,
    pub end: String,
}

/// A building storey of the hierarchy at its elevation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storey {
    pub number: u32,
    pub name: String,
    pub elevation_mm: f64,
}

/// Site, building and storeys («Иерархия здания»), exported as the IFC
/// spatial structure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Hierarchy {
    pub site: String,
    pub building: String,
    pub storeys: Vec<Storey>,
}

impl Default for Hierarchy {
    fn default() -> Self {
        Self {
            site: "Площадка".into(),
            building: "Здание".into(),
            storeys: Vec::new(),
        }
    }
}

/// Days since 0000-03-01 of a `ГГГГ-ММ-ДД` date (for comparing dates).
pub fn day_number(date: &str) -> Option<i64> {
    let mut it = date.trim().split('-');
    let (y, m, d): (i64, i64, i64) = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    if it.next().is_some() || !(1..=12).contains(&m) || !(1000..=9999).contains(&y) {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=days[(m - 1) as usize]).contains(&d) {
        return None;
    }
    // Shift the year to start in March (civil-from-days inverse).
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    Some(365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + d - 1)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Management {
    pub phases: Vec<ListEntry>,
    /// New parts get this phase.
    pub current_phase: u32,
    pub lots: Vec<ListEntry>,
    /// Sequence names; part numbers are kept on the parts.
    pub sequences: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
    pub hierarchy: Hierarchy,
}

impl Default for Management {
    fn default() -> Self {
        Self {
            phases: vec![ListEntry {
                number: 1,
                name: "Стадия 1".into(),
                comment: String::new(),
            }],
            current_phase: 1,
            lots: Vec::new(),
            sequences: Vec::new(),
            tasks: Vec::new(),
            hierarchy: Hierarchy::default(),
        }
    }
}

impl Management {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), crate::ModelError> {
        let bad = |m: &str| Err(crate::ModelError::InvalidValue(m.into()));
        for (list, what) in [(&self.phases, "стадии"), (&self.lots, "партии")] {
            let mut seen = std::collections::BTreeSet::new();
            for s in list {
                if s.number == 0 || !seen.insert(s.number) {
                    return bad(&format!(
                        "Номер {what} {} повторяется или равен 0",
                        s.number
                    ));
                }
                if s.name.trim().is_empty() || s.name.len() > 256 || s.comment.len() > 4096 {
                    return bad(&format!("Имя {what} {}: от 1 до 256 символов", s.number));
                }
            }
        }
        if !self.phases.iter().any(|p| p.number == self.current_phase) {
            return bad("Текущая стадия не найдена в списке стадий");
        }
        let mut seen = std::collections::BTreeSet::new();
        for t in &self.tasks {
            if t.number == 0 || !seen.insert(t.number) || t.name.trim().is_empty() {
                return bad(&format!(
                    "Задание {}: номер повторяется или пустое имя",
                    t.number
                ));
            }
            match (day_number(&t.start), day_number(&t.end)) {
                (Some(a), Some(b)) if a <= b => {}
                _ => {
                    return bad(&format!(
                        "Задание «{}»: даты в виде ГГГГ-ММ-ДД, начало не позже конца",
                        t.name
                    ));
                }
            }
        }
        let h = &self.hierarchy;
        if h.site.trim().is_empty() || h.building.trim().is_empty() {
            return bad("Имена площадки и здания не могут быть пустыми");
        }
        let mut seen = std::collections::BTreeSet::new();
        for s in &h.storeys {
            if s.number == 0
                || !seen.insert(s.number)
                || s.name.trim().is_empty()
                || !s.elevation_mm.is_finite()
                || s.elevation_mm.abs() > 1e7
            {
                return bad(&format!(
                    "Этаж {}: номер, имя или отметка неверны",
                    s.number
                ));
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for s in &self.sequences {
            if s.trim().is_empty() || s.len() > 256 || !names.insert(s) {
                return bad("Имя последовательности пустое или повторяется");
            }
        }
        Ok(())
    }
}
