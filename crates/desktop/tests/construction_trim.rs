//! Construction lines/circles, model dimensions and the fitting
//! and cutting commands, checked against exact geometry.
use open_bim_commands::Command;
use open_bim_desktop::interaction::member_edit::ends;
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::interaction::trim::{
    TrimTool, combine, construction, cut_line, cut_part, cut_polygon, dimension, extend_to,
    fit_end, split,
};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ConstructionKind, ObjectId, Project};
use open_bim_renderer::DVec3;

/// The analytic test sections these geometry checks are written for
/// (profile 2 is I 300×150×8×12), with the default grid; new models in the
/// app start with ГОСТ sections instead.
fn default_project() -> Result<open_bim_model::Project, String> {
    with_default_grid(open_bim_model::Project::default())
}

const Z: DVec3 = DVec3::Z;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            if let Err(e) = b.editor.commit(&label, &commands) {
                panic!("{label}: {e} {commands:?}");
            }
            true
        }
        _ => false,
    }
}

fn beam(b: &mut Workbench, from: DVec3, to: DVec3) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.pick(from);
    let r = b.pick(to);
    assert!(apply(b, r));
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn commit(b: &mut Workbench, commands: Vec<Command>) {
    b.editor.commit("Тест", &commands).unwrap();
}

fn p(b: &Workbench) -> &Project {
    b.editor.document.project()
}

fn span(b: &Workbench, id: ObjectId) -> (DVec3, DVec3) {
    ends(&p(b).elements[&id].shape).unwrap()
}

fn close(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-6
}

#[test]
fn construction_line_is_a_snap_target_and_survives_save() {
    let mut b = bench();
    let (a, c) = (DVec3::new(1000., 1000., 0.), DVec3::new(5000., 3000., 0.));
    let cmds = construction(p(&b), TrimTool::ConstructionLine, a, c, Z).unwrap();
    commit(&mut b, cmds);
    let circle = construction(
        p(&b),
        TrimTool::ConstructionCircle,
        DVec3::new(9000., 3000., 0.),
        DVec3::new(10500., 3000., 0.),
        Z,
    )
    .unwrap();
    commit(&mut b, circle);
    let project = p(&b).clone();
    assert_eq!(project.construction.len(), 2);
    let radius = project.construction.values().find_map(|c| match c.kind {
        ConstructionKind::Circle { radius, .. } => Some(radius),
        _ => None,
    });
    assert_eq!(radius, Some(1500.));
    // Ends and midpoint are snap points.
    let snaps = open_bim_desktop::interaction::snap::construction_points(&project);
    assert!(snaps.iter().any(|q| close(*q, a)));
    assert!(snaps.iter().any(|q| close(*q, (a + c) * 0.5)));
    // A line across grid lines «А» and «Б» and the first line: their
    // crossings are snap points too.
    let cross = construction(
        &project,
        TrimTool::ConstructionLine,
        DVec3::new(3000., -1000., 0.),
        DVec3::new(3000., 7000., 0.),
        Z,
    )
    .unwrap();
    commit(&mut b, cross);
    let snaps = open_bim_desktop::interaction::snap::construction_points(p(&b));
    for node in [
        DVec3::new(3000., 0., 0.),
        DVec3::new(3000., 6000., 0.),
        DVec3::new(3000., 2000., 0.),
    ] {
        assert!(snaps.iter().any(|q| close(*q, node)), "intersection {node}");
    }
    // Saved and loaded unchanged; drawings never list them.
    let bytes = open_bim_persistence::encode(p(&b)).unwrap();
    let back = open_bim_persistence::decode(&bytes).unwrap();
    assert_eq!(back.construction, p(&b).construction);
}

#[test]
fn model_dimension_follows_the_beam_end() {
    let mut b = bench();
    let id = beam(
        &mut b,
        DVec3::new(0., 0., 3600.),
        DVec3::new(6000., 0., 3600.),
    );
    let cmds = dimension(
        p(&b),
        DVec3::new(0., 0., 3600.),
        DVec3::new(6000., 0., 3600.),
    )
    .unwrap();
    commit(&mut b, cmds);
    let d = p(&b).dimensions.values().next().unwrap().clone();
    assert_eq!(p(&b).dimension_value(&d), Some(6000.));
    let cmds = extend_to(p(&b), id, DVec3::new(7000., 500., 3600.)).unwrap();
    commit(&mut b, cmds);
    assert!((p(&b).dimension_value(&d).unwrap() - 7000.).abs() < 1e-9);
    // Deleting the beam removes its dimension.
    commit(
        &mut b,
        vec![Command::Delete {
            id,
            policy: open_bim_commands::DeletePolicy::Cascade,
        }],
    );
    assert!(p(&b).dimensions.is_empty());
}

