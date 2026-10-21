use open_bim_ifc::{GUID_PROPERTY, export, import, step};
use open_bim_model::{DetailKind, ElementShape, Project, PropertyValue};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn models() -> Vec<(&'static str, Project)> {
    let mut frame = open_bim_model::example_steel_frame().unwrap();
    let first = *frame.elements.keys().next().unwrap();
    frame.elements.get_mut(&first).unwrap().properties.insert(
        "Примечание".into(),
        PropertyValue::Text("Колонна «К-1», ось 'А'".into()),
    );
    vec![
        ("steel-frame", frame),
        (
            "connections",
            open_bim_persistence::load(fixtures().join("steel_release/source.obim")).unwrap(),
        ),
        (
            "drawing-control",
            open_bim_drawings::sample::control_project().project,
        ),
    ]
}

fn guid(p: &Project, id: open_bim_model::ObjectId) -> String {
    match p.elements[&id].properties.get(GUID_PROPERTY) {
        Some(PropertyValue::Text(g)) => g.clone(),
        _ => open_bim_ifc::export::element_guid(p, id),
    }
}

fn close(a: &[f64], b: &[f64], tolerance: f64) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tolerance)
}

#[test]
fn round_trip_keeps_guid_units_placement_dimensions_and_properties() {
    for (name, original) in models() {
        let (text, report) = export(&original).unwrap();
        let path = fixtures().join("ifc").join(format!("{name}.ifc"));
        if std::env::var_os("OPEN_BIM_BLESS").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "{name}.ifc");
        assert_eq!(
            report.beams + report.columns + report.plates,
            original.elements.len()
        );
        let (imported, import_report) = import(&text).unwrap();
        assert_eq!(import_report.length_scale, 1.);
        assert!(
            import_report.unsupported_entities.is_empty(),
            "{:?}",
            import_report.unsupported_entities
        );
        assert!(
            import_report.issues.is_empty(),
            "{name}: {:?}",
            import_report.issues
        );
        assert_eq!(imported.elements.len(), original.elements.len());
        let by_guid: BTreeMap<String, open_bim_model::ObjectId> = imported
            .elements
            .keys()
            .map(|id| (guid(&imported, *id), *id))
            .collect();
        for (id, a) in &original.elements {
            let b = &imported.elements[&by_guid[&guid(&original, *id)]];
            assert_eq!(a.name, b.name);
            let (ma, mb) = (
                &original.materials[&a.material],
                &imported.materials[&b.material],
            );
            assert_eq!(ma.name, mb.name);
            assert!((ma.density_kg_m3 - mb.density_kg_m3).abs() < 1e-9);
            let (fa, fb) = (a.shape.frame(), b.shape.frame());
            assert!(
                close(&fa.origin.0, &fb.origin.0, 0.01),
                "{name} #{} origin",
                id.0
            );
            for k in 0..3 {
                assert!(
                    close(&fa.axes[k], &fb.axes[k], 1e-9),
                    "{name} #{} axes",
                    id.0
                );
            }
            match (&a.shape, &b.shape) {
                (
                    ElementShape::Member {
                        profile: pa,
                        length: la,
                        member_kind: ka,
                        ..
                    },
                    ElementShape::Member {
                        profile: pb,
                        length: lb,
                        member_kind: kb,
                        ..
                    },
                ) => {
                    assert_eq!(ka, kb);
                    assert!((la.value() - lb.value()).abs() <= 0.01);
                    let sa = serde_json::to_value(&original.profiles[pa].shape).unwrap();
                    let sb = serde_json::to_value(&imported.profiles[pb].shape).unwrap();
                    assert_eq!(sa["kind"], sb["kind"]);
                    for (key, value) in sa.as_object().unwrap() {
                        if let Some(x) = value.as_f64() {
                            assert!((x - sb[key].as_f64().unwrap()).abs() <= 0.01, "{key}");
                        }
                    }
                }
                (
                    ElementShape::Plate {
                        contour: ca,
                        thickness: ta,
                        ..
                    },
                    ElementShape::Plate {
                        contour: cb,
                        thickness: tb,
                        ..
                    },
                ) => {
                    assert!((ta.value() - tb.value()).abs() <= 0.01);
                    assert_eq!(ca.len(), cb.len());
                    for (p, q) in ca.iter().zip(cb) {
                        assert!(close(p, q, 0.01));
                    }
                }
                _ => panic!("{name}: shape kind changed"),
            }
            let props = |p: &Project, e: &open_bim_model::Element| {
                let _ = p;
                e.properties
                    .iter()
                    .filter(|(k, _)| k.as_str() != GUID_PROPERTY)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<BTreeMap<_, _>>()
            };
            assert_eq!(props(&original, a), props(&imported, b));
            let holes = |p: &Project, part| {
                let mut h: Vec<(f64, [f64; 3])> = p
                    .details
                    .values()
                    .filter_map(|d| match &d.kind {
                        DetailKind::Hole {
                            part: q,
                            frame,
                            diameter,
                            ..
                        } if *q == part => Some((*diameter, frame.origin.0)),
                        _ => None,
                    })
                    .collect();
                h.sort_by(|x, y| x.partial_cmp(y).unwrap());
                h
            };
            let (ha, hb) = (holes(&original, *id), holes(&imported, b.id));
            assert_eq!(ha.len(), hb.len(), "{name} #{} holes", id.0);
            for (x, y) in ha.iter().zip(&hb) {
                assert!((x.0 - y.0).abs() <= 0.01 && close(&x.1, &y.1, 0.01));
            }
            // Net mass survives (holes as openings).
            let (va, vb) = (
                original.element_mass_kg(*id).unwrap(),
                imported.element_mass_kg(b.id).unwrap(),
            );
            assert!((va - vb).abs() < 1e-6, "{name} #{} mass {va} vs {vb}", id.0);
        }
        // Re-export keeps every GUID.
        let (again, _) = export(&imported).unwrap();
        let guids = |t: &str| {
            let file = step::parse(t).unwrap();
            let mut g: Vec<String> = file
                .entities
                .values()
                .filter(|e| matches!(e.kind.as_str(), "IFCBEAM" | "IFCCOLUMN" | "IFCPLATE"))
                .map(|e| e.args[0].as_str().unwrap().to_string())
                .collect();
            g.sort();
            g
        };
        assert_eq!(guids(&text), guids(&again), "{name}");
    }
}

