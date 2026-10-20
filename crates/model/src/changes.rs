//! Changes of issued documents (ГОСТ Р 21.101-2026, section 7). A change
//! is numbered within its set (7.3.1), made under one permission and lists
//! the sheets it touches: changed sheets keep their changed areas (closed
//! contours, appendix Л), replaced sheets are «Зам.», added ones «Нов.»,
//! annulled ones «Аннул.» (7.3.13, 7.4.4).
use crate::{ModelError, ObjectId, SET_MARKS, parse_date};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetChangeKind {
    /// Sheet changed in place; each area [x0, y0, x1, y1] on the sheet, mm.
    Changed { areas: Vec<[f64; 4]> },
    /// Sheet issued anew instead of the previous one.
    Replaced,
    /// Sheet added to the set.
    New,
    /// Sheet withdrawn: its number and title stay in the list.
    Annulled { sheet: usize, title: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetChange {
    pub drawing: ObjectId,
    pub kind: SheetChangeKind,
}

impl SheetChange {
    /// Graph 15 «Кол. уч.»: changed areas, a dash for a replaced sheet.
    pub fn areas_text(&self) -> String {
        match &self.kind {
            SheetChangeKind::Changed { areas } => areas.len().to_string(),
            _ => "-".into(),
        }
    }

    /// Graph 16 «Лист»: «Зам.», «Нов.» or a dash.
    pub fn sheet_text(&self) -> &'static str {
        match self.kind {
            SheetChangeKind::Replaced => "Зам.",
            SheetChangeKind::New => "Нов.",
            SheetChangeKind::Annulled { .. } => "Аннул.",
            SheetChangeKind::Changed { .. } => "-",
        }
    }
}

/// Reason codes of table К.1.
pub const REASONS: [(u8, &str); 5] = [
    (1, "Введение усовершенствований"),
    (2, "Изменение стандартов и норм"),
    (3, "Дополнительные требования заказчика"),
    (4, "Устранение ошибок"),
    (5, "Другие причины"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// Set mark, «КМ» or «КМД».
    pub mark: String,
    /// Change number within the set, 1, 2, 3…
    pub number: u32,
    /// Permission designation (graph 18 «№ док.»), e.g. «15-26».
    pub permission: String,
    /// Date of the change, «ДД.ММ.ГГ».
    pub date: String,
    /// Content of the change for the permission and the journal.
    pub description: String,
    /// Reason code of table К.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<u8>,
    /// Who made the change (journal graph 7).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    pub sheets: Vec<SheetChange>,
}

impl Change {
    pub fn validate(&self, paper: impl Fn(ObjectId) -> Option<[f64; 2]>) -> Result<(), ModelError> {
        let invalid = |m: &str| Err(ModelError::InvalidValue(format!("Изменение: {m}")));
        if !SET_MARKS.contains(&self.mark.as_str()) || self.number == 0 {
            return invalid("марка комплекта или номер");
        }
        let text = |s: &str, n: usize| s.len() <= n && !s.contains('\0');
        if !text(&self.permission, 64)
            || self.permission.trim().is_empty()
            || !text(&self.description, 8192)
            || !text(&self.author, 256)
        {
            return invalid("обозначение разрешения, содержание или автор");
        }
        parse_date(&self.date).map_err(ModelError::InvalidValue)?;
        if self.reason.is_some_and(|r| !(1..=5).contains(&r)) {
            return invalid("код причины 1…5 (таблица К.1)");
        }
        if self.sheets.is_empty() || self.sheets.len() > 10_000 {
            return invalid("нет листов");
        }
        for s in &self.sheets {
            match &s.kind {
                SheetChangeKind::Changed { areas } => {
                    // A later annulment removes the drawing; its record stays.
                    let size = paper(s.drawing).unwrap_or([f64::INFINITY; 2]);
                    if areas.is_empty()
                        || areas.len() > 999
                        || areas.iter().any(|a| {
                            !a.iter().all(|v| v.is_finite())
                                || a[0] >= a[2]
                                || a[1] >= a[3]
                                || a[0] < 0.
                                || a[1] < 0.
                                || a[2] > size[0]
                                || a[3] > size[1]
                        })
                    {
                        return invalid("участок изменения вне листа");
                    }
                }
                SheetChangeKind::Annulled { sheet, title } => {
                    if *sheet == 0 || !text(title, 4096) {
                        return invalid("аннулированный лист");
                    }
                }
                SheetChangeKind::Replaced | SheetChangeKind::New => {}
            }
        }
        Ok(())
    }
}
