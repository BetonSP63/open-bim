//! Drawing definitions stored with the model. Geometry is never stored here:
//! views, dimensions and marks are regenerated from referenced object IDs.
//! References may become dangling after deletions; this is reported by the
//! drawing engine as a lost binding instead of blocking model edits.
use crate::{ModelError, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetSize {
    A4,
    A3,
    A2,
    A1,
    A0,
}
impl SheetSize {
    /// Landscape width × height, millimetres (ГОСТ 2.301).
    pub fn landscape_mm(self) -> [f64; 2] {
        match self {
            Self::A4 => [297., 210.],
            Self::A3 => [420., 297.],
            Self::A2 => [594., 420.],
            Self::A1 => [841., 594.],
            Self::A0 => [1189., 841.],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::A2 => "A2",
            Self::A1 => "A1",
            Self::A0 => "A0",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DrawingKind {
    /// General arrangement of the listed parts (empty = whole model).
    General { objects: Vec<ObjectId> },
    /// Single-part fabrication drawing in the part frame.
    Part { part: ObjectId },
    /// Assembly drawing in the main part frame.
    Assembly { assembly: ObjectId },
}

/// Viewing direction in the drawing frame (first-angle projection, ГОСТ 2.305).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewDirection {
    Front,
    Top,
    Left,
    Back,
    Bottom,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingView {
    pub id: u32,
    pub name: String,
    pub direction: ViewDirection,
    /// Section plane depth along the viewing direction, drawing-frame mm;
    /// geometry in front of the plane is removed and the cut is hatched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<f64>,
    /// View centre on the sheet, paper mm from the lower-left corner.
    pub position: [f64; 2],
    /// Scale denominator override (1:N); `None` uses the drawing scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnchorPoint {
    /// Member start or plate frame origin.
    Start,
    /// Member end point (start + length along the member axis).
    End,
    /// Plate contour vertex, zero-based.
    Vertex { index: usize },
    /// Hole centre at its entry face.
    Hole { hole: ObjectId },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub object: ObjectId,
    pub point: AnchorPoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionAxis {
    Horizontal,
    Vertical,
    Aligned,
}

/// A dimension chain: consecutive anchors measured along one axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionDef {
    pub id: u32,
    pub view: u32,
    pub anchors: Vec<Anchor>,
    pub axis: DimensionAxis,
    /// Distance of the dimension line from the measured points, paper mm.
    pub offset: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextNote {
    pub id: u32,
    /// Sheet position, paper mm.
    pub position: [f64; 2],
    pub text: String,
    /// Character height, paper mm (ГОСТ 2.304 sizes 2.5–14).
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawing {
    pub id: ObjectId,
    pub kind: DrawingKind,
    /// Document designation shown in the title block.
    pub number: String,
    pub title: String,
    pub sheet: SheetSize,
    /// Scale denominator, 1:N.
    pub scale: f64,
    pub views: Vec<DrawingView>,
    #[serde(default)]
    pub dimensions: Vec<DimensionDef>,
    #[serde(default)]
    pub texts: Vec<TextNote>,
    /// Object fingerprints captured at the last regeneration.
    #[serde(default)]
    pub fingerprints: BTreeMap<ObjectId, String>,
    /// Model revision of the last regeneration.
    pub generated_revision: u64,
    /// Model revision at issue; only an up-to-date drawing can be issued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_revision: Option<u64>,
}

pub const MAX_DRAWING_ITEMS: usize = 10_000;

impl Drawing {
    pub fn validate(&self) -> Result<(), ModelError> {
        let invalid = |m: &str| Err(ModelError::InvalidValue(format!("Чертёж: {m}")));
        let text = |s: &str| s.len() <= 4096 && !s.contains('\0');
        if !text(&self.number) || !text(&self.title) || self.number.trim().is_empty() {
            return invalid("обозначение или наименование");
        }
        if !(self.scale.is_finite() && (0.01..=10_000.).contains(&self.scale)) {
            return invalid("масштаб");
        }
        if self.views.is_empty()
            || self.views.len() > 64
            || self.dimensions.len() > MAX_DRAWING_ITEMS
            || self.texts.len() > MAX_DRAWING_ITEMS
            || self.fingerprints.len() > 1_000_000
        {
            return invalid("количество видов, размеров или текстов");
        }
        if let DrawingKind::General { objects } = &self.kind
            && objects.len() > 1_000_000
        {
            return invalid("количество объектов");
        }
        let paper = self.sheet.landscape_mm();
        let on_sheet = |p: [f64; 2]| {
            p.iter().all(|v| v.is_finite())
                && (0. ..=paper[0]).contains(&p[0])
                && (0. ..=paper[1]).contains(&p[1])
        };
        let mut ids = std::collections::BTreeSet::new();
        for view in &self.views {
            if !ids.insert(view.id) || !text(&view.name) || !on_sheet(view.position) {
                return invalid("вид");
            }
            if view
                .section
                .is_some_and(|v| !v.is_finite() || v.abs() > 1e9)
                || view
                    .scale
                    .is_some_and(|s| !(s.is_finite() && (0.01..=10_000.).contains(&s)))
            {
                return invalid("сечение или масштаб вида");
            }
        }
        let mut dims = std::collections::BTreeSet::new();
        for dim in &self.dimensions {
            if !dims.insert(dim.id)
                || !ids.contains(&dim.view)
                || dim.anchors.len() < 2
                || dim.anchors.len() > 1000
                || !dim.offset.is_finite()
                || dim.offset.abs() > 1000.
            {
                return invalid("размер");
            }
        }
        let mut notes = std::collections::BTreeSet::new();
        for note in &self.texts {
            if !notes.insert(note.id)
                || !text(&note.text)
                || !on_sheet(note.position)
                || !(note.height.is_finite() && (1.0..=50.).contains(&note.height))
            {
                return invalid("текст");
            }
        }
        if self.fingerprints.values().any(|f| f.len() > 128) {
            return invalid("отпечаток объекта");
        }
        Ok(())
    }
}
