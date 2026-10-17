//! IFC4 exchange of the declared subset: IfcBeam, IfcColumn, IfcPlate with
//! swept-solid bodies, parameterised profiles, materials, placements, GUIDs,
//! property sets and openings. See docs/ifc-support.md.
pub mod export;
pub mod import;
pub mod step;

pub use export::{ExportReport, export};
pub use import::{ImportReport, import};

/// Element property that carries the IFC GlobalId across round trips.
pub const GUID_PROPERTY: &str = "IFC GUID";
/// Property set holding element properties.
pub const PSET_NAME: &str = "Open BIM";
