//! Serializable BIM domain; no dependency on desktop, drawing or GPU libraries.
mod body;
mod catalog;
mod changes;
mod connection;
mod construction;
mod detailing;
mod document_info;
mod drawing;
mod drawing_objects;
mod frame;
pub mod numbering_rules;
mod project;
mod validation;

pub use catalog::{Material, Profile, ProfileShape};
pub use changes::{Change, REASONS, SheetChange, SheetChangeKind};
pub use connection::{ConnectionInstance, ConnectionTemplate, TemplateKind};
pub use construction::{Construction, ConstructionKind, DimPoint, ModelDimension};
pub use detailing::{Detail, DetailKind, WeldSides, WeldType, weld_mark};
pub use document_info::{
    DEFAULT_BASE, DocumentInfo, MAX_APPROVALS, MAX_GENERAL_NOTES, MAX_OTHER_SIGNERS, Person, Stage,
    format_date, parse_date,
};
pub use drawing::{
    Anchor, AnchorPoint, DimensionAxis, DimensionDef, Drawing, DrawingKind, DrawingView,
    MAX_DRAWING_ITEMS, SET_MARKS, STANDARD_SCALES, SheetSize, TextNote, ViewDirection,
    standard_scale,
};
pub use drawing_objects::{HatchPattern, LineType, SheetObject, SheetObjectKind};
pub use frame::{Frame, GridAxis, Level};
pub use open_bim_geometry::{GeometryError, LengthMm, Point3};
pub use project::{
    Assembly, Element, ElementShape, MemberKind, ObjectId, Project, PropertyValue, Units,
    example_steel_frame,
};
pub use validation::{ModelError, polygon_area};
