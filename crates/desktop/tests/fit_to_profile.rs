//! Sizes of connections picked from the section of the main part: by
//! «Автосоединения» and by «Подобрать узлы по профилю».
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{Element, ElementShape, Frame, MemberKind, ObjectId, Point3, TemplateKind};

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

fn run(b: &mut Workbench, cmd: Cmd) -> bool {
    match b.execute(cmd) {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        None => false,
        _ => panic!("unexpected request"),
    }
}

fn frame() -> (Workbench, ObjectId) {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    member(
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
    (b, beam)
}

#[test]
fn auto_connections_take_sizes_from_the_i300_section() {
    let (mut b, beam) = frame();
    assert!(run(&mut b, Cmd::AutoConnections));
    let p = b.editor.document.project();
    let mut beam_ends = 0;
    for c in p.connections.values() {
        let t = &c.template;
        // I 300: М20 with a Ø22 hole, 2d = 45 mm to the edge.
        assert_eq!(
            (t.bolt_diameter, t.clearance, t.edge_distance),
            (20., 2., 45.),
            "{}",
            t.name
        );
        match t.kind {
            TemplateKind::BasePlate { overhang } => {
                assert_eq!((overhang, t.thickness), (65., 20.));
            }
            TemplateKind::FinPlate {
                height,
                pitch,
                rows,
                ..
            } => {
                assert_eq!(c.primary, beam);
                // 0,7 of the 276 mm web, two bolts at 60 mm.
                assert_eq!((height, pitch, rows, t.thickness), (190., 60., 2, 10.));
                beam_ends += 1;
            }
            ref other => panic!("{other:?}"),
        }
        assert!(t.weld_size >= open_bim_connections::norms::min_weld(t.thickness));
    }
    assert_eq!(beam_ends, 2);
}

#[test]
fn fit_to_profile_resets_edited_sizes_and_skips_locked_components() {
    let (mut b, _) = frame();
    assert!(run(&mut b, Cmd::AutoConnections));
    let p = b.editor.document.project();
    let mut edited: Vec<_> = p
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::FinPlate { .. }))
        .cloned()
        .collect();
    edited[0].template.thickness = 16.;
    edited[1].template.thickness = 16.;
    edited[1].general.locked = true;
    let commands: Vec<_> = edited
        .iter()
        .cloned()
        .map(|connection| open_bim_commands::Command::ReplaceConnection { connection })
        .collect();
    b.editor.commit("Изменить", &commands).unwrap();
    assert!(run(&mut b, Cmd::FitToProfile));
    let p = b.editor.document.project();
    assert_eq!(p.connections[&edited[0].id].template.thickness, 10.);
    assert_eq!(p.connections[&edited[1].id].template.thickness, 16.);
    // Nothing left to change: no commit, a message instead.
    assert!(!run(&mut b, Cmd::FitToProfile));
    assert!(
        b.tools
            .message
            .as_deref()
            .unwrap()
            .contains("менять нечего")
    );
}
