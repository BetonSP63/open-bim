use open_bim_model::*;

#[test]
fn catalog_areas_match_independent_reference_values() {
    let project = Project::default();
    let csv = include_str!("../../../tests/fixtures/model/profile-areas.csv");
    for line in csv.lines().skip(1) {
        let values: Vec<_> = line.split(',').collect();
        let id = ObjectId(values[0].parse().unwrap());
        let expected: f64 = values[2].parse().unwrap();
        let actual = project.profiles[&id].shape.area_mm2().unwrap();
        assert!(
            (actual - expected).abs() / expected < 1e-12,
            "{}: {actual} != {expected}",
            values[1]
        );
    }
}

#[test]
fn steel_frame_preserves_identity_geometry_and_properties() {
    let project = example_steel_frame().unwrap();
    let fixture = include_str!("../../../tests/fixtures/model/steel-frame.json");
    assert_eq!(Project::from_json(fixture).unwrap(), project);
    assert_eq!(project.elements.len(), 7);
    assert_eq!(project.axes.len(), 2);
    assert_eq!(project.levels.len(), 2);
    assert_eq!(project.assemblies.len(), 1);
    assert_eq!(
        Project::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
    let total = project
        .elements
        .keys()
        .map(|id| project.element_mass_kg(*id).unwrap())
        .sum::<f64>();
    assert!((total - 1119.3472).abs() < 1e-8);
    let ElementShape::Member { frame, length, .. } = &project.elements[&ObjectId(12)].shape else {
        panic!("Expected beam")
    };
    assert_eq!(
        frame.world(Point3([0.0, 0.0, length.value()])),
        Point3([6000.0, 0.0, 3000.0])
    );
    assert_eq!(project.assemblies[&ObjectId(15)].main_part, ObjectId(8));
    assert_eq!(
        project.elements[&ObjectId(14)].properties["Марка"],
        PropertyValue::Text("П-1".into())
    );
}

#[test]
fn rejects_duplicate_ids_in_json_instead_of_silent_overwrite() {
    let project = example_steel_frame().unwrap();
    let value = serde_json::to_string(&project.elements[&ObjectId(8)]).unwrap();
    let json = project.to_json().unwrap().replacen(
        "\"elements\": {",
        &format!("\"elements\": {{\"8\":{value},"),
        1,
    );
    assert!(matches!(
        Project::from_json(&json),
        Err(ModelError::Serialization(_))
    ));
    let mut corrupt = project.clone();
    corrupt.materials.insert(
        ObjectId(8),
        Material {
            id: ObjectId(8),
            name: "Duplicate".into(),
            density_kg_m3: 7850.0,
        },
    );
    assert_eq!(
        corrupt.validate(),
        Err(ModelError::DuplicateId(ObjectId(8)))
    );
}

#[test]
fn rejects_missing_references_and_negative_dimensions() {
    let mut project = example_steel_frame().unwrap();
    project.elements.get_mut(&ObjectId(8)).unwrap().material = ObjectId(999);
    assert_eq!(
        project.validate(),
        Err(ModelError::MissingReference(ObjectId(999)))
    );
    let mut data = serde_json::to_value(example_steel_frame().unwrap()).unwrap();
    data["elements"]["8"]["shape"]["length"] = serde_json::json!(-10.0);
    assert!(Project::from_json(&data.to_string()).is_err());
    let mut project = Project::default();
    project.profiles.get_mut(&ObjectId(3)).unwrap().shape = ProfileShape::Angle {
        height: 100.0,
        width: 75.0,
        thickness: -8.0,
    };
    assert!(project.validate().is_err());
}

#[test]
fn rejects_cycles_and_multiple_owners() {
    let mut project = example_steel_frame().unwrap();
    let second = project.allocate_id().unwrap();
    let mut assembly = project.assemblies[&ObjectId(15)].clone();
    assembly.id = second;
    project.assemblies.insert(second, assembly);
    assert_eq!(
        project.validate(),
        Err(ModelError::MultipleOwners(ObjectId(8)))
    );
    let child = project.assemblies.get_mut(&second).unwrap();
    child.members = std::collections::BTreeSet::from([ObjectId(9)]);
    child.main_part = ObjectId(9);
    child.children.insert(ObjectId(15));
    project
        .assemblies
        .get_mut(&ObjectId(15))
        .unwrap()
        .children
        .insert(second);
    assert!(matches!(
        project.validate(),
        Err(ModelError::OwnershipCycle(_))
    ));
}

#[test]
fn placement_validation_and_endpoint_mapping() {
    for end in [
        Point3([0.0, 0.0, 3000.0]),
        Point3([6000.0, 0.0, 0.0]),
        Point3([-100.0, 80.0, 900.0]),
    ] {
        let start = Point3([1.0, 2.0, 3.0]);
        let (frame, length) = Frame::along(start, end).unwrap();
        let actual = frame.world(Point3([0.0, 0.0, length.value()]));
        assert!(actual.distance(end) < 1e-9);
    }
    let mut frame = Frame::default();
    frame.axes[2][2] = -1.0;
    assert!(frame.validate().is_err());
    assert!(Frame::along(Point3([0.0; 3]), Point3([0.0; 3])).is_err());
    assert!(polygon_area(&[[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]]).is_err());
    assert_eq!(
        polygon_area(&[[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]).unwrap(),
        12.0
    );
}

#[test]
fn validated_deserialization_rejects_invalid_coordinates_and_counter() {
    let mut project = example_steel_frame().unwrap();
    project.origin = Point3([f64::INFINITY, 0.0, 0.0]);
    assert!(project.validate().is_err());
    let mut data = serde_json::to_value(example_steel_frame().unwrap()).unwrap();
    data["next_id"] = serde_json::json!(8);
    assert!(Project::from_json(&data.to_string()).is_err());
    data["next_id"] = serde_json::json!(100);
    data["units"] = serde_json::json!("metres");
    assert!(Project::from_json(&data.to_string()).is_err());
}
