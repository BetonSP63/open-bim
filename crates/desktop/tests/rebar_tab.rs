//! «Армирование»: bars generated in concrete parts and placed by picks,
//! the bar schedule and the visibility toggle, each set one undo step.
use open_bim_desktop::interaction::rebar::{self, RebarSettings};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{ObjectId, RebarKind};
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

fn inside(b: [[f64; 3]; 2], p: [f64; 3]) -> bool {
    (0..3).all(|k| p[k] > b[0][k] - 1e-6 && p[k] < b[1][k] + 1e-6)
}

#[test]
fn bars_in_concrete_parts() {
    let mut b = bench();
    b.execute(Cmd::ConcreteBeam);
    b.pick(DVec3::new(0., 0., 3000.));
    let r = b.pick(DVec3::new(6000., 0., 3000.));
    let beam = commit(&mut b, r)[0];
    // Longitudinal: 2 per face → 4 corner bars inside the cover.
    b.execute(Cmd::RebarLongitudinal);
    let r = b.pick_object(Some(beam));
    commit(&mut b, r);
    assert!(b.rebar.show);
    let p = b.editor.document.project();
    let set = &p.concrete.rebars[0];
    assert_eq!(
        (set.number, set.kind, set.bars.len()),
        (1, RebarKind::Longitudinal, 4)
    );
    let bounds = p.element_bounds(beam).unwrap();
    for bar in &set.bars {
        for q in bar {
            assert!(inside(bounds, q.0));
        }
        assert!((open_bim_model::RebarSet::bar_length(bar) - 5940.).abs() < 1e-6);
    }
    // 300x600 beam, cover 30, Ø16: bars at x = ±112, 8 mm from the y faces.
    let xs: Vec<f64> = set.bars.iter().map(|bar| bar[0].0[1]).collect();
    assert!(xs.iter().all(|x| (x.abs() - 112.).abs() < 1e-6), "{xs:?}");
    // Stirrups from the selection: Ø8 at 150.
    b.rebar.settings.diameter = 8.;
    b.rebar.settings.spacing = 150.;
    b.editor.selected = [beam].into();
    let r = b.execute(Cmd::RebarStirrup);
    commit(&mut b, r);
    let set = &b.editor.document.project().concrete.rebars[1];
    assert_eq!(set.kind, RebarKind::Stirrup);
    // 30..5970 at 150 → 40 steps + the end.
    assert_eq!(set.bars.len(), 41);
    assert!(set.bars.iter().all(|s| s.len() == 5 && s[0] == s[4]));
    // A slab gets two two-way meshes.
    b.editor.selected.clear();
    b.execute(Cmd::ConcreteSlab);
    for p in [
        [0., 0., 3000.],
        [6000., 0., 3000.],
        [6000., 3000., 3000.],
        [0., 3000., 3000.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let r = b.execute(Cmd::FinishPicks);
    let slab = commit(&mut b, r)[0];
    b.rebar.settings.diameter = 12.;
    b.rebar.settings.spacing = 200.;
    b.execute(Cmd::RebarFace);
    let r = b.pick_object(Some(slab));
    commit(&mut b, r);
    let p = b.editor.document.project();
    let mesh = &p.concrete.rebars[2];
    let bounds = p.element_bounds(slab).unwrap();
    assert!(mesh.bars.iter().flatten().all(|q| inside(bounds, q.0)));
    // Along x: y from 36 to 2964 at 200 → 15 + end; along y: x 36..5964 → 30 + end.
    assert_eq!(mesh.bars.len(), 2 * (16 + 31));
    // A panel gets meshes at its two wide faces.
    b.execute(Cmd::ConcretePanel);
    b.pick(DVec3::new(0., 5000., 0.));
    let r = b.pick(DVec3::new(3000., 5000., 0.));
    let panel = commit(&mut b, r)[0];
    b.execute(Cmd::RebarFace);
    let r = b.pick_object(Some(panel));
    commit(&mut b, r);
    let p = b.editor.document.project();
    let bounds = p.element_bounds(panel).unwrap();
    let mesh = p.concrete.rebars.last().unwrap();
    assert!(mesh.bars.iter().flatten().all(|q| inside(bounds, q.0)));
    let ys: std::collections::BTreeSet<i64> = mesh
        .bars
        .iter()
        .flatten()
        .map(|q| q.0[1].round() as i64)
        .collect();
    // 200 thick: layers 36 and 48 mm from each face.
    assert_eq!(ys.into_iter().collect::<Vec<_>>(), [4936, 4948, 5052, 5064]);
    // Steel parts are refused.
    let r = rebar::longitudinal(
        &open_bim_model::example_steel_frame().unwrap(),
        ObjectId(100),
        &RebarSettings::default(),
    );
    assert!(r.is_err());
}

#[test]
fn bars_by_picks_schedule_and_undo() {
    let mut b = bench();
    b.execute(Cmd::RebarSingle);
    b.pick(DVec3::new(0., 0., 0.));
    let r = b.pick(DVec3::new(1000., 0., 0.));
    commit(&mut b, r);
    b.execute(Cmd::RebarGroup);
    b.pick(DVec3::new(0., 0., 0.));
    b.pick(DVec3::new(2000., 0., 0.));
    let r = b.pick(DVec3::new(500., 1000., 0.));
    commit(&mut b, r);
    b.execute(Cmd::RebarGuide);
    for p in [[0., 0., 0.], [0., 0., 500.], [1000., 0., 500.]] {
        assert!(b.pick(DVec3::from_array(p)).is_none());
    }
    let r = b.execute(Cmd::FinishPicks);
    commit(&mut b, r);
    let p = b.editor.document.project();
    let sets = &p.concrete.rebars;
    assert_eq!(sets.len(), 3);
    // Group: 0..1000 at 200 → 6 bars of 2000.
    assert_eq!(sets[1].bars.len(), 6);
    assert_eq!(sets[2].bars[0].len(), 3);
    // Schedule: Ø16 А500С, 1 + 6 + 1 bars, 1 + 12 + 1.5 m.
    let rows = rebar::schedule(p);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bars, 8);
    assert!((rows[0].length_m - 14.5).abs() < 1e-9);
    let mass = std::f64::consts::PI * 0.016 * 0.016 / 4. * 14.5 * 7850.;
    assert!((rows[0].mass_kg - mass).abs() < 1e-9);
    // Undo removes the last set; visibility toggles.
    b.editor.history(false).unwrap();
    assert_eq!(b.editor.document.project().concrete.rebars.len(), 2);
    let shown = b.rebar.show;
    b.execute(Cmd::RebarVisibility);
    assert_ne!(shown, b.rebar.show);
    b.execute(Cmd::RebarMore);
    assert!(b.rebar.open);
    // Saved and read back.
    let json = serde_json::to_string(b.editor.document.project()).unwrap();
    let back: open_bim_model::Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.concrete, b.editor.document.project().concrete);
}
