//! Bars by the codes: classes and nominal masses, bend diameters, hooks,
//! anchorage and laps (СП 63.13330.2018), and bar sets that follow their
//! host part when it changes.
use open_bim_commands::{Command, DeletePolicy};
use open_bim_desktop::interaction::rebar::{self, RebarSettings};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::rebar::{self as model, Hook, HookKind, Stress};
use open_bim_model::{ElementShape, LengthMm, ObjectId, Point3, RebarSet};
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
                    Command::Create { element } => Some(element.id),
                    _ => None,
                })
                .collect();
            b.editor.commit(&label, &commands).unwrap();
            ids
        }
        _ => panic!("no commit: {:?}", b.tools.message),
    }
}

fn beam(b: &mut Workbench, length: f64) -> ObjectId {
    b.execute(Cmd::ConcreteBeam);
    b.pick(DVec3::new(0., 0., 3000.));
    let r = b.pick(DVec3::new(length, 0., 3000.));
    commit(b, r)[0]
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() < tol
}

#[test]
fn classes_masses_bends_and_anchorage() {
    let names: Vec<&str> = model::REBAR_CLASSES.iter().map(|c| c.name).collect();
    assert_eq!(names, ["А240", "А400", "А500С", "В500С", "А600"]);
    let a500 = model::rebar_class("A500C").unwrap();
    assert_eq!((a500.rs, a500.rsc), (435., 435.));
    assert_eq!(model::rebar_class("в500с").unwrap().rsc, 415.);
    assert!(model::rebar_class("А800").is_none());
    // ГОСТ nominal masses.
    for (d, m) in [
        (6., 0.222),
        (12., 0.888),
        (16., 1.578),
        (25., 3.853),
        (40., 9.865),
    ] {
        assert_eq!(model::mass_per_m(d), m, "Ø{d}");
    }
    // Mandrels: plain 2.5d / 4d, ribbed 5d / 8d.
    let a240 = model::rebar_class("А240");
    assert_eq!(model::min_bend_diameter(a240, 12.), 30.);
    assert_eq!(model::min_bend_diameter(a240, 20.), 80.);
    assert_eq!(model::min_bend_diameter(Some(a500), 16.), 80.);
    assert_eq!(model::min_bend_diameter(Some(a500), 25.), 200.);
    // А500С Ø16 in В25: Rbond = 2.5·1.05, l0,an = 435·16 / (4·2.625).
    let t = model::anchorage(a500, 16., 1.05, 1., Stress::Tension);
    let basic = 435. * 16. / (4. * 2.625);
    assert!(near(t.basic, basic, 1e-9));
    assert!(near(t.anchorage, basic, 1e-9));
    assert!(near(t.lap, 1.2 * basic, 1e-9));
    let c = model::anchorage(a500, 16., 1.05, 1., Stress::Compression);
    assert!(near(c.anchorage, 0.75 * basic, 1e-9));
    assert!(near(c.lap, 0.9 * basic, 1e-9));
    // Plain bars bond less (η1 = 1.5); Ø36–Ø40 take η2 = 0.9.
    let a = model::anchorage(a240.unwrap(), 12., 1.05, 1., Stress::Tension);
    assert!(near(a.basic, 210. * 12. / (4. * 1.575), 1e-9));
    let a400 = model::rebar_class("А400").unwrap();
    let big = model::anchorage(a400, 40., 1.05, 1., Stress::Tension);
    assert!(near(big.basic, 350. * 40. / (4. * 2.5 * 0.9 * 1.05), 1e-9));
    // Little steel needed: the minimums 15·ds / 200 mm and 20·ds / 250 mm.
    let low = model::anchorage(a500, 10., 1.05, 0.1, Stress::Tension);
    let b10: f64 = 435. * 10. / (4. * 2.625);
    assert!(near(low.anchorage, (0.3 * b10).max(200.), 1e-9));
    assert!(near(low.lap, (0.4 * 1.2 * b10).max(250.), 1e-9));
    // The tool refuses a diameter the class is not rolled in.
    let s = RebarSettings {
        grade: "В500С".into(),
        diameter: 25.,
        ..Default::default()
    };
    assert!(rebar::single(DVec3::ZERO, DVec3::X * 1000., &s).is_err());
}

