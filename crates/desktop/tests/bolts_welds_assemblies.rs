//! Bolts by parts and points with «3*80» spacing and a preview, welds with a
//! type choice that join assemblies and show on drawings, assembly commands,
//! gusset and stiffener presets, and the help texts.
use open_bim_desktop::interaction::tools::{bolt_axes, bolt_pattern, with_default_grid};
use open_bim_desktop::tools::detailing_ui::spacings;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd, palette::STEEL_WORKFLOWS};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project, PropertyValue,
    WeldSides, WeldType,
};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

/// The analytic test sections these geometry checks are written for
/// (profile 2 is I 300×150×8×12), with the default grid; new models in the
/// app start with ГОСТ sections instead.
fn default_project() -> Result<open_bim_model::Project, String> {
    with_default_grid(open_bim_model::Project::default())
}

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            if let Err(e) = b.editor.commit(&label, &commands) {
                panic!("{label}: {e}");
            }
            true
        }
        _ => false,
    }
}

fn p(b: &Workbench) -> &Project {
    b.editor.document.project()
}

fn last(b: &Workbench) -> ObjectId {
    *p(b).elements.keys().last().unwrap()
}

fn member(b: &mut Workbench, tool: Cmd, points: &[DVec3]) -> ObjectId {
    b.execute(tool);
    let mut r = None;
    for q in points {
        r = b.pick(*q);
    }
    assert!(apply(b, r));
    b.execute(Cmd::CancelTool);
    last(b)
}

/// A 400 × 200 × 20 plate lying at height `z`.
fn plate(b: &mut Workbench, z: f64) -> ObjectId {
    let element = Element {
        id: ObjectId(p(b).next_id),
        name: "Пластина".into(),
        material: ObjectId(1),
        shape: ElementShape::Plate {
            contour: vec![[0., 0.], [400., 0.], [400., 200.], [0., 200.]],
            thickness: LengthMm::new(20.).unwrap(),
            frame: Frame {
                origin: Point3([0., 0., z]),
                ..Frame::default()
            },
        },
        properties: Default::default(),
    };
    b.editor
        .commit(
            "Пластина",
            &[open_bim_commands::Command::Create { element }],
        )
        .unwrap();
    last(b)
}

