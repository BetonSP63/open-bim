//! Serializable BIM domain; no dependency on desktop, drawing or GPU libraries.
mod body;
mod catalog;
mod detailing;
mod frame;
mod project;
mod validation;

pub use catalog::{Material, Profile, ProfileShape};
pub use detailing::{Detail, DetailKind, WeldType};
pub use frame::{Frame, GridAxis, Level};
pub use open_bim_geometry::{GeometryError, LengthMm, Point3};
pub use project::{
    Assembly, Element, ElementShape, MemberKind, ObjectId, Project, PropertyValue, Units,
    example_steel_frame,
};
pub use validation::{ModelError, polygon_area};
