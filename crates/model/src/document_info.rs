//! Document attributes for the main inscription (ГОСТ Р 21.101-2026,
//! appendix Е): who issues the documentation and for what, who signs it, and
//! archive numbers. Signatures themselves are put by people (or as an
//! electronic signature); the program only lays out the names and dates.
use crate::ModelError;
use serde::{Deserialize, Serialize};

/// Kind of documentation for graph 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// «П» — проектная документация.
    Design,
    /// «Р» — рабочая документация.
    #[default]
    Working,
}

impl Stage {
    pub fn code(self) -> &'static str {
        match self {
            Self::Design => "П",
            Self::Working => "Р",
        }
    }
}

/// One signing person: position (graph 10), surname (graph 11), date of
/// signing (graph 13, «ДД.ММ.ГГ»).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub position: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
}

impl Person {
    pub fn is_empty(&self) -> bool {
        self.position.is_empty() && self.name.is_empty() && self.date.is_empty()
    }
}

/// Attributes shared by every sheet of the project's documents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentInfo {
    /// Base designation (contract number or object code, 4.1.2.1); the set
    /// designation is «<base>-<mark>» (4.2.4).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_designation: String,
    /// Graph 2: enterprise / complex / construction stage.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    /// Graph 3: building or structure.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building: String,
    /// Graph 9: developer organisation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub organisation: String,
    /// Graph 27: customer's short name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub customer: String,
    #[serde(default, skip_serializing_if = "is_working")]
    pub stage: Stage,
    /// Rows of graphs 10–13: «Разраб.», «Пров.», up to four more, «Н. контр.»
    /// and the approving person in the last row.
    #[serde(default, skip_serializing_if = "Person::is_empty")]
    pub developer: Person,
    #[serde(default, skip_serializing_if = "Person::is_empty")]
    pub checker: Person,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub others: Vec<Person>,
    #[serde(default, skip_serializing_if = "Person::is_empty")]
    pub norm_control: Person,
    #[serde(default, skip_serializing_if = "Person::is_empty")]
    pub approver: Person,
    /// «Согласовано» graphs on the binding field of the first sheet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approvals: Vec<Person>,
    /// Graph 20: inventory number of the original.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub inventory_number: String,
    /// Graph 22: inventory number of the original this one replaces.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub replaced_inventory: String,
    /// General notes of «Общие данные» (4.3.5), one item per entry; the
    /// program adds its own items (masses, steel grades, welds) after them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub general_notes: Vec<String>,
}

fn is_working(stage: &Stage) -> bool {
    *stage == Stage::Working
}

pub const MAX_OTHER_SIGNERS: usize = 4;
pub const MAX_APPROVALS: usize = 6;
pub const MAX_GENERAL_NOTES: usize = 50;

/// Base designation used when none is set.
pub const DEFAULT_BASE: &str = "0000";

impl DocumentInfo {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Designation of a set of the given mark («КМ», «КМД»): «2345-КМ».
    pub fn designation(&self, mark: &str) -> String {
        let base = self.base_designation.trim();
        let base = if base.is_empty() { DEFAULT_BASE } else { base };
        format!("{base}-{mark}")
    }

