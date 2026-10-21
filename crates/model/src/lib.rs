//! Serializable BIM domain; no dependency on desktop, drawing or GPU libraries.
mod body;
mod catalog;
mod changes;
mod connection;
mod construction;
mod custom_component;
mod detailing;
mod document_info;
mod drawing;
mod drawing_objects;
pub mod formula;
mod frame;
pub mod library;
pub mod numbering_rules;
mod position;
mod project;
mod validation;

pub use catalog::{Material, Profile, ProfileShape};
pub use changes::{Change, REASONS, SheetChange, SheetChangeKind};
pub use connection::{
    ComponentGeneral, ConnectionInstance, ConnectionTemplate, HAUNCH_ROLES, SEAT_ROLES,
    TemplateKind, beam_splice_roles, gusset_roles,
};
pub use construction::{Construction, ConstructionKind, DimPoint, ModelDimension};
pub use custom_component::{
    BUILTIN_NAMES, CustomBolts, CustomDefinition, CustomPlate, PlatePlane, Variable, WeldTo,
    example_definition,
};
pub use detailing::{
    Detail, DetailKind, LEG_SIGN, WeldProcess, WeldSides, WeldType, bolt_positions, bolt_set,
    circle_points, weld_mark,
};
pub use document_info::{
    DEFAULT_BASE, DocumentInfo, MAX_APPROVALS, MAX_GENERAL_NOTES, MAX_OTHER_SIGNERS, Person, Stage,
    format_date, parse_date,
};
pub use drawing::{
    Anchor, AnchorPoint, DimensionAxis, DimensionDef, Drawing, DrawingKind, DrawingView,
    MAX_DRAWING_ITEMS, NodeRegion, SET_MARKS, STANDARD_SCALES, SheetSize, TextNote, ViewDirection,
    standard_scale,
};
pub use drawing_objects::{HatchPattern, LineType, SheetObject, SheetObjectKind};
pub use frame::{Frame, GridAxis, Level};
pub use open_bim_geometry::{GeometryError, LengthMm, Point3};
pub use position::{AtDepth, OnPlane, PartPosition, Rotation, place, reference_line};
pub use project::{
    Assembly, Element, ElementShape, MemberKind, ObjectId, Project, PropertyValue, Units,
    example_steel_frame,
};
pub use validation::{ModelError, polygon_area};
