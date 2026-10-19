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
    /// Allowed multiples of a basic format (ГОСТ 2.301, additional formats:
    /// A0×2, A0×3, A1×3, A1×4, A2×3…A2×5, A3×3…A3×7, A4×3…A4×9).
    pub fn multiples(self) -> std::ops::RangeInclusive<u8> {
        match self {
            Self::A0 => 2..=3,
            Self::A1 => 3..=4,
            Self::A2 => 3..=5,
            Self::A3 => 3..=7,
            Self::A4 => 3..=9,
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
    /// User lines, rectangles, circles, hatching and masking on the sheet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<crate::SheetObject>,
    /// Object fingerprints captured at the last regeneration.
    #[serde(default)]
    pub fingerprints: BTreeMap<ObjectId, String>,
    /// Model revision of the last regeneration.
    pub generated_revision: u64,
    /// Model revision at issue; only an up-to-date drawing can be issued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_revision: Option<u64>,
    /// Multiple of the basic format (1 = basic, 3 = «A3×3», ГОСТ 2.301).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub multiple: u8,
    /// Portrait sheet: the main inscription along the short side.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub portrait: bool,
}

fn one() -> u8 {
    1
}

fn is_one(v: &u8) -> bool {
    *v == 1
}

pub const MAX_DRAWING_ITEMS: usize = 10_000;

/// Scale denominators allowed by ГОСТ 2.302 (1:N): reduction, natural size
/// and enlargement (2:1 = 0.5 … 100:1 = 0.01).
pub const STANDARD_SCALES: [f64; 27] = [
    0.01, 0.02, 0.025, 0.05, 0.1, 0.2, 0.25, 0.4, 0.5, 1., 2., 2.5, 4., 5., 10., 15., 20., 25.,
    40., 50., 75., 100., 200., 400., 500., 800., 1000.,
];

/// Whether a denominator is one of the ГОСТ 2.302 scales.
pub fn standard_scale(denominator: f64) -> bool {
    STANDARD_SCALES
        .iter()
        .any(|s| (s - denominator).abs() <= 1e-9 * s.max(1.))
}

impl Drawing {
    /// Sheet width × height in millimetres for its format, multiple and
    /// orientation. A multiple lengthens the short side of the basic format.
    pub fn sheet_mm(&self) -> [f64; 2] {
        let [long, short] = self.sheet.landscape_mm();
        let [w, h] = if self.multiple > 1 {
            [short * self.multiple as f64, long]
        } else {
            [long, short]
        };
        if self.portrait { [h, w] } else { [w, h] }
    }

    /// Format designation for graph 26: «A3», «A3×3».
    pub fn format_label(&self) -> String {
        if self.multiple > 1 {
            format!("{}×{}", self.sheet.label(), self.multiple)
        } else {
            self.sheet.label().to_string()
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let invalid = |m: &str| Err(ModelError::InvalidValue(format!("Чертёж: {m}")));
        let text = |s: &str| s.len() <= 4096 && !s.contains('\0');
        if !text(&self.number) || !text(&self.title) || self.number.trim().is_empty() {
            return invalid("обозначение или наименование");
        }
        if !standard_scale(self.scale) {
            return invalid("масштаб не из ряда ГОСТ 2.302");
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
        if self.multiple != 1 && !self.sheet.multiples().contains(&self.multiple) {
            return invalid("кратность формата вне ГОСТ 2.301");
        }
        let paper = self.sheet_mm();
        let on_sheet = |p: [f64; 2]| {
            p.iter().all(|v| v.is_finite())
                && (0. ..=paper[0]).contains(&p[0])
                && (0. ..=paper[1]).contains(&p[1])
        };
        if self.objects.len() > MAX_DRAWING_ITEMS {
            return invalid("количество объектов");
        }
        let mut object_ids = std::collections::BTreeSet::new();
        for object in &self.objects {
            if !object_ids.insert(object.id) {
                return invalid("повтор ID объекта");
            }
            object.validate(paper)?;
        }
        let mut ids = std::collections::BTreeSet::new();
        for view in &self.views {
            if !ids.insert(view.id) || !text(&view.name) || !on_sheet(view.position) {
                return invalid("вид");
            }
            if view
                .section
                .is_some_and(|v| !v.is_finite() || v.abs() > 1e9)
                || view.scale.is_some_and(|s| !standard_scale(s))
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
