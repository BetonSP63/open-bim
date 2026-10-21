//! «Сдвоенный профиль»: one command makes two mirrored members on the same
//! reference line with a gap between them.
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::twin::Twin;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ElementShape, ObjectId, Project};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn p(b: &Workbench) -> &Project {
    b.editor.document.project()
}

fn profile(b: &Workbench, name: &str) -> ObjectId {
    *p(b)
        .profiles
        .iter()
        .find(|(_, s)| s.name.starts_with(name))
        .unwrap()
        .0
}

/// Frame origin and local x of the members created by the last picks.
fn members(b: &mut Workbench, count: usize) -> Vec<([f64; 3], [f64; 3], ObjectId)> {
    b.pick(DVec3::new(0., 0., 0.));
    let r = b.pick(DVec3::new(3000., 0., 0.));
    let Some(Request::Action(Action::Commit(label, commands))) = r else {
        panic!("no members");
    };
    assert_eq!(commands.len(), count);
    b.editor.commit(&label, &commands).unwrap();
    let ids: Vec<_> = p(b)
        .elements
        .keys()
        .rev()
        .take(count)
        .rev()
        .copied()
        .collect();
    ids.iter()
        .map(|id| match &p(b).elements[id].shape {
            ElementShape::Member { frame, profile, .. } => {
                (frame.origin.0, frame.axes[0], *profile)
            }
            _ => panic!(),
        })
        .collect()
}

#[test]
fn channels_back_to_back_and_toes_in_with_a_gap() {
    let mut b = bench();
    let channel = profile(&b, "[20П");
    let w = match p(&b).profiles[&channel].shape {
        open_bim_model::ProfileShape::Channel { width, .. } => width,
        _ => panic!(),
    };
    b.execute(Cmd::ToolTwin);
    assert_eq!(b.editor.draft.twin, Twin::BackToBack);
    b.editor.draft.profile = channel;
    b.editor.draft.twin_gap = "10".into();
    // Along +X the section's local x is global Y; the web is at local -x.
    let m = members(&mut b, 2);
    assert!(m.iter().all(|(_, _, profile)| *profile == channel));
    let web = |(o, x, _): &([f64; 3], [f64; 3], ObjectId)| o[1] - x[1] * w / 2.;
    assert!((web(&m[0]) - 5.).abs() < 1e-9, "{m:?}");
    assert!((web(&m[1]) + 5.).abs() < 1e-9, "{m:?}");
    // Mirrored: flanges point away from each other.
    assert!(m[0].1[1] > 0.9 && m[1].1[1] < -0.9);

    b.editor.draft.twin = Twin::ToesIn;
    b.editor.draft.twin_gap = "20".into();
    let m = members(&mut b, 2);
    // Webs outside, toes 20 mm apart.
    let toe = |(o, x, _): &([f64; 3], [f64; 3], ObjectId)| o[1] + x[1] * w / 2.;
    assert!((toe(&m[0]) + 10.).abs() < 1e-9, "{m:?}");
    assert!((toe(&m[1]) - 10.).abs() < 1e-9, "{m:?}");

    // A wrong gap is refused with a reason.
    b.editor.draft.twin_gap = "-5".into();
    b.pick(DVec3::new(0., 1000., 0.));
    assert!(b.pick(DVec3::new(3000., 1000., 0.)).is_none());
}

#[test]
fn stacked_pair_and_plain_beam_after_it() {
    let mut b = bench();
    let beam = profile(&b, "30Б1");
    let h = match p(&b).profiles[&beam].shape {
        open_bim_model::ProfileShape::ISection { height, .. } => height,
        _ => panic!(),
    };
    b.execute(Cmd::ToolTwin);
    b.editor.draft.profile = beam;
    b.editor.draft.twin = Twin::Stacked;
    b.editor.draft.twin_gap = "0".into();
    let m = members(&mut b, 2);
    assert!((m[0].0[2] - h / 2.).abs() < 1e-9, "{m:?}");
    assert!((m[1].0[2] + h / 2.).abs() < 1e-9, "{m:?}");
    // The plain beam command makes one member again.
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolBeam);
    assert_eq!(b.editor.draft.twin, Twin::Single);
    members(&mut b, 1);
    // «Сдвоенный профиль» keeps an arrangement already chosen.
    b.execute(Cmd::CancelTool);
    b.editor.draft.twin = Twin::ToesIn;
    b.execute(Cmd::ToolTwin);
    assert_eq!(b.editor.draft.twin, Twin::ToesIn);
}

#[test]
fn twin_from_the_coordinate_fields() {
    let project = default_project().unwrap();
    let draft = open_bim_desktop::tools::Draft {
        twin: Twin::BackToBack,
        ..Default::default()
    };
    let commands = draft.commands(&project).unwrap();
    assert_eq!(commands.len(), 2);
    let mut doc = open_bim_commands::Document::new(project).unwrap();
    doc.transact("Сдвоенный профиль", &commands).unwrap();
}
