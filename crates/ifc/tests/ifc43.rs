//! IFC4.3 (IFC4X3_ADD2): export of the same subset, its predefined types and
//! IfcBearing, IFC4 fallbacks, and import of IFC4.3 files of other programs.
use open_bim_ifc::{
    IFC_ENTITY_PROPERTY, IFC_OBJECT_TYPE_PROPERTY, IFC_PREDEFINED_PROPERTY, Schema, export,
    export_schema, import,
};
use open_bim_model::{ElementShape, Project, PropertyValue};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn connections() -> Project {
    open_bim_persistence::load(fixtures().join("steel_release/source.obim")).unwrap()
}

fn data(text: &str) -> &str {
    &text[text.find("DATA;").unwrap()..]
}

fn text(t: &str) -> PropertyValue {
    PropertyValue::Text(t.into())
}

fn first_plate(p: &Project) -> open_bim_model::ObjectId {
    *p.elements
        .iter()
        .find(|(_, e)| matches!(e.shape, ElementShape::Plate { .. }))
        .unwrap()
        .0
}

#[test]
fn ifc4x3_file_has_the_same_data_and_reads_back_cleanly() {
    let models = [
        (
            "steel-frame",
            open_bim_model::example_steel_frame().unwrap(),
        ),
        ("connections", connections()),
        (
            "drawing-control",
            open_bim_drawings::sample::control_project().project,
        ),
    ];
    for (name, project) in models {
        let (ifc4, _) = export(&project).unwrap();
        let (ifc43, report) = export_schema(&project, Schema::Ifc4x3).unwrap();
        let path = fixtures().join("ifc/ifc4x3").join(format!("{name}.ifc"));
        if std::env::var_os("OPEN_BIM_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &ifc43).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), ifc43, "{name}");
        assert!(ifc43.contains("FILE_SCHEMA(('IFC4X3_ADD2'));"));
        assert_eq!(report.schema, Schema::Ifc4x3);
        assert!(report.downgraded.is_empty());
        // Only the optional attribute IFC4.3 added to IfcDerivedUnit differs.
        let differ: Vec<_> = data(&ifc4)
            .lines()
            .zip(data(&ifc43).lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(differ.len(), 1, "{name}: {differ:?}");
        assert!(differ[0].0.contains("IFCDERIVEDUNIT(") && differ[0].1.ends_with(",$,$);"));
        let (back, import_report) = import(&ifc43).unwrap();
        assert_eq!(import_report.schema, ["IFC4X3_ADD2"]);
        assert!(
            import_report.issues.is_empty(),
            "{name}: {:?}",
            import_report.issues
        );
        assert!(import_report.unsupported_entities.is_empty());
        assert_eq!(back.elements.len(), project.elements.len());
    }
}

#[test]
fn ifc4x3_types_are_written_natively_and_userdefined_in_ifc4() {
    let mut p = connections();
    let plate = first_plate(&p);
    let column = *p
        .elements
        .iter()
        .find(|(_, e)| !matches!(e.shape, ElementShape::Plate { .. }))
        .unwrap()
        .0;
    let props = &mut p.elements.get_mut(&plate).unwrap().properties;
    props.insert(IFC_PREDEFINED_PROPERTY.into(), text("BASE_PLATE"));
    let props = &mut p.elements.get_mut(&column).unwrap().properties;
    props.insert(IFC_ENTITY_PROPERTY.into(), text("IfcBearing"));
    props.insert(IFC_PREDEFINED_PROPERTY.into(), text("POT"));

    let (ifc43, report) = export_schema(&p, Schema::Ifc4x3).unwrap();
    let (ifc4, report4) = export(&p).unwrap();
    // Both go to the independent check (verify.py).
    let dir = fixtures().join("ifc/ifc4x3");
    for (name, file) in [("types.ifc", &ifc43), ("types-as-ifc4.ifc", &ifc4)] {
        if std::env::var_os("OPEN_BIM_BLESS").is_some() {
            std::fs::write(dir.join(name), file).unwrap();
        }
        assert_eq!(
            &std::fs::read_to_string(dir.join(name)).unwrap(),
            file,
            "{name}"
        );
    }
    assert!(report.downgraded.is_empty());
    assert!(ifc43.contains(",.BASE_PLATE.);"));
    assert!(
        ifc43
            .lines()
            .any(|l| l.contains("IFCBEARING(") && l.ends_with(",.POT.);"))
    );

    assert_eq!(
        report4.downgraded,
        vec![column.min(plate), column.max(plate)]
    );
    assert!(!ifc4.contains("BASE_PLATE.") && !ifc4.contains("IFCBEARING("));
    assert!(ifc4.lines().any(|l| l.contains("IFCPLATE(")
        && l.contains(",'BASE_PLATE',")
        && l.ends_with(",.USERDEFINED.);")));
    assert!(ifc4.lines().any(|l| l.contains("IFCBUILDINGELEMENTPROXY(")
        && l.contains(",'POT',")
        && l.ends_with(",.USERDEFINED.);")));

    // Both files give the plate back its IFC4.3 type.
    for file in [&ifc43, &ifc4] {
        let (back, _) = import(file).unwrap();
        let types: Vec<_> = back
            .elements
            .values()
            .filter_map(|e| e.properties.get(IFC_PREDEFINED_PROPERTY))
            .collect();
        assert_eq!(types, [&text("BASE_PLATE")]);
        assert!(
            back.elements
                .values()
                .all(|e| !e.properties.contains_key(IFC_OBJECT_TYPE_PROPERTY))
        );
    }
}

#[test]
fn curved_beam_tessellation_is_written_for_ifc4x3() {
    let mut project = open_bim_model::example_steel_frame().unwrap();
    let id = *project.elements.keys().next().unwrap();
    project.swept_parts.insert(
        id,
        open_bim_model::SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
    );
    let (ifc43, report) = export_schema(&project, Schema::Ifc4x3).unwrap();
    let path = fixtures().join("ifc/ifc4x3/curved.ifc");
    if std::env::var_os("OPEN_BIM_BLESS").is_some() {
        std::fs::write(&path, &ifc43).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), ifc43);
    assert_eq!(report.tessellated, vec![id]);
    // IFC4.3 adds TagList to the point list.
    assert!(
        ifc43
            .lines()
            .any(|l| l.contains("IFCCARTESIANPOINTLIST3D(") && l.ends_with("),$);"))
    );
}

#[test]
fn foreign_ifc4x3_bridge_file_is_imported() {
    let file = "ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('ViewDefinition [Alignment-basedReferenceView]'),'2;1');
FILE_NAME('bridge.ifc','2026-01-01T00:00:00',(''),(''),'','','');
FILE_SCHEMA(('IFC4X3_ADD2'));
ENDSEC;
DATA;
#1=IFCCARTESIANPOINT((0.,0.,0.));
#2=IFCDIRECTION((0.,0.,1.));
#3=IFCDIRECTION((1.,0.,0.));
#4=IFCAXIS2PLACEMENT3D(#1,#2,#3);
#5=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#4,$);
#6=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#7=IFCUNITASSIGNMENT((#6));
#8=IFCPROJECT('0YvctVUKr0kugbFTf53O9L',$,'Мост',$,$,$,$,(#5),#7);
#9=IFCLOCALPLACEMENT($,#4);
#10=IFCSITE('1YvctVUKr0kugbFTf53O9L',$,'Площадка',$,$,#9,$,$,.ELEMENT.,$,$,$,$,$);
#11=IFCLOCALPLACEMENT(#9,#4);
#12=IFCBRIDGE('2YvctVUKr0kugbFTf53O9L',$,'Путепровод',$,$,#11,$,$,.ELEMENT.,.GIRDER.);
#13=IFCBRIDGEPART('3YvctVUKr0kugbFTf53O9L',$,'Пролёт 1',$,$,#11,$,$,.ELEMENT.,.LATERAL.,.SUPERSTRUCTURE.);
#14=IFCRELAGGREGATES('0ZvctVUKr0kugbFTf53O9L',$,$,$,#8,(#10));
#15=IFCRELAGGREGATES('1ZvctVUKr0kugbFTf53O9L',$,$,$,#10,(#12));
#16=IFCRELAGGREGATES('2ZvctVUKr0kugbFTf53O9L',$,$,$,#12,(#13));
#17=IFCISHAPEPROFILEDEF(.AREA.,'I60Ш1',$,0.32,0.58,0.012,0.017,$,$,$);
#18=IFCEXTRUDEDAREASOLID(#17,#4,#2,24.);
#19=IFCSHAPEREPRESENTATION(#5,'Body','SweptSolid',(#18));
#20=IFCPRODUCTDEFINITIONSHAPE($,$,(#19));
#21=IFCCARTESIANPOINT((0.,0.,1.));
#22=IFCDIRECTION((1.,0.,0.));
#23=IFCDIRECTION((0.,1.,0.));
#24=IFCAXIS2PLACEMENT3D(#21,#22,#23);
#25=IFCLOCALPLACEMENT(#11,#24);
#26=IFCBEAM('0BvctVUKr0kugbFTf53O9L',$,'Главная балка',$,$,#25,#20,$,.GIRDER_SEGMENT.);
#27=IFCBEARING('0CvctVUKr0kugbFTf53O9L',$,'Опорная часть',$,$,#25,$,$,.POT.);
#28=IFCALIGNMENT('0DvctVUKr0kugbFTf53O9L',$,'Ось',$,$,#9,$,.NOTDEFINED.);
#29=IFCRELCONTAINEDINSPATIALSTRUCTURE('3ZvctVUKr0kugbFTf53O9L',$,$,$,(#26,#27),#13);
#30=IFCMATERIAL('С345',$,$);
#31=IFCRELASSOCIATESMATERIAL('0EvctVUKr0kugbFTf53O9L',$,$,$,(#26),#30);
ENDSEC;
END-ISO-10303-21;
";
    let (project, report) = import(file).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(project.elements.len(), 1);
    let beam = project.elements.values().next().unwrap();
    assert_eq!(beam.name, "Главная балка");
    assert_eq!(
        beam.properties.get(IFC_PREDEFINED_PROPERTY),
        Some(&text("GIRDER_SEGMENT"))
    );
    let ElementShape::Member { length, .. } = &beam.shape else {
        panic!("{:?}", beam.shape)
    };
    assert!((length.value() - 24000.).abs() < 1e-6);
    assert_eq!(
        report.profiles.get("I60Ш1").map(String::as_str),
        Some("60Ш1")
    );
    // Facilities hold elements; infrastructure objects are listed in words.
    assert!(
        !report.unsupported_entities.contains_key("IFCBRIDGE")
            && !report.unsupported_entities.contains_key("IFCBRIDGEPART"),
        "{:?}",
        report.unsupported_entities
    );
    assert_eq!(report.not_imported.get("опорные части"), Some(&1));
    assert_eq!(report.not_imported.get("трассы"), Some(&1));
    // On the next export the girder keeps its IFC4.3 type.
    let (ifc43, _) = export_schema(&project, Schema::Ifc4x3).unwrap();
    assert!(ifc43.contains(".GIRDER_SEGMENT.);"));
}

#[test]
fn draft_schemas_between_ifc4_and_ifc4x3_are_flagged() {
    let (ifc, _) = export(&open_bim_model::example_steel_frame().unwrap()).unwrap();
    let draft = ifc.replace("FILE_SCHEMA(('IFC4'))", "FILE_SCHEMA(('IFC4X1'))");
    let (_, report) = import(&draft).unwrap();
    assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
    assert!(report.issues[0].contains("IFC4X1"));
}
