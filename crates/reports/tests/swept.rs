use open_bim_model::{ElementShape, SweptPart, example_steel_frame};

#[test]
fn bent_plate_report_uses_the_developed_blank_dimensions() {
    let mut project = example_steel_frame().unwrap();
    let id = project
        .elements
        .values()
        .find(|e| matches!(e.shape, ElementShape::Plate { .. }))
        .unwrap()
        .id;
    let thickness = match &project.elements[&id].shape {
        ElementShape::Plate { thickness, .. } => thickness.value(),
        _ => unreachable!(),
    };
    project.swept_parts.insert(
        id,
        SweptPart::BentPlate {
            width: 100.,
            first_length: 200.,
            second_length: 150.,
            radius: 30.,
            angle: std::f64::consts::FRAC_PI_2,
        },
    );
    let (section, length) = open_bim_reports::section(&project, id).unwrap();
    assert_eq!(
        section,
        format!("—{}×100", open_bim_reports::fmt(thickness))
    );
    assert!((length - (350. + (30. + thickness / 2.) * std::f64::consts::FRAC_PI_2)).abs() < 1e-9);
}
