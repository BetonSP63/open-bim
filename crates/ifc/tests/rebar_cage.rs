//! IFC4.3 roles of a beam cage, a straight tie and column starters.
use std::collections::BTreeMap;

use open_bim_ifc::step;
use open_bim_ifc::{Schema, export_schema};
use open_bim_model::concrete::{CageType, REBAR_COVER, StarterPath};
use open_bim_model::rebar::{install_cage, install_starter};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId, Point3, Profile,
    ProfileShape, Project, PropertyValue,
};

fn member(project: &mut Project, id: ObjectId, name: &str, kind: MemberKind, length: f64) {
    let mut properties = BTreeMap::new();
    properties.insert(REBAR_COVER.into(), PropertyValue::Number(30.));
    project.elements.insert(
        id,
        Element {
            id,
            name: name.into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: kind,
                profile: ObjectId(2),
                length: LengthMm::new(length).unwrap(),
                frame: Frame {
                    origin: Point3([id.0 as f64 * 1000., 0., 0.]),
                    ..Frame::default()
                },
                position: Default::default(),
            },
            properties,
        },
    );
}

fn project() -> Project {
    let mut project = Project {
        name: "Каркас КЖ".into(),
        next_id: 40,
        ..Project::default()
    };
    project.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2500.,
        },
    );
    project.profiles.insert(
        ObjectId(2),
        Profile {
            id: ObjectId(2),
            name: "400×600".into(),
            shape: ProfileShape::Rectangle {
                height: 600.,
                width: 400.,
            },
            source: "тест".into(),
            standard: None,
            area_mm2: None,
        },
    );
    member(&mut project, ObjectId(10), "Балка", MemberKind::Beam, 3000.);
    member(
        &mut project,
        ObjectId(11),
        "Колонна",
        MemberKind::Column,
        3000.,
    );
    let mut closed = CageType::new("closed", "Каркас с хомутами");
    closed.long_diameter = 20.;
    closed.stirrup_diameter = 10.;
    closed.stirrup_spacing = 200.;
    let mut studs = CageType::new("studs", "Каркас со шпильками");
    studs.long_diameter = 16.;
    studs.stirrup_diameter = 8.;
    studs.stirrup_shape = "gost.straight".into();
    studs.stirrup_spacing = 200.;
    project.concrete.cage_types.push(closed);
    project.concrete.cage_types.push(studs);
    project.concrete = install_cage(&project, ObjectId(10), "closed").unwrap();
    project.concrete = install_cage(&project, ObjectId(11), "studs").unwrap();
    project.concrete = install_starter(
        &project,
        ObjectId(11),
        StarterPath::ColumnTop,
        "gost.straight",
        12.,
        "А500С",
        200.,
    )
    .unwrap();
    project
}

fn enums(file: &step::File, kind: &str) -> Vec<String> {
    file.entities
        .values()
        .filter(|e| e.kind == kind)
        .flat_map(|e| {
            e.args
                .iter()
                .filter_map(|a| a.as_enum().map(str::to_string))
        })
        .collect()
}

#[test]
fn cage_bars_export_as_main_ligature_stud_and_anchoring() {
    let project = project();
    let (text, report) = export_schema(&project, Schema::Ifc4x3).unwrap();
    let bars: usize = project.concrete.rebars.iter().map(|s| s.bars.len()).sum();
    assert_eq!(report.rebars, bars);
    assert!(report.rebar_not_in_schema.is_empty());
    let file = step::parse(&text).unwrap();
    let roles = enums(&file, "IFCREINFORCINGBAR");
    for role in ["MAIN", "LIGATURE", "STUD", "ANCHORING"] {
        assert!(roles.iter().any(|r| r == role), "{role} in {roles:?}");
    }
    let stirrup = file
        .entities
        .values()
        .find(|e| e.kind == "IFCREINFORCINGBARTYPE" && e.args[14].as_str() == Some("Х"))
        .unwrap();
    assert_eq!(stirrup.args[9].as_enum(), Some("LIGATURE"));
    assert!(file.entities.values().any(|e| {
        e.kind == "IFCREINFORCINGBARTYPE" && e.args[9].as_enum() == Some("ANCHORING")
    }));
}
