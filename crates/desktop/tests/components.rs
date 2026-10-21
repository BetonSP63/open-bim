//! Component catalogue: arm a component, click the beam and the column, and
//! the connection lands at the beam end next to the column.
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::components::{catalog, search};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{Element, ElementShape, Frame, MemberKind, ObjectId, Point3};

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

fn commit(b: &mut Workbench, r: Option<Request>) {
    match r {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("nothing to commit"),
    }
}

#[test]
fn catalogue_search_finds_the_new_fin_plate() {
    assert!(catalog().len() >= 4);
    let found = search("шарнир");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Шарнир на пластине");
    assert!(search("колонн").iter().any(|c| c.name == "Опорная плита"));
}

#[test]
fn fin_plate_by_two_clicks_goes_to_the_column_end_of_the_beam() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    // Column I 300 along Z: flange faces at Y = ±150. Beam along +Y, its
    // start 15 mm off the flange face.
    let column = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let beam = member(
        &mut b,
        MemberKind::Beam,
        [0., 165., 3000.],
        [0., 6000., 3000.],
    );
    b.execute(Cmd::ComponentCatalog);
    assert!(b.components.window);
    let fin = search("шарнир").remove(0);
    b.arm_component(fin);
    assert!(b.picks_objects());
    assert!(b.pick_object(Some(beam)).is_none(), "waits for the support");
    let r = b.pick_object(Some(column));
    commit(&mut b, r);
    let project = b.editor.document.project();
    let c = project.connections.values().next().unwrap();
    assert_eq!((c.primary, c.secondary), (beam, Some(column)));
    assert!(!c.at_end, "the beam start is next to the column");
    assert_eq!(c.children.len(), 3 + 2 * 2);
    // The command stays armed for the next joint; Esc ends it.
    assert!(b.components.armed.is_some());
    b.execute(Cmd::CancelTool);
    assert!(b.components.armed.is_none());
}

#[test]
fn column_stiffeners_by_two_clicks_follow_the_beam_flanges() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    let column = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let beam = member(
        &mut b,
        MemberKind::Beam,
        [0., 150., 3000.],
        [0., 6000., 3000.],
    );
    let stiffeners = search("рёбра").remove(0);
    assert_eq!(stiffeners.name, "Рёбра жёсткости колонны");
    b.arm_component(stiffeners);
    assert!(b.pick_object(Some(column)).is_none(), "waits for the beam");
    let r = b.pick_object(Some(beam));
    commit(&mut b, r);
    let project = b.editor.document.project();
    let c = project.connections.values().next().unwrap();
    assert_eq!((c.primary, c.secondary), (column, Some(beam)));
    assert_eq!(c.children.len(), 8);
}

#[test]
fn beam_splice_by_two_clicks_goes_at_the_shared_ends() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    let left = member(
        &mut b,
        MemberKind::Beam,
        [0., 0., 3000.],
        [0., 3000., 3000.],
    );
    let right = member(
        &mut b,
        MemberKind::Beam,
        [0., 3010., 3000.],
        [0., 6000., 3000.],
    );
    let splice = search("стык").remove(0);
    assert_eq!(splice.name, "Стык балок на накладках");
    b.arm_component(splice);
    assert!(
        b.pick_object(Some(left)).is_none(),
        "waits for the second beam"
    );
    let r = b.pick_object(Some(right));
    commit(&mut b, r);
    let project = b.editor.document.project();
    let c = project.connections.values().next().unwrap();
    assert_eq!((c.primary, c.secondary), (left, Some(right)));
    assert!(c.at_end, "the end of the first beam meets the second");
}

#[test]
fn haunch_by_two_clicks_goes_under_the_column_end() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    let column = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let beam = member(
        &mut b,
        MemberKind::Beam,
        [0., 150., 3000.],
        [0., 6000., 3000.],
    );
    let haunch = search("вута").remove(0);
    assert_eq!(haunch.name, "Вута");
    b.arm_component(haunch);
    assert!(b.pick_object(Some(beam)).is_none(), "waits for the support");
    let r = b.pick_object(Some(column));
    commit(&mut b, r);
    let c = b
        .editor
        .document
        .project()
        .connections
        .values()
        .next()
        .unwrap();
    assert_eq!(
        (c.primary, c.secondary, c.at_end),
        (beam, Some(column), false)
    );
}

#[test]
fn gusset_by_two_clicks_goes_at_the_column_end_of_the_brace() {
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    let column = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let brace = member(
        &mut b,
        MemberKind::Beam,
        [0., 400., 300.],
        [0., 3000., 3000.],
    );
    // The brace is an angle L 100×75×8.
    let mut element = b.editor.document.project().elements[&brace].clone();
    if let ElementShape::Member { profile, .. } = &mut element.shape {
        *profile = ObjectId(3);
    }
    b.editor
        .commit(
            "Уголок",
            &[open_bim_commands::Command::ReplaceElement { element }],
        )
        .unwrap();
    let gusset = search("фасонка").remove(0);
    assert_eq!(gusset.name, "Фасонка связи");
    b.arm_component(gusset);
    assert!(
        b.pick_object(Some(brace)).is_none(),
        "waits for the support"
    );
    let r = b.pick_object(Some(column));
    commit(&mut b, r);
    let c = b
        .editor
        .document
        .project()
        .connections
        .values()
        .next()
        .unwrap();
    assert_eq!(
        (c.primary, c.secondary, c.at_end),
        (brace, Some(column), false)
    );
}
