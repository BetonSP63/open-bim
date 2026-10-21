//! Custom components: plates, bolts and welds laid out by formulas at the
//! end of the main part.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    CustomBolts, CustomDefinition, CustomPlate, DetailKind, Element, ElementShape, Frame,
    MemberKind, ObjectId, PlatePlane, Point3, Project, TemplateKind, Variable, WeldTo,
    example_definition,
};

const COLUMN: ObjectId = ObjectId(8);
const BEAM: ObjectId = ObjectId(9);

fn member(id: ObjectId, kind: MemberKind, start: [f64; 3], end: [f64; 3]) -> Element {
    let (frame, length) = Frame::along(Point3(start), Point3(end)).unwrap();
    Element {
        id,
        name: "Деталь".into(),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: kind,
            profile: ObjectId(2),
            length,
            frame,
            position: Default::default(),
        },
    }
}

/// Column I 300×150×8×12 along Z, flange faces at Y = ±150; beam I 300 at
/// Z = 3000 between `start` and `end` along Y.
fn fixture(start: f64, end: f64) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        COLUMN,
        member(COLUMN, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]),
    );
    p.elements.insert(
        BEAM,
        member(BEAM, MemberKind::Beam, [0., start, 3000.], [0., end, 3000.]),
    );
    p.next_id = 10;
    p.validate().unwrap();
    p
}

fn template(definition: CustomDefinition) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Свой".into(),
        kind: TemplateKind::Custom { definition },
        thickness: 12.,
        edge_distance: 45.,
        bolt_diameter: 20.,
        clearance: 2.,
        bolt_standard: "ГОСТ 7798-70".into(),
        bolt_grade: "5.8".into(),
        weld_size: 6.,
    }
}

fn attempt(doc: &mut Document, at_end: bool, t: ConnectionTemplate) -> Result<ObjectId, String> {
    let id = ObjectId(doc.project().next_id);
    doc.transact(
        "Свой компонент",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: BEAM,
                secondary: Some(COLUMN),
                at_end,
                template: t,
                children: Default::default(),
                general: Default::default(),
            },
        }],
    )
    .map(|_| id)
    .map_err(|e| e.to_string())
}

fn world_points(p: &Project, id: ObjectId) -> Vec<[f64; 3]> {
    let ElementShape::Plate {
        contour,
        thickness,
        frame,
    } = &p.elements[&id].shape
    else {
        panic!()
    };
    contour
        .iter()
        .flat_map(|q| [0., thickness.value()].map(|z| frame.world(Point3([q[0], q[1], z])).0))
        .collect()
}

fn span(points: &[[f64; 3]], axis: usize) -> (f64, f64) {
    points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p[axis]), hi.max(p[axis]))
    })
}

#[test]
fn the_example_end_plate_is_sized_from_the_section_at_either_end() {
    // The plate (20 mm = max(t, d)) fills the gap between the beam end and
    // the column face at Y = 150.
    for (start, end, at_end) in [(170., 6000., false), (6000., 170., true)] {
        let mut doc = Document::new(fixture(start, end)).unwrap();
        let id = attempt(&mut doc, at_end, template(example_definition())).unwrap();
        let p = doc.project();
        let c = &p.connections[&id];
        assert_eq!(c.children.len(), example_definition().roles().len());
        let plate = c.children["plate-0"];
        let pts = world_points(p, plate);
        // 150 + 2·65 wide, 300 + 2·65 high, 20 thick, outside the beam end.
        let (x, y, z) = (span(&pts, 0), span(&pts, 1), span(&pts, 2));
        assert!(
            (x.0 + 140.).abs() < 1e-6 && (x.1 - 140.).abs() < 1e-6,
            "{x:?}"
        );
        assert!(
            (y.0 - 150.).abs() < 1e-6 && (y.1 - 170.).abs() < 1e-6,
            "{y:?}"
        );
        assert!(
            (z.0 - 2785.).abs() < 1e-6 && (z.1 - 3215.).abs() < 1e-6,
            "{z:?}"
        );
        let DetailKind::BoltGroup {
            parts,
            holes,
            steps_x,
            steps_y,
            ..
        } = &p.details[&c.children["bolts-0"]].kind
        else {
            panic!()
        };
        assert_eq!(parts.len(), 1);
        assert_eq!(holes.len(), 4);
        assert_eq!((steps_x.clone(), steps_y.clone()), (vec![190.], vec![340.]));
        let DetailKind::Weld { parts, path, .. } = &p.details[&c.children["weld-0"]].kind else {
            panic!()
        };
        assert!(parts.contains(&BEAM) && parts.contains(&plate));
        assert_eq!(path.len(), 2);
        // Four Ø22 holes come out of the 280 × 430 × 20 plate.
        let v = p.element_volume_mm3(plate).unwrap();
        let full = 280. * 430. * 20.;
        let holes = 4. * std::f64::consts::PI * 11. * 11. * 20.;
        assert!((v - (full - holes)).abs() < 0.01 * full, "{v}");
    }
}

