//! «Обработка поверхности»: parts grouped by treatment with their outer area.
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project, PropertyValue,
};
use open_bim_reports::{SURFACE_TREATMENT, generate};

fn plate(id: u64, x: f64, treatment: Option<&str>) -> Element {
    Element {
        id: ObjectId(id),
        name: "Пластина".into(),
        material: ObjectId(1),
        shape: ElementShape::Plate {
            contour: vec![[0., 0.], [400., 0.], [400., 300.], [0., 300.]],
            thickness: LengthMm::new(20.).unwrap(),
            frame: Frame {
                origin: Point3([x, 0., 0.]),
                ..Frame::default()
            },
        },
        properties: treatment
            .map(|t| {
                [(SURFACE_TREATMENT.to_string(), PropertyValue::Text(t.into()))]
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[test]
fn area_per_treatment() {
    let mut p = Project::default();
    for (id, x, t) in [
        (20, 0., Some("Горячее цинкование")),
        (21, 1000., Some("Горячее цинкование")),
        (22, 2000., Some("Грунт ГФ-021")),
        (23, 3000., None),
    ] {
        p.elements.insert(ObjectId(id), plate(id, x, t));
    }
    p.next_id = 24;
    let r = generate(&p).unwrap();
    assert_eq!(r.coatings.len(), 2);
    // 2 × 400 × 300 + 2 × (400 + 300) × 20 = 268 000 mm² per plate.
    let zinc = r
        .coatings
        .iter()
        .find(|c| c.treatment == "Горячее цинкование")
        .unwrap();
    assert_eq!(zinc.quantity, 2);
    assert!((zinc.area_m2 - 0.536).abs() < 1e-9, "{zinc:?}");
    let primer = r.coatings.iter().find(|c| c.quantity == 1).unwrap();
    assert!((primer.area_m2 - 0.268).abs() < 1e-9);
    let csv = open_bim_reports::csv::coatings(&r);
    assert!(csv.contains("Горячее цинкование"), "{csv}");
    // No treatments: nothing new in the release data.
    p.elements.values_mut().for_each(|e| e.properties.clear());
    let json = serde_json::to_string(&generate(&p).unwrap()).unwrap();
    assert!(!json.contains("coatings"));
}
