//! Serializable BIM domain; no dependency on desktop, drawing or GPU libraries.
pub mod analysis;
mod body;
mod catalog;
mod changes;
pub mod concrete;
mod connection;
mod construction;
mod custom_component;
mod detailing;
mod document_info;
mod drawing;
mod drawing_objects;
mod fabrication;
pub mod family;
pub mod formula;
mod frame;
pub mod library;
pub mod management;
mod model_views;
pub mod numbering_rules;
pub mod parametric;
mod plugin;
mod position;
mod project;
pub mod sections;
pub mod shapes;
mod swept;
mod validation;
mod view_settings;
pub mod weld;

pub use analysis::{
    Analysis, AnalysisModel, Load, LoadGroup, LoadKind, LoadShape, Members, PartAnalysis, PartRole,
    RigidLink,
};
pub use catalog::{Material, Profile, ProfileShape};
pub use changes::{Change, REASONS, SheetChange, SheetChangeKind};
pub use concrete::{Concrete, FormPanel, PourBreak, PourUnit, Prop, RebarKind, RebarSet};
pub use connection::{
    ComponentGeneral, ConnectionInstance, ConnectionTemplate, HAUNCH_ROLES, SEAT_ROLES,
    TemplateKind, beam_splice_roles, gusset_roles,
};
pub use construction::{Construction, ConstructionKind, DimPoint, ModelDimension};
pub use custom_component::{
    BUILTIN_NAMES, CustomBolts, CustomDefinition, CustomPlate, PlatePlane, Variable, WeldTo,
    example_definition,
};
pub use detailing::{Detail, DetailKind, bolt_positions, bolt_set, circle_points};
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
pub use family::{
    Axis, BodyArray, BodyShape, Family, FamilyBody, FamilyInstance, FamilyParameter, FamilyPart,
    FamilyType, FamilyValues, LocalShape, ParamKind, Placement, RefPlane, SpecMode,
    rebuild_families, sample_families,
};
pub use frame::{Frame, GridAxis, GridSettings, Level};
pub use management::{Hierarchy, ListEntry, Management, Storey, Task};
pub use open_bim_geometry::{GeometryError, LengthMm, Point3};
pub use parametric::{Binding, ParamField, Parametric};
pub use plugin::{MAX_PLUGIN_INPUTS, PluginInput, PluginInstance, PluginValue};
pub use position::{AtDepth, OnPlane, PartPosition, Rotation, place, reference_line};
pub use project::{
    Assembly, Element, ElementShape, ID_BLOCK, MemberKind, ObjectId, Project, PropertyValue, Units,
    example_steel_frame,
};
pub use sections::{
    SectionDefinition, SectionPlate, SectionWeld, Sections, WeldedPiece, WeldedSeam,
};
pub use shapes::{ItemShape, ShapePart, Shapes};
pub use swept::SweptPart;
pub use validation::{ModelError, polygon_area};
pub use weld::{
    Intermittent, Joint, JointKind, LEG_SIGN, WeldGost, WeldProcess, WeldSides, WeldType,
    weld_advice, weld_mark, weld_notes, welded_length,
};

pub use view_settings::{LinePen, ViewAppearance};

pub use model_views::{ModelViewDefinition, ModelViewKind, ModelViewRender};
