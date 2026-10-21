//! Files of other programs: IFC2x3, metres, sections named their way or
//! written as outlines, shared (mapped) geometry, material lists, types and
//! assemblies. Sections and steels end up in the ГОСТ catalogue.
use open_bim_ifc::import;
use open_bim_model::{ElementShape, Project};

const FILE: &str = r"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('ViewDefinition [CoordinationView_V2.0]'),'2;1');
FILE_NAME('frame.ifc','2026-10-21T12:00:00',(''),(''),'','','');
FILE_SCHEMA(('IFC2X3'));
ENDSEC;
DATA;
#1=IFCPROJECT('0YvctVUKr0kugbFTf53O9L',$,'\X2\041F0440043E0435043A0442\X0\',$,$,$,$,(#20),#30);
#20=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#21,$);
#21=IFCAXIS2PLACEMENT3D(#22,$,$);
#22=IFCCARTESIANPOINT((0.,0.,0.));
#30=IFCUNITASSIGNMENT((#31));
#31=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCLOCALPLACEMENT($,#21);
#100=IFCBEAM('1aaaaaaaaaaaaaaaaaaaaa',$,'BEAM',$,'I20B1_26020_83',#101,#104,'B1');
#101=IFCLOCALPLACEMENT(#40,#102);
#102=IFCAXIS2PLACEMENT3D(#103,#160,#161);
#103=IFCCARTESIANPOINT((0.,0.,3.));
#160=IFCDIRECTION((1.,0.,0.));
#161=IFCDIRECTION((0.,1.,0.));
#162=IFCDIRECTION((0.,0.,1.));
#104=IFCPRODUCTDEFINITIONSHAPE($,$,(#105));
#105=IFCSHAPEREPRESENTATION(#20,'Body','SweptSolid',(#106));
#106=IFCEXTRUDEDAREASOLID(#107,#21,#162,6.);
#107=IFCISHAPEPROFILEDEF(.AREA.,'I20B1_26020_83',#108,0.1,0.2,0.0056,0.0085,0.012);
#108=IFCAXIS2PLACEMENT2D(#109,$);
#109=IFCCARTESIANPOINT((0.,0.));
#200=IFCMEMBER('2aaaaaaaaaaaaaaaaaaaaa',$,'\X2\04230433043E043B043E043A\X0\:L100x8:345',$,$,#201,#204,$);
#201=IFCLOCALPLACEMENT(#40,#202);
#202=IFCAXIS2PLACEMENT3D(#203,$,$);
#203=IFCCARTESIANPOINT((2.,1.,0.));
#204=IFCPRODUCTDEFINITIONSHAPE($,$,(#205));
#205=IFCSHAPEREPRESENTATION(#20,'Body','SweptSolid',(#206));
#206=IFCEXTRUDEDAREASOLID(#207,#21,#162,2.5);
#207=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#208);
#208=IFCPOLYLINE((#210,#211,#212,#213,#214,#215,#210));
#210=IFCCARTESIANPOINT((0.05,-0.05));
#211=IFCCARTESIANPOINT((0.15,-0.05));
#212=IFCCARTESIANPOINT((0.15,-0.042));
#213=IFCCARTESIANPOINT((0.058,-0.042));
#214=IFCCARTESIANPOINT((0.058,0.05));
#215=IFCCARTESIANPOINT((0.05,0.05));
#300=IFCCOLUMN('3aaaaaaaaaaaaaaaaaaaaa',$,'COLUMN',$,$,#301,#304,$);
#301=IFCLOCALPLACEMENT(#40,#302);
#302=IFCAXIS2PLACEMENT3D(#303,$,$);
#303=IFCCARTESIANPOINT((5.,0.,0.));
#304=IFCPRODUCTDEFINITIONSHAPE($,$,(#305));
#305=IFCSHAPEREPRESENTATION(#20,'Body','MappedRepresentation',(#306));
#306=IFCMAPPEDITEM(#307,#310);
#307=IFCREPRESENTATIONMAP(#21,#308);
#308=IFCSHAPEREPRESENTATION(#20,'Body','SweptSolid',(#309));
#309=IFCEXTRUDEDAREASOLID(#311,#21,#162,3.6);
#311=IFCRECTANGLEHOLLOWPROFILEDEF(.AREA.,$,#108,0.1,0.15,0.006,$,$);
#310=IFCCARTESIANTRANSFORMATIONOPERATOR3D($,$,#312,1.,$);
#312=IFCCARTESIANPOINT((0.,0.,0.5));
#400=IFCPLATE('0baaaaaaaaaaaaaaaaaaaa',$,'PLATE',$,$,#401,#404,$);
#401=IFCLOCALPLACEMENT(#40,#21);
#404=IFCPRODUCTDEFINITIONSHAPE($,$,(#405));
#405=IFCSHAPEREPRESENTATION(#20,'Body','SweptSolid',(#406));
#406=IFCEXTRUDEDAREASOLID(#407,#21,#162,0.01);
#407=IFCRECTANGLEPROFILEDEF(.AREA.,$,#108,0.2,0.3);
#500=IFCWALL('1baaaaaaaaaaaaaaaaaaaa',$,'Wall',$,$,$,$,$);
#600=IFCELEMENTASSEMBLY('2baaaaaaaaaaaaaaaaaaaa',$,'A-1',$,$,#40,$,$,.FACTORY.,.GIRDER.);
#601=IFCRELAGGREGATES('3baaaaaaaaaaaaaaaaaaaa',$,$,$,#600,(#100,#400));
#700=IFCMATERIAL('\X2\042104420430043B044C\X0\ C345');
#701=IFCMATERIALLIST((#700));
#702=IFCRELASSOCIATESMATERIAL('0caaaaaaaaaaaaaaaaaaaa',$,$,$,(#100,#300,#400),#701);
#800=IFCMEMBERTYPE('1caaaaaaaaaaaaaaaaaaaa',$,'L100x8',$,$,$,$,$,$,.BRACE.);
#801=IFCRELDEFINESBYTYPE('2caaaaaaaaaaaaaaaaaaaa',$,$,$,(#200),#800);
#802=IFCRELASSOCIATESMATERIAL('3caaaaaaaaaaaaaaaaaaaa',$,$,$,(#800),#803);
#803=IFCMATERIAL('C255');
ENDSEC;
END-ISO-10303-21;
";

