//! «Опалубка»: panels over the formed faces of concrete parts, props under
//! slabs and beams, and the formwork schedule.
use open_bim_desktop::interaction::formwork;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::ObjectId;
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(Editor::new(open_bim_model::Project::default()).unwrap())
}

fn commit(b: &mut Workbench, r: Option<Request>) -> Vec<ObjectId> {
    match r {
        Some(Request::Action(Action::Commit(label, commands))) => {
            let ids = commands
                .iter()
                .filter_map(|c| match c {
                    open_bim_commands::Command::Create { element } => Some(element.id),
                    _ => None,
                })
                .collect();
            b.editor.commit(&label, &commands).unwrap();
            ids
        }
        _ => panic!("no commit: {:?}", b.tools.message),
    }
}

fn area(b: &Workbench, host: ObjectId) -> f64 {
    b.editor
        .document
        .project()
        .concrete
        .forms
        .iter()
        .filter(|f| f.host == host)
        .map(|f| f.area_m2())
        .sum()
}

#[test]
fn panels_props_and_schedule() {
    let mut b = bench();
    b.execute(Cmd::ConcreteColumn);
    let r = b.pick(DVec3::new(0., 0., 0.));
    let col = commit(&mut b, r)[0];
    b.execute(Cmd::ConcreteBeam);
    b.pick(DVec3::new(0., 0., 3000.));
    let r = b.pick(DVec3::new(6000., 0., 3000.));
    let beam = commit(&mut b, r)[0];
    b.execute(Cmd::ConcreteSlab);
    for p in [
        [0., 0., 3600.],
        [6000., 0., 3600.],
        [6000., 4800., 3600.],
        [0., 4800., 3600.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let r = b.execute(Cmd::FinishPicks);
    let slab = commit(&mut b, r)[0];
    // Column 400x400x3000: four sides of 1.2 m².
    b.execute(Cmd::FormworkPanels);
    let r = b.pick_object(Some(col));
    commit(&mut b, r);
    assert!((area(&b, col) - 4.8).abs() < 1e-9);
    assert!(b.formwork.show);
    // Beam 300x600x6000: two sides and the soffit, no top.
    b.editor.selected = [beam].into();
    let r = b.execute(Cmd::FormworkPanels);
    commit(&mut b, r);
    assert!((area(&b, beam) - (2. * 0.6 * 6. + 0.3 * 6.)).abs() < 1e-9);
    // Panels are at most 1200 wide: the 6 m sides make 5 panels each.
    let n = b
        .editor
        .document
        .project()
        .concrete
        .forms
        .iter()
        .filter(|f| f.host == beam)
        .count();
    assert_eq!(n, 15);
    // Slab 6×4.8×0.2: soffit plus edges.
    b.editor.selected = [slab].into();
    let r = b.execute(Cmd::FormworkPanels);
    commit(&mut b, r);
    assert!((area(&b, slab) - (6. * 4.8 + 2. * (6. + 4.8) * 0.2)).abs() < 1e-9);
    // Running again replaces, not doubles.
    let r = b.execute(Cmd::FormworkPanels);
    commit(&mut b, r);
    assert!((area(&b, slab) - (6. * 4.8 + 2. * (6. + 4.8) * 0.2)).abs() < 1e-9);
    // Props under the slab on a 1200 grid: 5 × 4, from the ground to 3400,
    // except those standing on the beam top (3000).
    let r = b.execute(Cmd::FormworkProps);
    commit(&mut b, r);
    let props = &b.editor.document.project().concrete.props;
    assert_eq!(props.len(), 20);
    assert!(props.iter().all(|p| (p.top.0[2] - 3400.).abs() < 1e-6));
    let on_beam = props.iter().filter(|p| p.base.0[2] == 3000.).count();
    assert_eq!(on_beam, 0, "props at y=600 are beside the 300 wide beam");
    // Props under the beam: 6000 / 1200 → 5, standing on the ground.
    b.editor.selected = [beam].into();
    let r = b.execute(Cmd::FormworkProps);
    commit(&mut b, r);
    let p = b.editor.document.project();
    let under: Vec<_> = p.concrete.props.iter().filter(|x| x.host == beam).collect();
    assert_eq!(under.len(), 5);
    assert!(
        under
            .iter()
            .all(|x| x.base.0[2] == 0. && (x.top.0[2] - 2400.).abs() < 1e-6)
    );
    // Schedule rows: panel sizes and prop lengths.
    let rows = formwork::schedule(p);
    let total: f64 = rows.iter().map(|r| r.area_m2).sum();
    let expected = 4.8 + 9. + 6. * 4.8 + 2. * (6. + 4.8) * 0.2;
    assert!((total - expected).abs() < 1e-9);
    assert!(
        rows.iter()
            .any(|r| r.item == "Опора L=3400" && r.count == 20)
    );
    assert!(
        rows.iter()
            .any(|r| r.item == "Опора L=2400" && r.count == 5)
    );
    b.execute(Cmd::FormworkSchedule);
    assert!(b.formwork.open);
    // Removing the beam's formwork and props in one step, then undo.
    let commands = formwork::clear(b.editor.document.project(), &[beam].into()).unwrap();
    b.editor.commit("Удалить", &commands).unwrap();
    assert_eq!(area(&b, beam), 0.);
    b.editor.history(false).unwrap();
    assert!(area(&b, beam) > 0.);
    // Steel parts are refused.
    let mut s =
        Workbench::new(Editor::new(open_bim_model::example_steel_frame().unwrap()).unwrap());
    s.editor.selected.clear();
    let id = *s.editor.document.project().elements.keys().next().unwrap();
    s.execute(Cmd::FormworkPanels);
    assert!(s.pick_object(Some(id)).is_none());
}