#[test]
fn fit_split_combine_and_extend_are_exact() {
    let mut b = bench();
    let id = beam(
        &mut b,
        DVec3::new(0., 0., 3600.),
        DVec3::new(6000., 0., 3600.),
    );
    // Fit by a line across the beam at x = 4500 (work plane XY).
    let cmds = fit_end(
        p(&b),
        id,
        DVec3::new(4500., -1000., 3600.),
        DVec3::new(4500., 1000., 3600.),
        Z,
    )
    .unwrap();
    commit(&mut b, cmds);
    let (s, e) = span(&b, id);
    assert!(close(s, DVec3::new(0., 0., 3600.)) && close(e, DVec3::new(4500., 0., 3600.)));
    // A slanted fitting line: the end lands where the axis meets it.
    let cmds = fit_end(
        p(&b),
        id,
        DVec3::new(5000., -1000., 3600.),
        DVec3::new(6000., 1000., 3600.),
        Z,
    )
    .unwrap();
    commit(&mut b, cmds);
    assert!(close(span(&b, id).1, DVec3::new(5500., 0., 3600.)));
    // Split at 2500: two members, lengths add up.
    let cmds = split(p(&b), id, DVec3::new(2500., 300., 3600.)).unwrap();
    commit(&mut b, cmds);
    let other = *p(&b).elements.keys().last().unwrap();
    assert_ne!(other, id);
    let (a0, a1) = span(&b, id);
    let (b0, b1) = span(&b, other);
    assert!(close(a1, DVec3::new(2500., 0., 3600.)) && close(b0, a1));
    assert!((a0.distance(a1) + b0.distance(b1) - 5500.).abs() < 1e-9);
    // Combine restores one member.
    let cmds = combine(p(&b), id, other).unwrap();
    commit(&mut b, cmds);
    assert!(!p(&b).elements.contains_key(&other));
    let (s, e) = span(&b, id);
    assert!(close(s, DVec3::new(0., 0., 3600.)) && close(e, DVec3::new(5500., 0., 3600.)));
    // Extend the start back to x = -500.
    let cmds = extend_to(p(&b), id, DVec3::new(-500., 0., 0.)).unwrap();
    commit(&mut b, cmds);
    assert!(close(span(&b, id).0, DVec3::new(-500., 0., 3600.)));
    // Errors: a fitting line parallel to the axis, combining crossing members.
    assert!(
        fit_end(
            p(&b),
            id,
            DVec3::new(0., 100., 3600.),
            DVec3::new(1000., 100., 3600.),
            Z
        )
        .is_err()
    );
    let cross = beam(
        &mut b,
        DVec3::new(3000., -2000., 3600.),
        DVec3::new(3000., 2000., 3600.),
    );
    assert!(combine(p(&b), id, cross).is_err());
}

fn volume(b: &Workbench, id: ObjectId) -> f64 {
    p(b).element_volume_mm3(id).unwrap()
}

