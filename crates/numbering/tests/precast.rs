use open_bim_model::{
    Assembly, Element, ElementShape, Frame, LengthMm, Material, ObjectId, Point3, Project,
    PropertyValue,
    pours::{CAST_UNIT_KEY, CONCRETE_KEY, PRECAST},
};
use open_bim_numbering::{number, stored_marks};
use std::collections::{BTreeMap, BTreeSet};

fn slab(project: &mut Project, x: f64, length: f64, kind: &str) -> ObjectId {
    let part = ObjectId(project.next_id);
    let assembly = ObjectId(project.next_id + 1);
    project.next_id += 2;
    project.elements.insert(
        part,
        Element {
            id: part,
            name: "Плита".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                frame: Frame {
                    origin: Point3([x, 0., 0.]),
                    ..Frame::default()
                },
                contour: vec![[0., 0.], [length, 0.], [length, 1000.], [0., 1000.]],
                thickness: LengthMm::new(200.).unwrap(),
            },
            properties: BTreeMap::from([(CONCRETE_KEY.into(), PropertyValue::Text("slab".into()))]),
        },
    );
    project.assemblies.insert(
        assembly,
        Assembly {
            id: assembly,
            name: "Литая единица".into(),
            main_part: part,
            members: BTreeSet::from([part]),
            children: BTreeSet::new(),
            properties: BTreeMap::from([
                (CAST_UNIT_KEY.into(), PropertyValue::Text(kind.into())),
                ("designation".into(), PropertyValue::Text("Серия 1".into())),
            ]),
        },
    );
    assembly
}

#[test]
fn precast_marks_survive_save_and_move_and_drive_specification() {
    let mut project = Project::default();
    project.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2400.,
        },
    );
    let first = slab(&mut project, 0., 2000., PRECAST);
    let second = slab(&mut project, 2500., 2000., PRECAST);
    let different = slab(&mut project, 5000., 1500., PRECAST);
    slab(&mut project, 7500., 2000., "монолитный");
    let numbering = number(&project).unwrap();
    let mark = &numbering.assemblies[&first];
    assert!(mark.starts_with('П'));
    assert_eq!(mark, &numbering.assemblies[&second]);
    assert_ne!(mark, &numbering.assemblies[&different]);

    project.numbering = stored_marks(&numbering);
    let mut reopened = Project::from_json(&project.to_json().unwrap()).unwrap();
    let part = reopened.assemblies[&second].main_part;
    let ElementShape::Plate { frame, .. } = &mut reopened.elements.get_mut(&part).unwrap().shape
    else {
        panic!("expected plate")
    };
    frame.origin.0[0] += 200.;
    let marks = number(&reopened).unwrap().assemblies;
    assert_eq!(marks, numbering.assemblies);
    let rows = reopened.precast_rows(&marks).unwrap();
    assert_eq!(rows.len(), 2);
    let row = rows.iter().find(|row| &row.mark == mark).unwrap();
    assert_eq!(row.count, 2);
    assert_eq!(row.designation, "Серия 1");
    assert_eq!(row.note(), "В25, 0,400 м³");
    assert!((row.unit_mass_kg - 960.).abs() < 0.001);
}