#[test]
fn bolt_group_by_parts_and_points_with_spacing_notation() {
    assert_eq!(spacings("3*80").unwrap(), [80., 80., 80.]);
    assert_eq!(spacings("50 2*70;40").unwrap(), [50., 70., 70., 40.]);
    assert!(spacings("0*80").is_err() && spacings("-5").is_err());
    let mut b = bench();
    let (lower, upper) = (plate(&mut b, 0.), plate(&mut b, 20.));
    b.tools.detailing.steps_x = "3*80".into();
    b.tools.detailing.steps_y = String::new();
    b.execute(Cmd::ToolBolt);
    assert!(b.prompt().contains("основную деталь"));
    assert!(b.pick_object(Some(lower)).is_none());
    assert!(b.pick_object(Some(upper)).is_none());
    assert!(b.execute(Cmd::FinishPicks).is_none());
    assert!(b.prompt().contains("начало"));
    let start = DVec3::new(80., 100., 40.);
    assert!(b.pick(start).is_none());
    // After the first point the whole group is known for the preview.
    let pattern = bolt_pattern(
        p(&b),
        &b.detail.parts,
        start,
        DVec3::new(300., 100., 40.),
        DVec3::Z,
        &b.tools.detailing,
    )
    .unwrap();
    let axes = bolt_axes(&pattern);
    assert_eq!(axes.len(), 4);
    let r = b.pick(DVec3::new(300., 100., 40.));
    assert!(apply(&mut b, r));
    let group = p(&b)
        .details
        .values()
        .find_map(|d| match &d.kind {
            DetailKind::BoltGroup {
                frame,
                steps_x,
                parts,
                holes,
                ..
            } => Some((*frame, steps_x.clone(), parts.clone(), holes.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(group.1, [80., 80., 80.]);
    assert_eq!(group.2, BTreeSet::from([lower, upper]));
    assert_eq!(group.3.len(), 8, "a hole per bolt in both plates");
    // Bolt centres exactly at x = 80, 160, 240, 320 on y = 100, through both plates.
    for (k, (s, e)) in axes.iter().enumerate() {
        let x = 80. + 80. * k as f64;
        assert!(s.distance(DVec3::new(x, 100., 0.)) < 1e-9, "{s}");
        assert!(e.distance(DVec3::new(x, 100., 40.)) < 1e-9, "{e}");
        let world = group.0.world(Point3([80. * k as f64, 0., 0.]));
        assert!(DVec3::from_array(world.0).distance(*s) < 1e-9);
    }
}

#[test]
fn shift_and_rotate_move_and_turn_the_group() {
    let mut b = bench();
    let parts = BTreeSet::from([plate(&mut b, 0.)]);
    b.tools.detailing.steps_x = "2*100".into();
    b.tools.detailing.steps_y = "60".into();
    b.tools.detailing.shift_x = "10".into();
    b.tools.detailing.shift_y = "20".into();
    let pattern = bolt_pattern(
        p(&b),
        &parts,
        DVec3::new(50., 50., 0.),
        DVec3::new(150., 50., 0.),
        DVec3::Z,
        &b.tools.detailing,
    )
    .unwrap();
    assert_eq!(pattern.frame.origin.0, [60., 70., 0.]);
    b.tools.detailing.rotate = true;
    let turned = bolt_pattern(
        p(&b),
        &parts,
        DVec3::new(50., 50., 0.),
        DVec3::new(150., 50., 0.),
        DVec3::Z,
        &b.tools.detailing,
    )
    .unwrap();
    assert_eq!(
        (turned.steps_x.clone(), turned.steps_y.clone()),
        (vec![60.], vec![100., 100.])
    );
}

#[test]
fn weld_with_type_choice_joins_an_assembly_lists_and_draws() {
    let mut b = bench();
    let column = member(&mut b, Cmd::ToolColumn, &[DVec3::new(0., 0., 0.)]);
    let beam = member(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)],
    );
    let d = &mut b.tools.detailing;
    d.weld_type = WeldType::Fillet;
    d.weld_sides = WeldSides::Both;
    d.weld_size = "6".into();
    d.shop = true;
    b.execute(Cmd::ToolWeld);
    assert!(b.prompt().contains("к которой приваривают"));
    assert!(b.pick_object(Some(column)).is_none());
    let r = b.pick_object(Some(beam));
    assert!(apply(&mut b, r));
    let weld = p(&b)
        .details
        .values()
        .find(|d| matches!(d.kind, DetailKind::Weld { .. }))
        .unwrap()
        .clone();
    let DetailKind::Weld {
        sides, weld_type, ..
    } = &weld.kind
    else {
        unreachable!()
    };
    assert_eq!((*weld_type, *sides), (WeldType::Fillet, WeldSides::Both));
    // One assembly with both parts, the column as main part.
    let assemblies: Vec<_> = p(&b).assemblies.values().collect();
    assert_eq!(assemblies.len(), 1);
    assert_eq!(assemblies[0].members, BTreeSet::from([column, beam]));
    assert_eq!(assemblies[0].main_part, column);
    // Weld list.
    let reports = open_bim_reports::generate(p(&b)).unwrap();
    assert!(
        reports
            .fasteners
            .iter()
            .any(|r| r.description == "Шов угловой ГОСТ 5264-80 Т3 k=6 заводской"),
        "{:?}",
        reports.fasteners
    );
    // ГОСТ 2.312-style sign on the assembly drawing.
    let id = assemblies[0].id;
    let drawing = open_bim_drawings::assembly_drawing(p(&b), ObjectId(9999), id, "КМД-1").unwrap();
    let sheet = open_bim_drawings::generate(p(&b), &drawing).unwrap();
    assert!(
        sheet
            .views
            .iter()
            .flat_map(|v| &v.labels)
            .any(|l| l.text == "ГОСТ 5264-80-Т3-"),
        "weld sign on the drawing"
    );
    // The leg sign is drawn as lines, the leg follows it.
    assert!(
        sheet
            .views
            .iter()
            .flat_map(|v| &v.labels)
            .any(|l| l.text == "6" && l.height == open_bim_drawings::TEXT_HEIGHT),
        "leg after the sign"
    );
    // A site weld does not join assemblies.
    let mut b2 = bench();
    let c2 = member(&mut b2, Cmd::ToolColumn, &[DVec3::new(0., 0., 0.)]);
    let b2_beam = member(
        &mut b2,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)],
    );
    b2.tools.detailing.shop = false;
    b2.execute(Cmd::ToolWeld);
    b2.pick_object(Some(c2));
    let r = b2.pick_object(Some(b2_beam));
    assert!(apply(&mut b2, r));
    assert!(p(&b2).assemblies.is_empty());
}