    /// The eight signature rows of forms 3 and 4 (position, person); form 5
    /// keeps the first two, two others, «Н. контр.» and the approver.
    pub fn signature_rows(&self, rows: usize) -> Vec<(String, &Person)> {
        let others = rows.saturating_sub(4);
        let mut out = vec![
            ("Разраб.".to_string(), &self.developer),
            ("Пров.".to_string(), &self.checker),
        ];
        static EMPTY: Person = Person {
            position: String::new(),
            name: String::new(),
            date: String::new(),
        };
        for i in 0..others {
            let p = self.others.get(i).unwrap_or(&EMPTY);
            out.push((p.position.clone(), p));
        }
        out.push(("Н. контр.".to_string(), &self.norm_control));
        out.push((self.approver.position.clone(), &self.approver));
        out
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let invalid = |m: String| {
            Err(ModelError::InvalidValue(format!(
                "Сведения о документе: {m}"
            )))
        };
        let fields = [
            &self.base_designation,
            &self.object,
            &self.building,
            &self.organisation,
            &self.customer,
            &self.inventory_number,
            &self.replaced_inventory,
        ];
        if fields.iter().any(|f| f.len() > 1024 || f.contains('\0')) {
            return invalid("слишком длинное поле".into());
        }
        // 4.1.2.3: no space as a separator inside designations.
        if self.base_designation.trim().contains(char::is_whitespace) {
            return invalid("в обозначении не допускается пробел (ГОСТ Р 21.101, 4.1.2.3)".into());
        }
        if self.others.len() > MAX_OTHER_SIGNERS {
            return invalid(format!(
                "не больше {MAX_OTHER_SIGNERS} дополнительных подписей"
            ));
        }
        if self.general_notes.len() > MAX_GENERAL_NOTES
            || self
                .general_notes
                .iter()
                .any(|n| n.len() > 2048 || n.contains('\0'))
        {
            return invalid(format!(
                "общих указаний не больше {MAX_GENERAL_NOTES}, каждое до 2048 байт"
            ));
        }
        if self.approvals.len() > MAX_APPROVALS {
            return invalid(format!("не больше {MAX_APPROVALS} согласований"));
        }
        let people = [
            &self.developer,
            &self.checker,
            &self.norm_control,
            &self.approver,
        ]
        .into_iter()
        .chain(&self.others)
        .chain(&self.approvals);
        for p in people {
            if [&p.position, &p.name]
                .iter()
                .any(|f| f.len() > 256 || f.contains('\0'))
            {
                return invalid("слишком длинное имя или должность".into());
            }
            if !p.date.is_empty() {
                parse_date(&p.date).map_err(ModelError::InvalidValue)?;
            }
        }
        Ok(())
    }
}

/// Parse a date written «ДД.ММ.ГГ» (appendix Е, note 2) into (day, month,
/// two-digit year).
pub fn parse_date(text: &str) -> Result<(u32, u32, u32), String> {
    let err = || format!("Дата «{text}»: формат ДД.ММ.ГГ (ГОСТ Р 21.101, приложение Е)");
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.len() != 2 || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(err());
    }
    let n = |s: &str| s.parse::<u32>().map_err(|_| err());
    let (d, m, y) = (n(parts[0])?, n(parts[1])?, n(parts[2])?);
    let leap = y % 4 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(err()),
    };
    if d == 0 || d > days {
        return Err(err());
    }
    Ok((d, m, y))
}

/// Date as «ДД.ММ.ГГ».
pub fn format_date(year: i32, month: u32, day: u32) -> String {
    format!("{day:02}.{month:02}.{:02}", year.rem_euclid(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_dd_mm_yy() {
        assert_eq!(parse_date("06.02.19"), Ok((6, 2, 19)));
        assert_eq!(parse_date("29.02.24"), Ok((29, 2, 24)));
        for bad in [
            "6.02.19",
            "06.02.2019",
            "31.04.26",
            "29.02.25",
            "06-02-19",
            "",
        ] {
            assert!(parse_date(bad).is_err(), "{bad}");
        }
        assert_eq!(format_date(2026, 10, 19), "19.10.26");
    }

    #[test]
    fn designation_and_rows() {
        let mut info = DocumentInfo::default();
        assert_eq!(info.designation("КМ"), "0000-КМ");
        info.base_designation = "2345".into();
        assert_eq!(info.designation("КМД"), "2345-КМД");
        info.base_designation = "23 45".into();
        assert!(info.validate().is_err());
        info.base_designation = "2345".into();
        info.approver.position = "ГИП".into();
        let rows = info.signature_rows(8);
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0].0, "Разраб.");
        assert_eq!(rows[6].0, "Н. контр.");
        assert_eq!(rows[7].0, "ГИП");
        assert_eq!(info.signature_rows(6).len(), 6);
    }
}
