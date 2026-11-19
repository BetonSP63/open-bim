//! Wall meshes and U-bars travel with the host into IFC4.3. IFC4 skips them,
//! the same way it skips slab bars.
use std::collections::BTreeMap;

use open_bim_ifc::{Schema, export, export_schema, step};
use open_bim_model::concrete::{REBAR_COVER, WallFace, WallRebarType};
use open_bim_model::pours::CONCRETE_KEY;
use open_bim_model::rebar::{self, detail_schedule, install_wall};
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId,
    Point3, Profile, ProfileShape, Project, PropertyValue,
};

fn wall() -> Project {
    let mut project = Project {
        name: "Стена КЖ".into(),
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
            name: "200x1200".into(),
            shape: ProfileShape::Rectangle {
                width: 200.,
                height: 1200.,
            },
            source: "Бетонное сечение".into(),
            standard: None,
            area_mm2: None,
        },
    );
    let id = ObjectId(10);
    let mut properties = BTreeMap::new();
    properties.insert(REBAR_COVER.into(), PropertyValue::Number(30.));
    properties.insert(CONCRETE_KEY.into(), PropertyValue::Text("panel".into()));
    project.elements.insert(
        id,
        Element {
            id,
            name: "Стена".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(2),
                length: LengthMm::new(1800.).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
            properties,
        },
    );
    project.details.insert(
        ObjectId(11),
        Detail {
            id: ObjectId(11),
            kind: DetailKind::PrismCut {
                part: id,
                frame: Frame {
                    origin: Point3([120., -150., 600.]),
                    axes: [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
                },
                contour: vec![[0., 0.], [400., 0.], [400., 300.], [0., 300.]],
                length: 240.,
            },
        },
    );
    let mut ty = WallRebarType::new("w", "Стена");
    ty.near = Some(WallFace::new(10., 200., 8., 200.));
    ty.far = Some(WallFace::new(10., 200., 8., 200.));
    ty.cut_openings = true;
    project.concrete.wall_types.push(ty);
    project.concrete = install_wall(&project, id, "w").unwrap();
    project
}

fn count(file: &step::File, kind_name: &str) -> usize {
    file.entities
        .values()
        .filter(|e| e.kind == kind_name)
        .count()
}

#[test]
fn wall_bars_are_typed_reinforcing_bars() {
    let project = wall();
    let rows = detail_schedule(&project);
    let bar_count: usize = project.concrete.rebars.iter().map(|s| s.bars.len()).sum();
    assert!(bar_count > 8, "{bar_count}");
    assert!(rows.iter().any(|r| r.code == "ПР"));
    assert!(rows.iter().any(|r| r.code == "П"));
    let bvbs = rebar::bvbs::file(&project);
    assert!(bvbs.contains(&format!("@p{}@", rows[0].mark)));

    let (ifc4, report4) = export(&project).unwrap();
    assert!(!ifc4.contains("IFCREINFORCING"));
    assert_eq!(report4.rebars, 0);

    let (text, report) = export_schema(&project, Schema::Ifc4x3).unwrap();
    assert_eq!(report.rebars, bar_count);
    assert!(report.rebar_not_in_schema.is_empty());
    let file = step::parse(&text).unwrap();
    assert_eq!(count(&file, "IFCREINFORCINGBAR"), bar_count);
    assert_eq!(count(&file, "IFCREINFORCINGBARTYPE"), rows.len());
    let codes: Vec<String> = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCREINFORCINGBARTYPE")
        .map(|e| e.args[14].as_str().unwrap().to_string())
        .collect();
    for code in ["ПР", "П"] {
        assert!(codes.iter().any(|c| c == code), "{code} in {codes:?}");
    }
}