#[test]
fn assembly_commands_are_single_undoable_steps() {
    let mut b = bench();
    let column = member(&mut b, Cmd::ToolColumn, &[DVec3::new(0., 0., 0.)]);
    let beam = member(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)],
    );
    let brace = member(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 0.), DVec3::new(6000., 0., 3600.)],
    );
    let depth = |b: &Workbench| b.editor.document.history_depth().0;
    // Create from the selection: the heaviest member is main.
    b.editor.selected = BTreeSet::from([column, beam]);
    let before = depth(&b);
    let r = b.execute(Cmd::AssemblyCreate);
    assert!(apply(&mut b, r));
    assert_eq!(depth(&b), before + 1);
    let a = p(&b).assemblies.values().next().unwrap().clone();
    assert_eq!(a.members, BTreeSet::from([column, beam]));
    assert_eq!(
        a.main_part, beam,
        "the 6000 mm beam outweighs the 3600 mm column"
    );
    // Add: select the brace, start the command, click a part of the assembly.
    b.editor.selected = BTreeSet::from([brace]);
    b.execute(Cmd::AssemblyAdd);
    assert!(b.picks_objects());
    let r = b.pick_object(Some(column));
    assert!(apply(&mut b, r));
    assert_eq!(
        p(&b).assemblies[&a.id].members,
        BTreeSet::from([column, beam, brace])
    );
    b.execute(Cmd::CancelTool);
    // Main part.
    b.editor.selected = BTreeSet::from([column]);
    let r = b.execute(Cmd::AssemblySetMain);
    assert!(apply(&mut b, r));
    assert_eq!(p(&b).assemblies[&a.id].main_part, column);
    // Remove the brace.
    b.editor.selected = BTreeSet::from([brace]);
    let r = b.execute(Cmd::AssemblyRemove);
    assert!(apply(&mut b, r));
    assert_eq!(
        p(&b).assemblies[&a.id].members,
        BTreeSet::from([column, beam])
    );
    // Explode: the assembly goes, the parts stay.
    b.editor.selected = BTreeSet::from([beam]);
    let r = b.execute(Cmd::AssemblyExplode);
    assert!(apply(&mut b, r));
    assert!(p(&b).assemblies.is_empty());
    assert!(
        [column, beam, brace]
            .iter()
            .all(|id| p(&b).elements.contains_key(id))
    );
    // Each step undoes alone.
    b.editor.history(false).unwrap();
    assert_eq!(p(&b).assemblies.len(), 1);
}

fn plate_points(b: &Workbench, id: ObjectId) -> Vec<DVec3> {
    let e = &p(b).elements[&id];
    let ElementShape::Plate {
        contour, thickness, ..
    } = &e.shape
    else {
        panic!("not a plate")
    };
    let f = e.shape.frame();
    contour
        .iter()
        .flat_map(|c| {
            [0., thickness.value()].map(|z| DVec3::from_array(f.world(Point3([c[0], c[1], z])).0))
        })
        .collect()
}

