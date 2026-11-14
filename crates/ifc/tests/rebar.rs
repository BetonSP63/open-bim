use open_bim_ifc::{Schema, export_schema, step};
use open_bim_model::rebar::Hook;
use open_bim_model::{Point3, Project, RebarKind, RebarSet};

fn model() -> Project {
    let mut project = open_bim_model::example_steel_frame().unwrap();
    let host = *project.elements.keys().next().unwrap();
    project.concrete.rebars = vec![RebarSet {
        number: 7,
        kind: RebarKind::Guide,
        host: Some(host),
        diameter: 16.,
        grade: "А500С".into(),
        bars: vec![vec![
            Point3([100., 200., 300.]),
            Point3([1100., 200., 300.]),
            Point3([1100., 700., 300.]),
        ]],
        hooks: [Hook::default(); 2],
        bend_diameter: None,
        rule: None,
    }];
    project
}

#[test]
fn reinforcing_bar_has_swept_geometry_steel_and_position() {
    for schema in [Schema::Ifc4, Schema::Ifc4x3] {
        let (text, report) = export_schema(&model(), schema).unwrap();
        assert_eq!(report.rebars, 1);
        let file = step::parse(&text).unwrap();
        let bar = file
            .entities
            .values()
            .find(|e| e.kind == "IFCREINFORCINGBAR")
            .unwrap();
        assert_eq!(bar.args[8].as_f64(), Some(16.));
        assert!((bar.args[9].as_f64().unwrap() - std::f64::consts::PI * 64.).abs() < 1e-9);
        let expected_length =
            model().concrete.rebars[0].developed_length(&model().concrete.rebars[0].bars[0]);
        assert!((bar.args[10].as_f64().unwrap() - expected_length).abs() < 1e-9);
        assert_eq!(bar.args[11].as_enum(), Some("NOTDEFINED"));
        assert_eq!(bar.args[12].as_enum(), Some("RIBBED"));
        assert_eq!(bar.args[13].as_str(), Some("А500С"));
        assert!(text.contains("IFCSWEPTDISKSOLID("));
        for (name, value) in [("Позиция", "1"), ("Арматурный набор", "7")] {
            assert!(file.entities.values().any(|e| {
                e.kind == "IFCPROPERTYSINGLEVALUE"
                    && e.args[0].as_str() == Some(name)
                    && e.args[2].as_str() == Some(value)
            }));
        }
        assert!(
            file.entities
                .values()
                .any(|e| { e.kind == "IFCMATERIAL" && e.args[0].as_str() == Some("А500С") })
        );
        assert!(file.entities.values().any(|e| {
            e.kind == "IFCRELAGGREGATES"
                && e.args[4]
                    .as_ref()
                    .is_some_and(|id| file.entities[&id].kind == "IFCCOLUMN")
        }));
    }
}

#[test]
fn multiple_physical_bars_keep_separate_deterministic_guids() {
    let mut project = model();
    project.concrete.rebars[0]
        .bars
        .push(vec![Point3([0., 0., 0.]), Point3([1000., 0., 0.])]);
    let (first, report) = export_schema(&project, Schema::Ifc4).unwrap();
    let (second, _) = export_schema(&project, Schema::Ifc4).unwrap();
    assert_eq!(report.rebars, 2);
    assert_eq!(first, second);
    let file = step::parse(&first).unwrap();
    let bars: Vec<_> = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCREINFORCINGBAR")
        .collect();
    assert_eq!(bars.len(), 2);
    assert_ne!(bars[0].args[0], bars[1].args[0]);
}

#[test]
fn exported_bars_round_trip_into_the_original_set_and_host() {
    for schema in [Schema::Ifc4, Schema::Ifc4x3] {
        let original = model();
        let (text, _) = export_schema(&original, schema).unwrap();
        let (imported, report) = open_bim_ifc::import(&text).unwrap();
        assert_eq!(report.imported["IFCREINFORCINGBAR"], 1);
        assert!(
            report.unsupported_entities.is_empty(),
            "{:?}",
            report.unsupported_entities
        );
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert!(report.summary().contains("стержней 1"));
        assert_eq!(imported.concrete.rebars.len(), 1);
        let before = &original.concrete.rebars[0];
        let after = &imported.concrete.rebars[0];
        assert_eq!(after.number, before.number);
        assert_eq!(after.diameter, before.diameter);
        assert_eq!(after.grade, before.grade);
        assert!(
            after
                .host
                .is_some_and(|host| imported.elements.contains_key(&host))
        );
        assert_eq!(after.bars.len(), before.bars.len());
        for (a, b) in before.bars[0].iter().zip(&after.bars[0]) {
            for k in 0..3 {
                assert!((a.0[k] - b.0[k]).abs() < 1e-8);
            }
        }
        let (again, export_report) = export_schema(&imported, schema).unwrap();
        assert_eq!(export_report.rebars, 1);
        assert!(again.contains("IFCREINFORCINGBAR"));
    }
}
