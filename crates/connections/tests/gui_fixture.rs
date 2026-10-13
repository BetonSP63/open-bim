use open_bim_model::ObjectId;

#[test]
fn actual_native_saved_connections_match_independent_reference_masses() {
    let project = open_bim_persistence::decode(include_bytes!(
        "../../../tests/connections_flows/gui-project.obim"
    ))
    .unwrap();
    let verification: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/connections_flows/verification-results.json"
    ))
    .unwrap();
    assert_eq!(project.connections.len(), 6);
    assert_eq!(project.elements.len(), 16);
    assert_eq!(project.details.len(), 52);
    for row in verification["reference_masses"].as_array().unwrap() {
        let id = ObjectId(row["part"].as_u64().unwrap());
        let expected = row["mass_kg"].as_f64().unwrap();
        let mass = project.element_mass_kg(id).unwrap();
        assert!(
            (mass / expected - 1.).abs() <= 1e-6,
            "part{id}: {mass} vs{expected}"
        );
    }
    assert_eq!(
        project.elements[&ObjectId(8)].shape.frame().origin.0[0],
        100.
    );
    assert_eq!(
        project.elements[&ObjectId(41)].shape.frame().origin.0,
        [-45., -220., -25.]
    );
}
