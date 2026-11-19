//! IFC4.3 reinforcement of the slab sample: meshes, U-bars and frogs.
use std::collections::BTreeMap;
use std::path::PathBuf;

use open_bim_ifc::step::{self, Value};
use open_bim_ifc::{
    IFC_ENTITY_PROPERTY, IFC_PREDEFINED_PROPERTY, KSI_PROPERTY, Schema, export, export_schema,
    import,
};
use open_bim_model::concrete::{MeshLayer, SlabRebarType};
use open_bim_model::rebar::{self, detail_schedule};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, Material, ObjectId, Point3, Project, PropertyValue,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn slab() -> Project {
    let mut project = Project {
        name: "Плита КЖ".into(),
        next_id: 40,
        ..Project::default()
    };
    project.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2500.,
        },
    );
    let id = ObjectId(10);
    let mut properties = BTreeMap::new();
    properties.insert(
        IFC_ENTITY_PROPERTY.into(),
        PropertyValue::Text("IfcSlab".into()),
    );
    properties.insert(
        IFC_PREDEFINED_PROPERTY.into(),
        PropertyValue::Text("FLOOR".into()),
    );
    properties.insert(KSI_PROPERTY.into(), PropertyValue::Text("КЖ.01".into()));
    project.elements.insert(
        id,
        Element {
            id,
            name: "Перекрытие".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [6000., 0.], [6000., 4000.], [0., 4000.]],
                thickness: LengthMm::new(200.).unwrap(),
                frame: Frame {
                    origin: Point3([0.; 3]),
                    ..Frame::default()
                },
            },
            properties,
        },
    );
    let mut ty = SlabRebarType::new("s", "Плита с сетками");
    let layer = MeshLayer::new(10., 200., 0.);
    ty.bottom = Some(layer.clone());
    ty.top = Some(layer);
    ty.edge_diameter = 12.;
    ty.edge_grade = "А500С".into();
    ty.edge_spacing = 200.;
    ty.frog_diameter = 8.;
    ty.frog_grade = "А500С".into();
    ty.frog_spacing = 600.;
    project.concrete.slab_types.push(ty);
    project.concrete = rebar::install_slab(&project, id, "s").unwrap();
    project
}

fn kind(file: &step::File, id: u64) -> &str {
    file.entities[&id].kind.as_str()
}

fn count(file: &step::File, kind_name: &str) -> usize {
    file.entities
        .values()
        .filter(|e| e.kind == kind_name)
        .count()
}

fn ref_at(entity: &step::Entity, index: usize) -> u64 {
    entity.args[index].as_ref().unwrap()
}

fn list_refs(value: &Value) -> Vec<u64> {
    value
        .as_list()
        .unwrap()
        .iter()
        .map(|v| v.as_ref().unwrap())
        .collect()
}