fn by_name<'a>(p: &'a Project, name: &str) -> &'a open_bim_model::Element {
    p.elements.values().find(|e| e.name == name).unwrap()
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

#[test]
fn file_of_another_program_lands_in_the_gost_catalogue() {
    let (p, report) = import(FILE).unwrap();
    assert_eq!(p.name, "Проект");
    assert_eq!(report.length_scale, 1000.);
    assert_eq!(p.elements.len(), 4, "{:?}", report.issues);
    assert_eq!(report.not_imported.get("стены"), Some(&1));
    assert!(report.unmatched_profiles.is_empty(), "{report:?}");

    // Parametric section with a foreign designation.
    let beam = by_name(&p, "BEAM");
    let ElementShape::Member {
        profile, length, ..
    } = &beam.shape
    else {
        panic!()
    };
    assert_eq!(p.profiles[profile].name, "20Б1");
    assert_eq!(
        p.profiles[profile].standard.as_deref(),
        Some("ГОСТ 26020-83")
    );
    assert!((length.value() - 6000.).abs() < 1e-6);
    assert_eq!(
        report.profiles.get("I20B1_26020_83").map(String::as_str),
        Some("20Б1")
    );
    assert_eq!(p.materials[&beam.material].name, "С345 · ГОСТ 27772-2021");

    // Outline of an angle; material through the type.
    let brace = p
        .elements
        .values()
        .find(|e| e.name.contains("L100x8"))
        .unwrap();
    let ElementShape::Member {
        profile,
        length,
        frame,
        ..
    } = &brace.shape
    else {
        panic!("{:?}", brace.shape)
    };
    assert_eq!(p.profiles[profile].name, "L100×8");
    assert!((length.value() - 2500.).abs() < 1e-6);
    assert!(close(frame.origin.0, [2100., 1000., 0.]), "{frame:?}");
    assert!(close(frame.axes[0], [1., 0., 0.]) && close(frame.axes[2], [0., 0., 1.]));
    assert_eq!(p.materials[&brace.material].name, "С255 · ГОСТ 27772-2021");

    // Shared geometry placed by a transformation.
    let column = by_name(&p, "COLUMN");
    let ElementShape::Member {
        profile,
        length,
        frame,
        member_kind,
        ..
    } = &column.shape
    else {
        panic!()
    };
    assert_eq!(*member_kind, open_bim_model::MemberKind::Column);
    assert_eq!(p.profiles[profile].name, "Гн 150×100×6");
    assert!((length.value() - 3600.).abs() < 1e-6);
    assert!(close(frame.origin.0, [5000., 0., 500.]));

    let plate = by_name(&p, "PLATE");
    let ElementShape::Plate { thickness, .. } = &plate.shape else {
        panic!()
    };
    assert!((thickness.value() - 10.).abs() < 1e-9);

    // Assembly: beam and plate, the beam is the main part.
    assert_eq!(p.assemblies.len(), 1);
    let a = p.assemblies.values().next().unwrap();
    assert_eq!(a.name, "A-1");
    assert_eq!(a.main_part, beam.id);
    assert_eq!(a.members, [beam.id, plate.id].into());
    let summary = report.summary();
    assert!(
        summary.contains("балок 2, колонн 1, пластин 1, сборок 1"),
        "{summary}"
    );
    assert!(summary.contains("стены 1"), "{summary}");
}

#[test]
fn unknown_outline_keeps_its_geometry_and_bad_parts_do_not_sink_the_file() {
    // A T-shaped outline nothing in the catalogue matches, and a plate with
    // a degenerate contour.
    let text = FILE
        .replace(
            "#208=IFCPOLYLINE((#210,#211,#212,#213,#214,#215,#210));",
            "#208=IFCPOLYLINE((#210,#211,#212,#213,#210));",
        )
        .replace(
            "#407=IFCRECTANGLEPROFILEDEF(.AREA.,$,#108,0.2,0.3);",
            "#407=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#408);\n#408=IFCPOLYLINE((#109,#109,#109));",
        );
    let (p, report) = import(&text).unwrap();
    assert_eq!(p.elements.len(), 3, "{:?}", report.issues);
    assert_eq!(report.skipped, 1);
    let brace = p
        .elements
        .values()
        .find(|e| e.name.contains("L100x8"))
        .unwrap();
    assert!(matches!(brace.shape, ElementShape::Plate { .. }));
    assert!(report.unmatched_profiles.contains("L100x8"), "{report:?}");
}
