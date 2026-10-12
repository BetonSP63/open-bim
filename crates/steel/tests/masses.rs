use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project,
};

#[test]
fn fabrication_mass_matches_independent_reference_values() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/steel/plate-masses.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let mut p = Project::default();
        p.elements.insert(
            ObjectId(8),
            Element {
                id: ObjectId(8),
                name: "Reference plate".into(),
                material: ObjectId(1),
                properties: Default::default(),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
                    thickness: LengthMm::new(20.).unwrap(),
                    frame: Frame::default(),
                },
            },
        );
        let feature = match row["case"].as_str().unwrap() {
            "plain" => None,
            "hole" => Some(DetailKind::Hole {
                part: ObjectId(8),
                frame: Frame {
                    origin: Point3([50., 50., -1.]),
                    ..Frame::default()
                },
                diameter: 20.,
                depth: 22.,
            }),
            "box_cut" => Some(DetailKind::BoxCut {
                part: ObjectId(8),
                origin: Point3([20., 20., -1.]),
                size: [20., 20., 22.],
            }),
            "plane_cut" => Some(DetailKind::PlaneCut {
                part: ObjectId(8),
                normal: [1., 1., 0.],
                offset: 100.,
            }),
            _ => panic!("Unknown reference case"),
        };
        if let Some(kind) = feature {
            p.details.insert(
                ObjectId(9),
                Detail {
                    id: ObjectId(9),
                    kind,
                },
            );
        }
        p.next_id = 10;
        p.validate().unwrap();
        let expected = row["volume_mm3"].as_f64().unwrap();
        let volume = p.element_volume_mm3(ObjectId(8)).unwrap();
        assert!(
            (volume - expected).abs() / expected <= 1e-6,
            "{}: {volume} != {expected}",
            row["case"]
        );
        let mass = p.element_mass_kg(ObjectId(8)).unwrap();
        assert!((mass - expected * 7850. / 1e9).abs() / mass <= 1e-6);
        let restored =
            open_bim_persistence::decode(&open_bim_persistence::encode(&p).unwrap()).unwrap();
        assert_eq!(restored, p);
    }
}
