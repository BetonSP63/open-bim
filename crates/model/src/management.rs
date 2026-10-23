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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Management {
    pub phases: Vec<ListEntry>,
    /// New parts get this phase.
    pub current_phase: u32,
    pub lots: Vec<ListEntry>,
    /// Sequence names; part numbers are kept on the parts.
    pub sequences: Vec<String>,
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
        let mut names = std::collections::BTreeSet::new();
        for s in &self.sequences {
            if s.trim().is_empty() || s.len() > 256 || !names.insert(s) {
                return bad("Имя последовательности пустое или повторяется");
            }
        }
        Ok(())
    }
}
