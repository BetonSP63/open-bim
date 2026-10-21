//! Components as model objects: a symbol at the joint coloured by status,
//! picked and selected like other objects, opened by a double click, changed
//! for several selected components at once.
use open_bim_commands::Command;
use open_bim_desktop::interaction::component_symbols::{
    Status, components_of, pick, status, symbols,
};
use open_bim_desktop::interaction::selection::Click;
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, chrome::SidePanel, commands::Cmd};
use open_bim_model::{ConnectionInstance, ConnectionTemplate, ObjectId, TemplateKind};
use open_bim_renderer::overlay::Projector;
use open_bim_renderer::{Bounds, Camera, DVec3};
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn beam(b: &mut Workbench, a: [f64; 3], c: [f64; 3]) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::from_array(a));
    let r = b.pick(DVec3::from_array(c));
    match r {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("beam not created"),
    }
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn template() -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Стык балок".into(),
        kind: TemplateKind::BeamSplice {
            rows: 2,
            pitch: 70.,
        },
        thickness: 10.,
        edge_distance: 50.,
        bolt_diameter: 20.,
        clearance: 2.,
        bolt_standard: "ГОСТ 7798-70".into(),
        bolt_grade: "5.8".into(),
        weld_size: 6.,
    }
}

/// Two beams end to end along +X at `y` with a 10 mm gap, spliced.
fn splice(b: &mut Workbench, y: f64) -> (ObjectId, ObjectId) {
    let left = beam(b, [0., y, 0.], [3000., y, 0.]);
    let right = beam(b, [3010., y, 0.], [6000., y, 0.]);
    let id = ObjectId(b.editor.document.project().next_id);
    b.editor
        .commit(
            "Стык балок",
            &[Command::CreateConnection {
                connection: ConnectionInstance {
                    id,
                    primary: left,
                    secondary: Some(right),
                    at_end: true,
                    template: template(),
                    children: Default::default(),
                    general: Default::default(),
                },
            }],
        )
        .unwrap();
    (id, right)
}

fn camera() -> Camera {
    let mut c = Camera::default();
    c.fit(Bounds {
        min: DVec3::new(-1000., -1000., -500.),
        max: DVec3::new(7000., 5000., 500.),
    });
    c
}

#[test]
fn symbol_at_the_joint_shows_status_and_is_picked() {
    let mut b = bench();
    let (id, right) = splice(&mut b, 0.);
    let c = camera();
    let p = Projector {
        camera: &c,
        origin: DVec3::ZERO,
        min: [0., 0.],
        size: [1200., 800.],
    };
    let project = b.editor.document.project().clone();
    let none = BTreeSet::new();
    let marks = symbols(&project, &p, &none, &none);
    assert_eq!(marks.len(), 1);
    let s = &marks[0];
    assert_eq!(s.id, id);
    assert_eq!(s.status, Status::Created);
    assert_eq!(s.label, "Стык балок");
    // Midway between the beam ends at x = 3000 and 3010.
    let joint = p.screen(DVec3::new(3005., 0., 0.)).unwrap();
    assert!((s.at[0] - joint[0]).abs() < 2. && (s.at[1] - joint[1]).abs() < 2.);
    assert_eq!(pick(&project, &p, &none, s.at, 8.), Some(id));
    assert_eq!(
        pick(&project, &p, &none, [s.at[0] + 40., s.at[1]], 8.),
        None
    );
    // A hidden main part hides its component.
    let hidden = BTreeSet::from([project.connections[&id].primary]);
    assert!(symbols(&project, &p, &hidden, &none).is_empty());
    // The parts the component made lead back to it.
    let plate = *project.connections[&id].children.values().next().unwrap();
    assert_eq!(
        components_of(&project, &BTreeSet::from([plate])),
        BTreeSet::from([id])
    );

    // Remarks: a bolt too close to the edge breaks the norms (yellow).
    let mut near = project.connections[&id].clone();
    near.template.edge_distance = 12.;
    assert!(matches!(status(&near), Status::Remarks(r) if !r.is_empty()));

    // The second beam moved away: the component stays, red.
    let mut moved = project.elements[&right].clone();
    moved.shape.frame_mut().origin.0[0] += 800.;
    b.editor
        .commit("Переместить", &[Command::ReplaceElement { element: moved }])
        .unwrap();
    let c = &b.editor.document.project().connections[&id];
    assert_eq!(status(c), Status::Failed);
}

#[test]
fn click_selects_and_double_click_opens_the_component() {
    let mut b = bench();
    let (id, _) = splice(&mut b, 0.);
    b.select_clicked(None, Some(id), Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([id]));
    // The switch «Выбор узлов» off: the symbol is not selected.
    b.select.components = false;
    b.select_clicked(None, Some(id), Click::Replace);
    assert!(b.editor.selected.is_empty());
    b.select.components = true;

    b.open_component(id);
    assert_eq!(b.side_panel, Some(SidePanel::Connections));
    assert_eq!(b.tools.connection.editing, Some(id));
    assert_eq!(b.editor.selected, BTreeSet::from([id]));
    // «Общие» are loaded and saved with the component.
    b.tools.connection.general.code = "СБ-1".into();
    let commands = b
        .tools
        .connection
        .commands(b.editor.document.project())
        .unwrap();
    b.editor.commit("Изменить узел", &commands).unwrap();
    let c = &b.editor.document.project().connections[&id];
    assert_eq!(c.general.code, "СБ-1");
    // Symbols on and off.
    assert!(b.show_components);
    b.execute(Cmd::ToggleComponentSymbols);
    assert!(!b.show_components);
}

#[test]
fn modify_several_selected_components() {
    let mut b = bench();
    let (first, _) = splice(&mut b, 0.);
    let (second, _) = splice(&mut b, 3000.);
    let selected = BTreeSet::from([first, second]);
    let project = b.editor.document.project().clone();
    let ui = &mut b.tools.connection;
    ui.load(&project, first).unwrap();
    ui.template.thickness = 12.;
    let (commands, skipped) = ui.modify_selected(&project, &selected).unwrap();
    assert_eq!((commands.len(), skipped), (2, 0));
    b.editor.commit("Изменить узлы", &commands).unwrap();
    for id in [first, second] {
        assert_eq!(
            b.editor.document.project().connections[&id]
                .template
                .thickness,
            12.
        );
    }

    // A locked component is skipped.
    let mut locked = b.editor.document.project().connections[&second].clone();
    locked.general.locked = true;
    b.editor
        .commit(
            "Заблокировать",
            &[Command::ReplaceConnection { connection: locked }],
        )
        .unwrap();
    let project = b.editor.document.project().clone();
    let ui = &mut b.tools.connection;
    ui.load(&project, first).unwrap();
    ui.template.thickness = 14.;
    let (commands, skipped) = ui.modify_selected(&project, &selected).unwrap();
    assert_eq!((commands.len(), skipped), (1, 1));

    // «Только узлы того же типа»: another type leaves these alone.
    ui.template.kind = TemplateKind::FinPlate {
        height: 200.,
        lap: 60.,
        gap: 10.,
        pitch: 70.,
        rows: 3,
    };
    assert!(ui.modify_selected(&project, &selected).is_err());
}