fn reals(value: &Value) -> Vec<f64> {
    value
        .as_list()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

#[test]
fn slab_bars_are_typed_swept_disks_on_their_host() {
    let project = slab();
    let rows = detail_schedule(&project);
    let bar_count: usize = project.concrete.rebars.iter().map(|s| s.bars.len()).sum();
    assert!(bar_count > 20, "{bar_count}");
    assert!(rows.iter().any(|r| r.code == "ПР"));
    assert!(rows.iter().any(|r| r.code == "П"));
    assert!(rows.iter().any(|r| r.code == "Л"));

    let (ifc4, report4) = export(&project).unwrap();
    assert!(!ifc4.contains("IFCREINFORCING"));
    assert_eq!(report4.rebars, 0);
    assert_eq!(
        report4.rebar_not_in_schema.len(),
        project.concrete.rebars.len()
    );

    let (text, report) = export_schema(&project, Schema::Ifc4x3).unwrap();
    let path = fixtures().join("ifc/ifc4x3/rebar-slab.ifc");
    if std::env::var_os("OPEN_BIM_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(report.rebars, bar_count);
    assert!(report.rebar_not_in_schema.is_empty());
    assert!(text.contains("FILE_SCHEMA(('IFC4X3_ADD2'));"));
    assert!(!text.contains("IFCREINFORCINGMESH"));
    assert!(text.contains("Pset_ReinforcingBarCommon"));
    assert!(text.contains("Pset_MaterialCommon"));

    let file = step::parse(&text).unwrap();
    assert_eq!(count(&file, "IFCREINFORCINGBAR"), bar_count);
    assert_eq!(count(&file, "IFCREINFORCINGBARTYPE"), rows.len());
    assert_eq!(count(&file, "IFCSWEPTDISKSOLID"), bar_count);
    assert_eq!(count(&file, "IFCSWEPTDISKSOLIDPOLYGONAL"), 0);

    let codes: Vec<String> = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCREINFORCINGBARTYPE")
        .map(|e| e.args[14].as_str().unwrap().to_string())
        .collect();
    for code in ["ПР", "П", "Л"] {
        assert!(codes.iter().any(|c| c == code), "{code} in {codes:?}");
    }
    let frog = file
        .entities
        .values()
        .find(|e| e.kind == "IFCREINFORCINGBARTYPE" && e.args[14].as_str() == Some("Л"))
        .unwrap();
    assert_eq!(frog.args[9].as_enum(), Some("SPACEBAR"));
    let params = frog.args[15].as_list().unwrap();
    assert!(params.len() >= 4, "{params:?}");
    assert!(
        params
            .iter()
            .any(|p| matches!(p, Value::Typed(k, _) if k == "IFCLENGTHMEASURE")),
        "{params:?}"
    );
    assert!(
        params.iter().any(|p| match p {
            Value::Typed(k, v) if k == "IFCPLANEANGLEMEASURE" => v.as_f64().unwrap().abs() > 1.,
            _ => false,
        }),
        "{params:?}"
    );
    let edge = file
        .entities
        .values()
        .find(|e| e.kind == "IFCREINFORCINGBARTYPE" && e.args[14].as_str() == Some("П"))
        .unwrap();
    assert_eq!(edge.args[9].as_enum(), Some("EDGE"));
    // Nominal diameter and bar length sit on the type.
    assert!(edge.args[10].as_f64().unwrap() > 0.);
    assert!(edge.args[12].as_f64().unwrap() > 0.);

    let mut arcs = 0;
    let mut straight = 0;
    for bar in file
        .entities
        .values()
        .filter(|e| e.kind == "IFCREINFORCINGBAR")
    {
        let tag = bar.args[7].as_str().unwrap();
        assert!(tag.parse::<u32>().is_ok(), "{tag}");
        let grade = bar.args[8].as_str().unwrap();
        assert!(grade.contains('С') || grade.contains("500"), "{grade}");
        let shape = &file.entities[&ref_at(bar, 6)];
        let reps = list_refs(&shape.args[2]);
        let rep = &file.entities[&reps[0]];
        assert_eq!(rep.args[1].as_str(), Some("Body"));
        assert_eq!(rep.args[2].as_str(), Some("AdvancedSweptSolid"));
        let solid = &file.entities[&list_refs(&rep.args[3])[0]];
        assert_eq!(solid.kind, "IFCSWEPTDISKSOLID");
        assert!(solid.args[1].as_f64().unwrap() > 0.);
        let curve = &file.entities[&ref_at(solid, 0)];
        match curve.kind.as_str() {
            "IFCPOLYLINE" => {
                assert_eq!(list_refs(&curve.args[0]).len(), 2);
                straight += 1;
                let point = &file.entities[&list_refs(&curve.args[0])[0]];
                let xyz = reals(&point.args[0]);
                assert_eq!(xyz.len(), 3);
                assert!(xyz.iter().all(|v| v.is_finite()));
            }
            "IFCCOMPOSITECURVE" => {
                let segments = list_refs(&curve.args[0]);
                assert!(segments.len() >= 3, "{}", segments.len());
                let has_arc = segments.iter().any(|id| {
                    let parent = ref_at(&file.entities[id], 2);
                    kind(&file, parent) == "IFCTRIMMEDCURVE"
                        && kind(&file, ref_at(&file.entities[&parent], 0)) == "IFCCIRCLE"
                });
                assert!(has_arc);
                arcs += 1;
            }
            other => panic!("directrix {other}"),
        }
    }
    assert!(straight > 0 && arcs > 0, "straight {straight}, arcs {arcs}");

    let slab_id = file
        .entities
        .iter()
        .find(|(_, e)| e.kind == "IFCSLAB")
        .map(|(id, _)| *id)
        .unwrap();
    let aggregate = file
        .entities
        .values()
        .find(|e| e.kind == "IFCRELAGGREGATES" && ref_at(e, 4) == slab_id)
        .unwrap();
    let related = list_refs(&aggregate.args[5]);
    assert_eq!(related.len(), bar_count);
    assert!(
        related
            .iter()
            .all(|id| kind(&file, *id) == "IFCREINFORCINGBAR")
    );

    let contained: Vec<u64> = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCRELCONTAINEDINSPATIALSTRUCTURE")
        .flat_map(|e| list_refs(&e.args[4]))
        .collect();
    assert!(contained.contains(&slab_id));
    assert_eq!(
        contained
            .iter()
            .filter(|id| kind(&file, **id) == "IFCREINFORCINGBAR")
            .count(),
        bar_count
    );

    let typed = file
        .entities
        .values()
        .filter(|e| e.kind == "IFCRELDEFINESBYTYPE")
        .map(|e| list_refs(&e.args[4]).len())
        .sum::<usize>();
    assert_eq!(typed, bar_count);
    assert!(file.entities.values().any(|e| {
        e.kind == "IFCRELDEFINESBYTYPE" && kind(&file, ref_at(e, 5)) == "IFCREINFORCINGBARTYPE"
    }));

    let classified = file
        .entities
        .values()
        .find(|e| e.kind == "IFCRELASSOCIATESCLASSIFICATION")
        .unwrap();
    let classed = list_refs(&classified.args[4]);
    assert_eq!(classed, vec![slab_id]);

    // The importer does not rebuild bars. It names them and keeps going.
    let (back, imported) = import(&text).unwrap();
    assert!(back.concrete.rebars.is_empty());
    assert_eq!(
        imported.unsupported_entities.get("IFCREINFORCINGBAR"),
        Some(&bar_count)
    );
    assert_eq!(imported.not_imported.get("арматура"), Some(&bar_count));
    assert!(
        imported.issues.is_empty(),
        "{:?}",
        imported.issues.iter().take(5).collect::<Vec<_>>()
    );
}
