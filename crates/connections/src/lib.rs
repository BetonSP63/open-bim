//! Open steel connection templates and ID-free geometry recipes.
pub use open_bim_model::{ConnectionInstance, ConnectionTemplate, TemplateKind};
mod base_plate;
pub use base_plate::{BasePlateGeometry, base_plate_geometry, rebuild_base_plates};