fn web_plate(through_main: bool) -> CustomDefinition {
    let s = |v: &str| v.to_string();
    CustomDefinition {
        variables: vec![Variable {
            name: s("высота"),
            label: s("Высота пластины"),
            formula: s("floor(0.6 * (h - 2 * tf) / 10) * 10"),
        }],
        plates: vec![CustomPlate {
            name: s("Пластина у стенки"),
            plane: PlatePlane::Web,
            offset: s("tw / 2"),
            contour: vec![
                [s("-20"), s("-высота / 2")],
                [s("100"), s("-высота / 2")],
                [s("100"), s("высота / 2")],
                [s("-20"), s("высота / 2")],
            ],
            thickness: s("t"),
            weld: Some(WeldTo::Support),
        }],
        bolts: vec![CustomBolts {
            plate: 0,
            x: s("50"),
            y: s("-35"),
            count_x: 1,
            count_y: 2,
            spacing_x: s("0"),
            spacing_y: s("70"),
            through_main,
        }],
    }
}

#[test]
fn a_web_plate_drills_the_beam_web_when_asked_and_welds_to_the_support() {
    for at_end in [false, true] {
        let (start, end) = if at_end { (6000., 170.) } else { (170., 6000.) };
        for through in [false, true] {
            let mut doc = Document::new(fixture(start, end)).unwrap();
            let id = attempt(&mut doc, at_end, template(web_plate(through))).unwrap();
            let p = doc.project();
            let c = &p.connections[&id];
            let in_beam = p
                .details
                .values()
                .filter(|d| matches!(d.kind, DetailKind::Hole { part, .. } if part == BEAM))
                .count();
            assert_eq!(in_beam, if through { 2 } else { 0 });
            // 0,6 · 276 → 160 high, beside the web (x from 4 to 16).
            let pts = world_points(p, c.children["plate-0"]);
            let (x, z) = (span(&pts, 0), span(&pts, 2));
            assert!((x.1 - x.0 - 12.).abs() < 1e-6, "{x:?}");
            assert!(x.0.abs() - 4. < 1e-6 || x.1.abs() - 4. < 1e-6, "{x:?}");
            assert!((z.1 - z.0 - 160.).abs() < 1e-6, "{z:?}");
            let DetailKind::Weld { parts, .. } = &p.details[&c.children["weld-0"]].kind else {
                panic!()
            };
            assert!(parts.contains(&COLUMN));
            p.element_body(BEAM).unwrap();
        }
    }
}

#[test]
fn a_changed_formula_rebuilds_in_place_and_a_wrong_one_is_refused() {
    let mut doc = Document::new(fixture(170., 6000.)).unwrap();
    let id = attempt(&mut doc, false, template(web_plate(true))).unwrap();
    let plate = doc.project().connections[&id].children["plate-0"];
    let height = |doc: &Document| span(&world_points(doc.project(), plate), 2);
    let mut c = doc.project().connections[&id].clone();
    let TemplateKind::Custom { definition } = &mut c.template.kind else {
        panic!()
    };
    definition.variables[0].formula = "200".into();
    doc.transact(
        "Изменить узел",
        &[Command::ReplaceConnection {
            connection: c.clone(),
        }],
    )
    .unwrap();
    assert!((height(&doc).1 - height(&doc).0 - 200.).abs() < 1e-6);
    assert_eq!(doc.project().connections[&id].children["plate-0"], plate);
    doc.undo().unwrap();
    assert!((height(&doc).1 - height(&doc).0 - 160.).abs() < 1e-6);

    let TemplateKind::Custom { definition } = &mut c.template.kind else {
        panic!()
    };
    definition.variables[0].formula = "ширина * 2".into();
    let err = doc
        .transact(
            "Изменить узел",
            &[Command::ReplaceConnection { connection: c }],
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("высота") && err.contains("«ширина»"), "{err}");

    // Bolts outside the plate are refused too.
    let mut def = web_plate(false);
    def.bolts[0].x = "500".into();
    let mut doc = Document::new(fixture(170., 6000.)).unwrap();
    let err = attempt(&mut doc, false, template(def)).unwrap_err();
    assert!(err.contains("вне пластины"), "{err}");
}

#[test]
fn the_definition_survives_a_template_file_and_a_saved_project() {
    let t = template(web_plate(true));
    let back = ConnectionTemplate::from_json(t.to_json().unwrap().as_bytes()).unwrap();
    assert_eq!(back, t);
    let mut doc = Document::new(fixture(170., 6000.)).unwrap();
    attempt(&mut doc, false, t).unwrap();
    let json = serde_json::to_string(doc.project()).unwrap();
    let loaded: Project = serde_json::from_str(&json).unwrap();
    loaded.validate().unwrap();
    assert_eq!(&loaded, doc.project());
}