#[test]
fn cuts_remove_exactly_the_expected_volume() {
    let mut b = bench();
    let id = beam(
        &mut b,
        DVec3::new(0., 0., 3600.),
        DVec3::new(6000., 0., 3600.),
    );
    let full = volume(&b, id);
    let area = full / 6000.;
    // Line cut across the beam at x = 4000, removing the x > 4000 side.
    let cmds = cut_line(
        p(&b),
        id,
        DVec3::new(4000., -1000., 3600.),
        DVec3::new(4000., 1000., 3600.),
        DVec3::new(5000., 0., 3600.),
        Z,
    )
    .unwrap();
    commit(&mut b, cmds);
    assert!(
        (volume(&b, id) - area * 4000.).abs() / full < 1e-6,
        "line cut"
    );
    b.editor.history(false).unwrap();
    // Cut by a column crossing the beam: the removed volume equals the
    // overlap of the two solids (column section × beam section height).
    let column = {
        b.execute(Cmd::ToolColumn);
        let r = b.pick(DVec3::new(3000., 0., 0.));
        assert!(apply(&mut b, r));
        b.execute(Cmd::CancelTool);
        *p(&b).elements.keys().last().unwrap()
    };
    let column_before = volume(&b, column);
    let cmds = cut_part(p(&b), id, column).unwrap();
    commit(&mut b, cmds);
    let removed = full - volume(&b, id);
    // Independent overlap: Boolean intersection of the untouched solids.
    let mut plain = p(&b).clone();
    plain.details.clear();
    let world_plain = |pid: ObjectId| {
        let e = &plain.elements[&pid];
        let f = e.shape.frame();
        plain
            .element_body(pid)
            .unwrap()
            .rigid_transform(f.axes, f.origin.0)
            .unwrap()
    };
    let overlap = world_plain(id)
        .intersection(&world_plain(column))
        .unwrap()
        .volume_mm3()
        .unwrap();
    assert!(overlap > 1000., "the column crosses the beam");
    assert!(
        (removed - overlap).abs() / overlap < 1e-3,
        "removed {removed} vs overlap {overlap}"
    );
    assert_eq!(volume(&b, column), column_before, "the cutting part stays");
    // Polygon cut: a 200 × 200 square in plan through the beam.
    b.editor.history(false).unwrap();
    let square = [
        DVec3::new(1000., -100., 0.),
        DVec3::new(1200., -100., 0.),
        DVec3::new(1200., 100., 0.),
        DVec3::new(1000., 100., 0.),
    ];
    let cmds = cut_polygon(p(&b), id, &square, Z).unwrap();
    commit(&mut b, cmds);
    assert!(
        (full - volume(&b, id) - area * 200.).abs() / full < 1e-6,
        "polygon cut"
    );
}

