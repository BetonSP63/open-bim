use open_bim_model::ObjectId;
use open_bim_steel::{FabricationRow, fabrication_rows};

#[test]
fn genuine_gui_fixture_has_expected_mass_and_fabrication_rows() {
    let project = open_bim_persistence::decode(include_bytes!(
        "../../../tests/steel_flows/gui-project.obim"
    ))
    .unwrap();
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/steel_flows/verification-results.json"
    ))
    .unwrap();
    for row in reference["reference_parts"].as_array().unwrap() {
        let id = ObjectId(row["part"].as_u64().unwrap());
        let expected = row["mass_kg"].as_f64().unwrap();
        let mass = project.element_mass_kg(id).unwrap();
        assert!((mass - expected).abs() / expected <= 1e-6);
    }
    let rows = fabrication_rows(&project).unwrap();
    assert!(
        matches!(&rows[0],FabricationRow::Bolts {diameter_mm:20.,count:4,standard,grade,parts,..} if standard=="ISO 4014" && grade=="8.8" && parts==&std::collections::BTreeSet::from([ObjectId(8),ObjectId(10)]))
    );
    let FabricationRow::Weld {
        length_mm,
        size_mm: 6.,
        shop: true,
        ..
    } = &rows[1]
    else {
        panic!("Expected weld report row")
    };
    assert!((length_mm - reference["weld_length_mm"].as_f64().unwrap()).abs() < 1e-9);
}
