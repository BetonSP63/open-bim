use open_bim_commands::project_hash;
use open_bim_desktop::interaction::{Engine, Outcome, Workplane, numeric, prompt};
use open_bim_desktop::tools::{Editor, ToolKind, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ElementShape, Project};
use open_bim_renderer::{Camera, DVec3, Ray};

fn bench() -> Workbench {
    Workbench::new(Editor::new(Project::default()).unwrap())
}

/// Apply a pick request the way the binary does.
fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        _ => false,
    }
}

fn pick(b: &mut Workbench, p: DVec3) -> bool {
    let r = b.pick(p);
    apply(b, r)
}

fn finish(b: &mut Workbench) -> bool {
    let r = b.execute(Cmd::FinishPicks);
    apply(b, r)
}

#[test]
fn beam_needs_two_picks_creates_once_and_rearms() {
    let mut b = bench();
    b.execute(Cmd::ToolBeam);
    assert_eq!(b.engine.prompt(), "Укажите начальную точку балки");
    assert!(!pick(&mut b, DVec3::ZERO));
    assert_eq!(b.engine.prompt(), "Укажите конечную точку балки");
    assert!(pick(&mut b, DVec3::new(6000., 0., 0.)));
    let project = b.editor.document.project();
    assert_eq!(project.elements.len(), 1);
    let beam = project.elements.values().next().unwrap();
    let ElementShape::Member { length, frame, .. } = &beam.shape else {
        panic!()
    };
    assert!((length.value() - 6000.).abs() < 1e-9 && frame.origin.0 == [0.; 3]);
    // Command stays active until Esc: two more beams.
    assert!(b.engine.active() && b.engine.picks.is_empty());
    for x in [6000., 12000.] {
        pick(&mut b, DVec3::new(x, 0., 0.));
        assert!(pick(&mut b, DVec3::new(x + 6000., 0., 0.)));
    }
    assert_eq!(b.editor.document.project().elements.len(), 3);
    assert_eq!(
        b.editor.document.history_depth().0,
        3,
        "one undo step per beam"
    );
}

#[test]
fn column_is_one_pick_with_bottom_and_top_from_properties() {
    let mut b = bench();
    b.execute(Cmd::ToolColumn);
    b.editor.draft.bottom = "-300".into();
    b.editor.draft.top = "3900".into();
    assert_eq!(b.engine.prompt(), "Укажите положение колонны");
    assert!(pick(&mut b, DVec3::new(100., 200., 0.)));
    let c = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap()
        .clone();
    let ElementShape::Member { length, frame, .. } = c.shape else {
        panic!()
    };
    assert!((length.value() - 4200.).abs() < 1e-9);
    assert_eq!(frame.origin.0, [100., 200., -300.]);
    assert_eq!(frame.axes[2], [0., 0., 1.]);
}

#[test]
fn escape_leaves_model_unchanged_enter_finishes_polygon_and_repeats() {
    let mut b = bench();
    let before = project_hash(b.editor.document.project()).unwrap();
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::ZERO);
    b.execute(Cmd::CancelTool);
    assert!(!b.engine.active());
    assert_eq!(project_hash(b.editor.document.project()).unwrap(), before);
    // Enter in idle repeats the last command.
    b.execute(Cmd::FinishPicks);
    assert_eq!(b.engine.tool, Some(ToolKind::Beam));
    b.execute(Cmd::CancelTool);
    // Plate: 4 picks then Enter.
    b.execute(Cmd::ToolPlate);
    let pts = [[0., 0.], [400., 0.], [400., 300.], [0., 300.]];
    for p in pts {
        assert!(!pick(&mut b, DVec3::new(1000. + p[0], 2000. + p[1], 0.)));
    }
    assert!(finish(&mut b));
    let plate = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap()
        .clone();
    let ElementShape::Plate { contour, frame, .. } = plate.shape else {
        panic!()
    };
    assert_eq!(frame.origin.0, [1000., 2000., 0.]);
    assert_eq!(contour, pts.to_vec());
    // Enter with too few points keeps the command alive.
    b.pick(DVec3::ZERO);
    b.pick(DVec3::new(10., 0., 0.));
    assert!(!finish(&mut b));
    assert_eq!(b.engine.picks.len(), 2);
}

#[test]
fn every_tool_has_russian_prompts_per_step() {
    for kind in [
        ToolKind::Axis,
        ToolKind::Beam,
        ToolKind::Column,
        ToolKind::Plate,
        ToolKind::Gusset,
        ToolKind::Stiffener,
    ] {
        for step in 0..4 {
            let p = prompt(kind, step);
            assert!(
                p.starts_with("Укажите") && p.chars().any(|c| ('а'..='я').contains(&c)),
                "{p}"
            );
        }
        if kind != ToolKind::Column {
            assert_ne!(
                prompt(kind, 0),
                prompt(kind, 3),
                "{kind:?} distinguishes steps"
            );
        }
    }
}