#[test]
fn ribbon_commands_run_by_picks_and_ifc_keeps_the_cut_volume() {
    let mut b = bench();
    let id = beam(
        &mut b,
        DVec3::new(0., 0., 3600.),
        DVec3::new(6000., 0., 3600.),
    );
    // «Подогнать торец» with the beam selected: two picks and it is done.
    b.editor.selected = [id].into();
    b.execute(Cmd::FitEnd);
    assert!(b.prompt().contains("первую точку линии подгонки"));
    assert!(b.pick(DVec3::new(5000., -500., 3600.)).is_none());
    let r = b.pick(DVec3::new(5000., 500., 3600.));
    assert!(apply(&mut b, r));
    assert!(close(span(&b, id).1, DVec3::new(5000., 0., 3600.)));
    b.execute(Cmd::CancelTool);
    // «Разрезать деталью»: target, then tool, by clicking parts.
    b.execute(Cmd::ToolColumn);
    let r = b.pick(DVec3::new(2500., 0., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let column = *p(&b).elements.keys().last().unwrap();
    b.editor.selected.clear();
    b.execute(Cmd::CutPart);
    assert!(b.picks_objects());
    assert!(b.pick_object(Some(id)).is_none());
    let r = b.pick_object(Some(column));
    assert!(apply(&mut b, r));
    // «Разрезать линией»: part, two points, the side to remove.
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::CutLine);
    assert!(b.pick_object(Some(id)).is_none());
    b.pick(DVec3::new(4000., -500., 3600.));
    b.pick(DVec3::new(4000., 500., 3600.));
    let r = b.pick(DVec3::new(4500., 0., 3600.));
    assert!(apply(&mut b, r));
    // «Разрезать многоугольником» closes with Enter.
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::CutPolygon);
    b.pick_object(Some(id));
    for q in [(500., -100.), (700., -100.), (700., 100.), (500., 100.)] {
        assert!(b.pick(DVec3::new(q.0, q.1, 3600.)).is_none());
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    assert_eq!(
        p(&b)
            .details
            .values()
            .filter(|d| d.kind.part() == Some(id))
            .count(),
        3
    );
    // IFC: export, import, same volume of the cut beam (within 0.5 %).
    let cut = p(&b).element_volume_mm3(id).unwrap();
    let (text, _) = open_bim_ifc::export(p(&b)).unwrap();
    let (back, report) = open_bim_ifc::import(&text).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let imported = back
        .elements
        .values()
        .filter(|e| {
            matches!(
                e.shape,
                open_bim_model::ElementShape::Member {
                    member_kind: open_bim_model::MemberKind::Beam,
                    ..
                }
            )
        })
        .map(|e| back.element_volume_mm3(e.id).unwrap())
        .next()
        .unwrap();
    assert!(
        (imported - cut).abs() / cut < 0.005,
        "IFC {imported} vs model {cut}"
    );
}

#[test]
fn points_divide_a_segment_and_are_snap_points() {
    let mut b = bench();
    // «Точка»: one point per click, the command stays armed.
    b.execute(Cmd::ConstructionPoint);
    let one = DVec3::new(1500., 2500., 0.);
    let r = b.pick(one);
    assert!(apply(&mut b, r));
    let r = b.pick(DVec3::new(2500., 2500., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    assert_eq!(p(&b).construction.len(), 2);
    // «Точки на линии»: ends and division points.
    b.tools.detailing.point_divisions = "4".into();
    b.execute(Cmd::PointsOnLine);
    b.pick(DVec3::new(0., 1000., 0.));
    let r = b.pick(DVec3::new(6000., 1000., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let points: Vec<DVec3> = p(&b)
        .construction
        .values()
        .filter_map(|c| match c.kind {
            ConstructionKind::Point { at } => Some(DVec3::from_array(at.0)),
            _ => None,
        })
        .collect();
    assert_eq!(points.len(), 7);
    for x in [0., 1500., 3000., 4500., 6000.] {
        assert!(points.iter().any(|q| close(*q, DVec3::new(x, 1000., 0.))));
    }
    // Points are snap points and are not lines.
    let snaps = open_bim_desktop::interaction::snap::construction_points(p(&b));
    assert!(snaps.iter().any(|q| close(*q, one)));
    assert!(open_bim_desktop::interaction::snap::construction_segments(p(&b)).is_empty());
    // Bad input is refused, nothing is created.
    let n = p(&b).construction.len();
    b.tools.detailing.point_divisions = "0".into();
    b.execute(Cmd::PointsOnLine);
    b.pick(DVec3::ZERO);
    let r = b.pick(DVec3::new(1000., 0., 0.));
    assert!(!apply(&mut b, r));
    assert_eq!(p(&b).construction.len(), n);
    // Saved and loaded unchanged.
    let bytes = open_bim_persistence::encode(p(&b)).unwrap();
    let back = open_bim_persistence::decode(&bytes).unwrap();
    assert_eq!(back.construction, p(&b).construction);
}

#[test]
fn edge_chamfer_removes_the_triangle_along_the_whole_edge() {
    use open_bim_desktop::interaction::trim::edge_chamfer;
    let mut b = bench();
    // A 300 × 200 × 20 plate.
    let mut project = p(&b).clone();
    let id = ObjectId(project.next_id);
    let material = *project.materials.keys().next().unwrap();
    project.elements.insert(
        id,
        open_bim_model::Element {
            id,
            name: "Пластина".into(),
            material,
            shape: open_bim_model::ElementShape::Plate {
                contour: vec![[0., 0.], [300., 0.], [300., 200.], [0., 200.]],
                thickness: open_bim_model::LengthMm::new(20.).unwrap(),
                frame: Default::default(),
            },
            properties: Default::default(),
        },
    );
    project.next_id += 1;
    b = Workbench::new(Editor::new(project).unwrap());
    let full = volume(&b, id);
    assert!((full - 300. * 200. * 20.).abs() < 1e-3);
    // Corner edge through the thickness at (300, 0): 30 × 30 triangle.
    let cmds = edge_chamfer(
        p(&b),
        id,
        DVec3::new(300., 0., 0.),
        DVec3::new(300., 0., 20.),
        30.,
        30.,
    )
    .unwrap();
    commit(&mut b, cmds);
    let removed = full - volume(&b, id);
    assert!((removed - 0.5 * 30. * 30. * 20.).abs() < 1e-3, "{removed}");
    // Top edge along y = 200 (end face z = 20): 10 on the face × 5 deep.
    let cmds = edge_chamfer(
        p(&b),
        id,
        DVec3::new(0., 200., 20.),
        DVec3::new(300., 200., 20.),
        10.,
        5.,
    )
    .unwrap();
    commit(&mut b, cmds);
    let removed = full - volume(&b, id);
    let expected = 0.5 * 30. * 30. * 20. + 0.5 * 10. * 5. * 300.;
    assert!((removed - expected).abs() < 1e-3, "{removed} vs {expected}");
    // Points not on one edge, zero legs: refused.
    assert!(
        edge_chamfer(
            p(&b),
            id,
            DVec3::new(0., 0., 0.),
            DVec3::new(300., 200., 20.),
            10.,
            10.
        )
        .is_err()
    );
    assert!(
        edge_chamfer(
            p(&b),
            id,
            DVec3::new(0., 0., 0.),
            DVec3::new(0., 0., 20.),
            0.,
            10.
        )
        .is_err()
    );
}
