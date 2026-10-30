//! IFC4 and IFC4.3 exchange of the declared subset: IfcBeam, IfcColumn,
//! IfcPlate with swept-solid bodies, parameterised profiles, materials,
//! placements, GUIDs, property sets and openings. See docs/ifc-support.md.
pub mod export;
pub mod import;
pub mod mapping;
pub mod merge;
pub mod step;

pub use export::{ExportReport, export, export_schema};
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

/// Schema of an exported file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Schema {
    /// IFC4 ADD2 TC1.
    #[default]
    Ifc4,
    /// IFC4.3 ADD2 (ISO 16739-1:2024).
    Ifc4x3,
}

impl Schema {
    /// Name in FILE_SCHEMA.
    pub fn name(self) -> &'static str {
        match self {
            Schema::Ifc4 => "IFC4",
            Schema::Ifc4x3 => "IFC4X3_ADD2",
        }
    }
    /// Name for messages.
    pub fn label(self) -> &'static str {
        match self {
            Schema::Ifc4 => "IFC4",
            Schema::Ifc4x3 => "IFC4.3",
        }
    }
}

/// Classes and predefined types that only IFC4.3 has. An IFC4 export writes
/// such a type as USERDEFINED with the type name as ObjectType, and such a
/// class as IfcBuildingElementProxy.
pub const IFC4X3_ONLY: &[&str] = &[
    "IfcBearing",
    "CORNICE",
    "DIAPHRAGM",
    "EDGEBEAM",
    "GIRDER_SEGMENT",
    "HATSTONE",
    "PIERCAP",
    "PIERSTEM",
    "PIERSTEM_SEGMENT",
    "STANDCOLUMN",
    "ARCH_SEGMENT",
    "STAY_CABLE",
    "STIFFENING_RIB",
    "STRUCTURALCABLE",
    "SUSPENDER",
    "SUSPENSION_CABLE",
    "TIEBAR",
    "BASE_PLATE",
    "COVER_PLATE",
    "FLANGE_PLATE",
    "GUSSET_PLATE",
    "SPLICE_PLATE",
    "STIFFENER_PLATE",
    "WEB_PLATE",
    "FENCE",
    "APPROACH_SLAB",
    "PAVING",
    "SIDEWALK",
    "TRACKSLAB",
    "WEARING",
];

/// Whether a class or predefined type exists only in IFC4.3.
pub fn ifc4x3_only(name: &str) -> bool {
    IFC4X3_ONLY.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// IFC classes a part may be exported as, with their predefined types (IFC4
/// and IFC4.3; see [`IFC4X3_ONLY`]).
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
            "CORNICE",
            "DIAPHRAGM",
            "EDGEBEAM",
            "GIRDER_SEGMENT",
            "HATSTONE",
            "PIERCAP",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcColumn",
        &[
            "COLUMN",
            "PILASTER",
            "PIERSTEM",
            "PIERSTEM_SEGMENT",
            "STANDCOLUMN",
            "USERDEFINED",
            "NOTDEFINED",
        ],
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
            "ARCH_SEGMENT",
            "STAY_CABLE",
            "STIFFENING_RIB",
            "STRUCTURALCABLE",
            "SUSPENDER",
            "SUSPENSION_CABLE",
            "TIEBAR",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcPlate",
        &[
            "CURTAIN_PANEL",
            "SHEET",
            "BASE_PLATE",
            "COVER_PLATE",
            "FLANGE_PLATE",
            "GUSSET_PLATE",
            "SPLICE_PLATE",
            "STIFFENER_PLATE",
            "WEB_PLATE",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcRailing",
        &[
            "HANDRAIL",
            "GUARDRAIL",
            "BALUSTRADE",
            "FENCE",
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
            "APPROACH_SLAB",
            "PAVING",
            "SIDEWALK",
            "TRACKSLAB",
            "WEARING",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcBuildingElementProxy",
        &["COMPLEX", "ELEMENT", "PARTIAL", "USERDEFINED", "NOTDEFINED"],
    ),
    (
        "IfcBearing",
        &[
            "CYLINDRICAL",
            "DISK",
            "ELASTOMERIC",
            "GUIDE",
            "POT",
            "ROCKER",
            "ROLLER",
            "SPHERICAL",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
];

/// Predefined types of an IFC class from [`ENTITIES`] (case-insensitive).
pub fn predefined_types(entity: &str) -> Option<&'static [&'static str]> {
    ENTITIES
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(entity))
        .map(|(_, t)| *t)
}