#[test]
fn unsupported_content_is_reported_not_silently_dropped() {
    let source = open_bim_persistence::load(fixtures().join("steel_release/source.obim")).unwrap();
    let (text, report) = export(&source).unwrap();
    assert!(report.skipped.contains_key("bolt_group") && report.skipped.contains_key("weld"));
    assert!(report.skipped.contains_key("connection"));
    // Foreign entities and representations in an imported file.
    let foreign = text.replace(
        "ENDSEC;\nEND-ISO-10303-21;",
        "#900001=IFCWALL('0aaaaaaaaaaaaaaaaaaaaa',$,'Стена',$,$,$,$,$,$);\n#900002=IFCMECHANICALFASTENER('0bbbbbbbbbbbbbbbbbbbbb',$,$,$,$,$,$,$,$,$,$);\nENDSEC;\nEND-ISO-10303-21;",
    );
    let (_, r) = import(&foreign).unwrap();
    assert_eq!(r.unsupported_entities.get("IFCWALL"), Some(&1));
    assert_eq!(
        r.unsupported_entities.get("IFCMECHANICALFASTENER"),
        Some(&1)
    );
    assert!(
        import(
            "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC2X2_FINAL'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;"
        )
        .is_err()
    );
}

#[test]
fn step_strings_round_trip_cyrillic_quotes_and_backslashes() {
    let text = "Балка «Б-1» 'ось' \\ № 5";
    let encoded = step::encode(text);
    assert!(encoded.is_ascii());
    let file = step::parse(&format!(
        "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;#1=IFCLABEL({encoded});ENDSEC;END-ISO-10303-21;"
    ))
    .unwrap();
    assert_eq!(file.entities[&1].args[0].as_str(), Some(text));
    assert!(step::valid_guid(&open_bim_ifc::export::derived_guid(
        "x", 7
    )));
}

#[test]
fn gost_property_set_and_ksi_classification() {
    let mut frame = open_bim_model::example_steel_frame().unwrap();
    let first = *frame.elements.keys().next().unwrap();
    frame.elements.get_mut(&first).unwrap().properties.insert(
        open_bim_ifc::KSI_PROPERTY.into(),
        PropertyValue::Text("TEST-01".into()),
    );
    let (text, _) = export(&frame).unwrap();
    let file = step::parse(&text).unwrap();
    let sets: Vec<&str> = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCPROPERTYSET")
        .filter_map(|e| e.args.get(2).and_then(step::Value::as_str))
        .collect();
    assert_eq!(
        sets.iter()
            .filter(|s| **s == open_bim_ifc::GOST_PSET_NAME)
            .count(),
        frame.elements.len(),
        "every part carries the ГОСТ set"
    );
    let names: Vec<&str> = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCPROPERTYSINGLEVALUE")
        .filter_map(|e| e.args.first().and_then(step::Value::as_str))
        .collect();
    for name in ["Профиль", "Сталь", "Масса, кг"] {
        assert!(names.contains(&name), "{name}");
    }
    let kinds: Vec<&str> = file.entities.values().map(|e| e.kind.as_str()).collect();
    for kind in [
        "IFCCLASSIFICATION",
        "IFCCLASSIFICATIONREFERENCE",
        "IFCRELASSOCIATESCLASSIFICATION",
    ] {
        assert_eq!(kinds.iter().filter(|k| **k == kind).count(), 1, "{kind}");
    }
    // The derived set does not come back as element properties; the code
    // does, through the Open BIM set.
    let (back, report) = import(&text).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let e = back
        .elements
        .values()
        .find(|e| e.properties.contains_key(open_bim_ifc::KSI_PROPERTY))
        .unwrap();
    assert!(e.properties.keys().all(|k| !k.contains("ГОСТ")));
}