#[test]
fn numeric_location_forms_and_errors() {
    let wp = Workplane::default();
    let prev = Some(DVec3::new(100., 0., 0.));
    // The default mode is relative; without a previous pick it starts at the origin.
    assert_eq!(
        numeric::parse("1000,2000,300", &wp, None, None).unwrap(),
        DVec3::new(1000., 2000., 300.)
    );
    assert_eq!(
        numeric::parse("1000, 2000", &wp, prev, None).unwrap(),
        DVec3::new(1100., 2000., 0.)
    );
    assert_eq!(
        numeric::parse("@500,0,0", &wp, prev, None).unwrap(),
        DVec3::new(600., 0., 0.)
    );
    assert_eq!(
        numeric::parse("$0,0,1000", &wp, prev, None).unwrap(),
        DVec3::new(0., 0., 1000.)
    );
    let shifted = Workplane::at_elevation(500.);
    assert_eq!(
        numeric::parse("!6000,12000,0", &shifted, prev, None).unwrap(),
        DVec3::new(6000., 12000., 0.)
    );
    assert_eq!(
        numeric::parse("$6000,12000", &shifted, prev, None).unwrap(),
        DVec3::new(6000., 12000., 500.)
    );
    let polar = numeric::parse("1000<90", &wp, prev, None).unwrap();
    assert!((polar - DVec3::new(100., 1000., 0.)).length() < 1e-9);
    let up = numeric::parse("@1000<0<90", &wp, prev, None).unwrap();
    assert!((up - DVec3::new(100., 0., 1000.)).length() < 1e-9);
    let along = numeric::parse("250", &wp, prev, Some(DVec3::new(0., 3., 4.))).unwrap();
    assert!((along - DVec3::new(100., 150., 200.)).length() < 1e-9);
    assert!(numeric::parse("@1,2", &wp, None, None).is_err());
    assert!(numeric::parse("$5", &wp, prev, None).is_err());
    assert!(numeric::parse("1<2<3<4", &wp, prev, None).is_err());
    assert!(numeric::parse("12a", &wp, None, None).is_err());
    assert!(numeric::parse("1,2,3,4", &wp, None, None).is_err());
    // Relative input follows the work plane axes.
    let tilted =
        Workplane::from_points(DVec3::ZERO, DVec3::new(0., 1., 0.), DVec3::new(0., 0., 1.))
            .unwrap();
    let p = numeric::parse("@100,0,0", &tilted, Some(DVec3::ZERO), None).unwrap();
    assert!((p - DVec3::new(0., 100., 0.)).length() < 1e-12);
    // Invalid typed input keeps the command active and reports an error.
    let mut e = Engine::default();
    e.start(ToolKind::Beam);
    assert!(matches!(e.pick_numeric("abc", None), Outcome::Ignored(_)));
    assert!(e.active() && e.error.is_some());
    assert_eq!(e.pick_numeric("0,0,0", None), Outcome::Continue);
    assert!(matches!(
        e.pick_numeric("@6000,0,0", None),
        Outcome::Complete { .. }
    ));
}

#[test]
fn tilted_work_plane_projects_rays_exactly() {
    let (a, b, c) = (
        DVec3::new(1000., 0., 0.),
        DVec3::new(1000., 5000., 0.),
        DVec3::new(1000., 0., 3000.),
    );
    let wp = Workplane::from_points(a, b, c).unwrap();
    // Plane x = 1000 with local x = +Y, local y = +Z.
    let hit = wp
        .intersect(DVec3::new(0., 2500., 1500.), DVec3::X)
        .unwrap();
    assert!((hit - DVec3::new(1000., 2500., 1500.)).length() < 1e-6);
    let local = wp.to_local(hit);
    assert!((local - DVec3::new(2500., 1500., 0.)).length() < 1e-6);
    assert!((wp.to_world(local) - hit).length() < 1e-6);
    assert!(
        wp.intersect(DVec3::ZERO, DVec3::Y).is_none(),
        "parallel ray"
    );
    assert!(Workplane::from_points(a, a, c).is_err());
}

#[test]
fn wheel_zoom_keeps_the_point_under_the_cursor() {
    for ortho in [false, true] {
        let mut camera = Camera {
            orthographic: ortho,
            ..Camera::default()
        };
        let origin = DVec3::new(500., 200., 0.);
        let aspect = 1.6;
        let uv = [0.27, 0.71];
        let ray: Ray = camera.ray(origin, uv, aspect).unwrap();
        let forward = (camera.target - camera.eye()).normalize();
        let t = (camera.target - ray.origin).dot(forward) / ray.direction.dot(forward);
        let anchor = ray.origin + ray.direction * t;
        camera.zoom_at(300., Some(ray));
        let after = camera.project(origin, anchor, aspect).unwrap();
        assert!(
            (after[0] - uv[0]).abs() < 1e-6 && (after[1] - uv[1]).abs() < 1e-6,
            "{ortho}: {after:?}"
        );
    }
}
