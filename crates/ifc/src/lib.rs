//! IFC4 exchange of the declared subset: IfcBeam, IfcColumn, IfcPlate with
//! swept-solid bodies, parameterised profiles, materials, placements, GUIDs,
//! property sets and openings. See docs/ifc-support.md.
pub mod export;
pub mod import;
pub mod mapping;
pub mod merge;
pub mod step;

pub use export::{ExportReport, export};
pub use import::{ImportReport, import};

/// Element property that carries the IFC GlobalId across round trips.
pub const GUID_PROPERTY: &str = "IFC GUID";
/// Property set holding element properties.
pub const PSET_NAME: &str = "Open BIM";
/// Derived property set for Russian reviewers: section, its standard, steel
/// and mass. Export only; import rebuilds these from the model.
pub const GOST_PSET_NAME: &str = "Сведения по ГОСТ";
/// Element property with a code of the construction information classifier
/// (КСИ); exported as an IFC classification reference.
pub const KSI_PROPERTY: &str = "КСИ";
/// Element properties that choose the exported IFC class, its predefined
/// type and a user-defined type («Экспорт в IFC» of the property pane).
pub const IFC_ENTITY_PROPERTY: &str = "ifc_entity";
pub const IFC_PREDEFINED_PROPERTY: &str = "ifc_predefined_type";
pub const IFC_OBJECT_TYPE_PROPERTY: &str = "ifc_object_type";

/// IFC4 classes a part may be exported as, with their predefined types.
pub const ENTITIES: &[(&str, &[&str])] = &[
    (
        "IfcBeam",
        &[
            "BEAM",
            "JOIST",
            "HOLLOWCORE",
            "LINTEL",
            "SPANDREL",
            "T_BEAM",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcColumn",
        &["COLUMN", "PILASTER", "USERDEFINED", "NOTDEFINED"],
    ),
    (
        "IfcMember",
        &[
            "BRACE",
            "CHORD",
            "COLLAR",
            "MEMBER",
            "MULLION",
            "PLATE",
            "POST",
            "PURLIN",
            "RAFTER",
            "STRINGER",
            "STRUT",
            "STUD",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcPlate",
        &["CURTAIN_PANEL", "SHEET", "USERDEFINED", "NOTDEFINED"],
    ),
    (
        "IfcRailing",
        &[
            "HANDRAIL",
            "GUARDRAIL",
            "BALUSTRADE",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcFooting",
        &[
            "CAISSON_FOUNDATION",
            "FOOTING_BEAM",
            "PAD_FOOTING",
            "PILE_CAP",
            "STRIP_FOOTING",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcPile",
        &[
            "BORED",
            "DRIVEN",
            "JETGROUTING",
            "COHESION",
            "FRICTION",
            "SUPPORT",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcSlab",
        &[
            "FLOOR",
            "ROOF",
            "LANDING",
            "BASESLAB",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcBuildingElementProxy",
        &["COMPLEX", "ELEMENT", "PARTIAL", "USERDEFINED", "NOTDEFINED"],
    ),
];

/// Predefined types of an IFC class from [`ENTITIES`] (case-insensitive).
pub fn predefined_types(entity: &str) -> Option<&'static [&'static str]> {
    ENTITIES
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(entity))
        .map(|(_, t)| *t)
}