#[test]
fn hooks_and_developed_length() {
    let mut b = bench();
    let host = beam(&mut b, 6000.);
    b.rebar.settings.hook = HookKind::Deg90;
    b.editor.selected = [host].into();
    let r = b.execute(Cmd::RebarLongitudinal);
    commit(&mut b, r);
    let p = b.editor.document.project();
    let set = &p.concrete.rebars[0];
    assert_eq!(set.hooks, [Hook::new(HookKind::Deg90); 2]);
    let bounds = p.element_bounds(host).unwrap();
    let r = set.bend_radius();
    assert_eq!(r, 48.);
    for bar in &set.bars {
        // Leg, bar, leg; the legs turn into the section, 10·ds past the bend.
        assert_eq!(bar.len(), 4);
        for q in bar {
            assert!((0..3).all(|k| q.0[k] > bounds[0][k] - 1e-6 && q.0[k] < bounds[1][k] + 1e-6));
        }
        let leg = |a: &Point3, c: &Point3| {
            (0..3)
                .map(|k| (c.0[k] - a.0[k]).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        assert!(near(leg(&bar[0], &bar[1]), r + 160., 1e-6));
        // Bar between the bend corners: 6000 − 2·30 − 2·8.
        assert!(near(leg(&bar[1], &bar[2]), 5924., 1e-6));
        let cut = 5924. + 2. * (r + 160.) - 2. * (2. * r - r * std::f64::consts::FRAC_PI_2);
        assert!(near(set.developed_length(bar), cut, 1e-6));
    }
    // Stirrups: 135° hooks into the core at the first corner by default.
    let r = b.execute(Cmd::RebarStirrup);
    commit(&mut b, r);
    let st = &b.editor.document.project().concrete.rebars[1];
    let s0 = &st.bars[0];
    let into = |a: &Point3, c: &Point3| [c.0[0] - a.0[0], c.0[1] - a.0[1], c.0[2] - a.0[2]];
    let (h0, h1) = (into(&s0[1], &s0[0]), into(&s0[5], &s0[6]));
    for k in 0..3 {
        assert!(near(h0[k], h1[k], 1e-6), "{h0:?} {h1:?}");
    }
}

#[test]
fn bars_follow_the_host_and_long_bars_are_lapped() {
    let mut b = bench();
    let host = beam(&mut b, 6000.);
    b.editor.selected = [host].into();
    let r = b.execute(Cmd::RebarLongitudinal);
    commit(&mut b, r);
    let r = b.execute(Cmd::RebarStirrup);
    commit(&mut b, r);
    assert_eq!(
        b.editor.document.project().concrete.rebars[1].bars.len(),
        31
    );
    // Lengthen the beam: the bars and the stirrup count follow.
    let mut e = b.editor.document.project().elements[&host].clone();
    if let ElementShape::Member { length, .. } = &mut e.shape {
        *length = LengthMm::new(9000.).unwrap();
    }
    b.editor
        .commit("Длина", &[Command::ReplaceElement { element: e }])
        .unwrap();
    let p = b.editor.document.project();
    assert!(p.concrete.rebars[0].bars.iter().all(|bar| near(
        RebarSet::bar_length(bar),
        8940.,
        1e-6
    )));
    assert_eq!(p.concrete.rebars[1].bars.len(), 46);
    // Moving it moves the bars.
    let before = p.concrete.rebars[0].bars[0][0];
    b.editor
        .commit(
            "Сдвиг",
            &[Command::Transform {
                ids: vec![host],
                rotation: open_bim_model::Frame {
                    origin: Point3([0., 1000., 0.]),
                    ..Default::default()
                },
                pivot: Point3([0.; 3]),
            }],
        )
        .unwrap();
    let after = b.editor.document.project().concrete.rebars[0].bars[0][0];
    assert!(
        near(after.0[1] - before.0[1], 1000., 1e-6),
        "{before:?} {after:?}"
    );
    // Undo puts the bars back with the beam.
    b.editor.history(false).unwrap();
    assert_eq!(
        b.editor.document.project().concrete.rebars[0].bars[0][0],
        before
    );
    // Saved and read back with hooks and rule.
    let json = serde_json::to_string(b.editor.document.project()).unwrap();
    assert!(json.contains("\"rule\"") && json.contains("deg135"));
    let back: open_bim_model::Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.concrete, b.editor.document.project().concrete);
    // Deleting the beam removes its bars.
    b.editor
        .commit(
            "Удалить",
            &[Command::Delete {
                id: host,
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
    assert!(b.editor.document.project().concrete.rebars.is_empty());

    // A 15 m beam: bars longer than 11.7 m come in two lapped pieces.
    let mut b = bench();
    let host = beam(&mut b, 15000.);
    b.editor.selected = [host].into();
    let r = b.execute(Cmd::RebarLongitudinal);
    commit(&mut b, r);
    let p = b.editor.document.project();
    let set = &p.concrete.rebars[0];
    assert_eq!(set.bars.len(), 8);
    let lap = model::set_lap(p, set);
    assert!(lap > 700. && lap < 1000., "{lap}");
    let mut total = 0.;
    for bar in &set.bars {
        let l = RebarSet::bar_length(bar);
        assert!(l <= 11_700. + 1e-6);
        total += l;
    }
    // Four lines of 14 940 mm, each with one lap.
    assert!(near(total, 4. * (14_940. + lap), 1e-6), "{total}");
}