fn role(b: &Workbench, id: ObjectId) -> String {
    match p(b).elements[&id].properties.get("fabrication_role") {
        Some(PropertyValue::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

#[test]
fn gusset_touches_both_members_and_stiffener_fits_between_flanges() {
    let mut b = bench();
    let column = member(&mut b, Cmd::ToolColumn, &[DVec3::new(0., 0., 0.)]);
    let beam = member(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3000.), DVec3::new(6000., 0., 3000.)],
    );
    b.editor.draft.thickness = "10".into();
    b.tools.detailing.gusset_leg = "300".into();
    b.tools.detailing.gusset_chamfer = "30".into();
    b.editor.selected.clear();
    b.execute(Cmd::ToolGusset);
    assert!(b.prompt().contains("первую деталь угла"));
    b.pick_object(Some(column));
    let r = b.pick_object(Some(beam));
    assert!(apply(&mut b, r), "{:?}", b.tools.message);
    let gusset = last(&b);
    assert_eq!(role(&b, gusset), "gusset");
    // Faces: the column's flange edge towards the beam, the beam's bottom.
    let column_face = {
        let e = &p(&b).elements[&column];
        let f = e.shape.frame();
        let section = p(&b).profiles[&match &e.shape {
            ElementShape::Member { profile, .. } => *profile,
            _ => unreachable!(),
        }]
            .shape
            .solid_contour()
            .unwrap();
        section
            .iter()
            .map(|q| f.axes[0][0] * q[0] + f.axes[1][0] * q[1])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let points = plate_points(&b, gusset);
    let tol = 1e-6;
    assert!(
        points
            .iter()
            .all(|q| q.x >= column_face - tol && q.z <= 2850. + tol)
    );
    assert!(
        points.iter().any(|q| (q.x - column_face).abs() < tol),
        "on the column face"
    );
    assert!(
        points.iter().any(|q| (q.z - 2850.).abs() < tol),
        "on the beam bottom"
    );
    assert!(
        points.iter().all(|q| q.y.abs() <= 5. + tol),
        "centred on the webs"
    );
    // Stiffener in the beam at x = 3000 on the +y side of the web.
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolStiffener);
    b.pick_object(Some(beam));
    let r = b.pick(DVec3::new(3000., 40., 3000.));
    assert!(apply(&mut b, r));
    let rib = last(&b);
    assert_eq!(role(&b, rib), "stiffener");
    let rib_points = plate_points(&b, rib);
    let (zmin, zmax) = rib_points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), q| {
            (a.min(q.z), c.max(q.z))
        });
    // I 300×150×8×12: between the flanges' inner faces, z = 3000 ± 138.
    assert!(
        (zmin - 2862.).abs() < tol && (zmax - 3138.).abs() < tol,
        "{zmin} {zmax}"
    );
    let (ymin, ymax) = rib_points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), q| {
            (a.min(q.y), c.max(q.y))
        });
    assert!(
        (ymin.abs() - 4.).abs() < tol && (ymax.abs() - 75.).abs() < tol
            || (ymax.abs() - 4.).abs() < tol && (ymin.abs() - 75.).abs() < tol,
        "from the web to the flange edge: {ymin} {ymax}"
    );
    let (xmin, xmax) = rib_points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), q| {
            (a.min(q.x), c.max(q.x))
        });
    assert!(
        (xmin - 2995.).abs() < tol && (xmax - 3005.).abs() < tol,
        "10 mm thick at x = 3000"
    );
    // The pane names them.
    b.editor.selected = BTreeSet::from([rib]);
    let c = open_bim_desktop::ui::property_pane::common(p(&b), &b.editor.selected).unwrap();
    assert_eq!(c.kind.value().unwrap(), "Ребро жёсткости");
}

#[test]
fn help_explains_the_workflows() {
    let all = STEEL_WORKFLOWS.join("\n");
    for needle in [
        "Enter",
        "3*80",
        "заводской шов объединяет",
        "косынка — две детали угла",
        "ребро жёсткости",
    ] {
        assert!(all.contains(needle), "{needle}");
    }
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/help/bolts-welds.md"
    ))
    .unwrap();
    for needle in [
        "Болтовая группа",
        "Сварной шов",
        "Сборки",
        "Пластина, косынка и ребро",
    ] {
        assert!(doc.contains(needle), "{needle}");
    }
}
