use open_bim_connections::{ConnectionTemplate, TemplateKind};

#[test]
fn open_template_fixtures_export_and_reject_invalid_parameters() {
    for (bytes, tag) in [
        (
            include_bytes!("../../../tests/fixtures/connections/base-plate.json").as_slice(),
            "base_plate",
        ),
        (
            include_bytes!("../../../tests/fixtures/connections/end-plate.json").as_slice(),
            "end_plate",
        ),
        (
            include_bytes!("../../../tests/fixtures/connections/clip-angle.json").as_slice(),
            "clip_angle",
        ),
    ] {
        let template = ConnectionTemplate::from_json(bytes).unwrap();
        let exported = template.to_json().unwrap();
        let independent: serde_json::Value = serde_json::from_str(&exported).unwrap();
        assert_eq!(independent["kind"]["type"], tag);
        assert_eq!(independent["bolt_diameter"], 20.0);
        assert_eq!(independent["bolt_standard"], "ISO 4014");
        assert_eq!(
            ConnectionTemplate::from_json(exported.as_bytes()).unwrap(),
            template
        );
        for value in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            let mut invalid = template.clone();
            invalid.thickness = value;
            assert!(invalid.to_json().is_err());
        }
        let mut invalid = template.clone();
        invalid.edge_distance = 5.0;
        assert!(invalid.validate().is_err());
        invalid = template.clone();
        invalid.version = 2;
        assert!(invalid.validate().is_err());
        let mut extra = independent.clone();
        extra["capacity_kn"] = serde_json::json!(100);
        assert!(ConnectionTemplate::from_json(extra.to_string().as_bytes()).is_err());
    }
    let mut angle = ConnectionTemplate::from_json(include_bytes!(
        "../../../tests/fixtures/connections/clip-angle.json"
    ))
    .unwrap();
    angle.kind = TemplateKind::ClipAngle {
        leg: 100.0,
        height: 170.0,
        pitch: 10.0,
    };
    assert!(angle.validate().is_err());
    assert!(ConnectionTemplate::from_json(&vec![b' '; 65537]).is_err());
}
