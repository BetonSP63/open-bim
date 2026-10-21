//! «Автосоединения»: free member ends get the first fitting component of
//! their rule, and running it again adds nothing.
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{Element, ElementShape, Frame, MemberKind, ObjectId, Point3};

/// The analytic test sections these geometry checks are written for
/// (profile 2 is I 300×150×8×12), with the default grid; new models in the
/// app start with ГОСТ sections instead.
fn default_project() -> Result<open_bim_model::Project, String> {
    with_default_grid(open_bim_model::Project::default())
}

fn member(b: &mut Workbench, kind: MemberKind, start: [f64; 3], end: [f64; 3]) -> ObjectId {
    let project = b.editor.document.project();
    let id = ObjectId(project.next_id);
    let (frame, length) = Frame::along(Point3(start), Point3(end)).unwrap();
    let element = Element {
        id,
        name: "Деталь".into(),
        material: ObjectId(1),
        shape: ElementShape::Member {
            member_kind: kind,
            profile: ObjectId(2),
            length,
            frame,
            position: Default::default(),
        },
        properties: Default::default(),
    };
    b.editor
        .commit("Деталь", &[open_bim_commands::Command::Create { element }])
        .unwrap();
    id
}

fn run(b: &mut Workbench) -> bool {
    match b.execute(Cmd::AutoConnections) {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        None => false,
        _ => panic!("unexpected request"),
    }
}

fn names(b: &Workbench) -> Vec<(String, ObjectId, bool)> {
    let mut v: Vec<_> = b
        .editor
        .document
        .project()
        .connections
        .values()
        .map(|c| (c.template.name.clone(), c.primary, c.at_end))
        .collect();
    v.sort();
    v
}

#[test]
fn a_frame_gets_bases_and_beam_ends_then_nothing_more() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    // Two I 300 columns, flange faces at Y = ±150 around each; a beam
    // between them with a 15 mm gap at both ends.
    let left = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let right = member(
        &mut b,
        MemberKind::Column,
        [0., 6300., 0.],
        [0., 6300., 3600.],
    );
    let beam = member(
        &mut b,
        MemberKind::Beam,
        [0., 165., 3000.],
        [0., 6135., 3000.],
    );
    assert!(run(&mut b));
    assert_eq!(
        names(&b),
        vec![
            ("Опорная плита".into(), left, false),
            ("Опорная плита".into(), right, false),
            ("Шарнир на пластине".into(), beam, false),
            ("Шарнир на пластине".into(), beam, true),
        ]
    );
    // Everything is connected now.
    assert!(!run(&mut b));
}

#[test]
fn collinear_beams_get_one_splice_and_a_selection_limits_the_scope() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    let column = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let first = member(
        &mut b,
        MemberKind::Beam,
        [0., 6000., 3000.],
        [0., 9000., 3000.],
    );
    let second = member(
        &mut b,
        MemberKind::Beam,
        [0., 9010., 3000.],
        [0., 12000., 3000.],
    );
    // Only the two beams are selected: the column base is left alone.
    b.editor.selected = [first, second].into();
    assert!(run(&mut b));
    assert_eq!(
        names(&b),
        vec![("Стык балок на накладках".into(), first, true)]
    );
    assert!(
        !b.editor
            .document
            .project()
            .connections
            .values()
            .any(|c| c.primary == column)
    );
}
