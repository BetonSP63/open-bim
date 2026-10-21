//! Object groups with «И» / «ИЛИ» rules, the object representation (colour,
//! see-through, hidden per group) and the selection filter by a group.
use open_bim_desktop::interaction::object_groups::{
    Condition, Field, Group, Join, RepresentationRow, Rule, Visibility,
};
use open_bim_desktop::interaction::selection::{Click, PropertyFilter};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ToolKind, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::ObjectId;
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn commit(b: &mut Workbench, r: Option<Request>) -> ObjectId {
    let Some(Request::Action(Action::Commit(label, commands))) = r else {
        panic!("not created: {:?}", b.tools.message);
    };
    b.editor.commit(&label, &commands).unwrap();
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn beam(b: &mut Workbench, y: f64, length: f64) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::new(0., y, 0.));
    let r = b.pick(DVec3::new(length, y, 0.));
    commit(b, r)
}

/// A beam 3000, a beam 1500, a column and a plate.
fn model(b: &mut Workbench) -> [ObjectId; 4] {
    let long = beam(b, 0., 3000.);
    let short = beam(b, 2000., 1500.);
    b.execute(Cmd::ToolColumn);
    let r = b.pick(DVec3::new(6000., 6000., 0.));
    let column = commit(b, r);
    b.editor.draft.kind = ToolKind::Plate;
    let commands = b
        .editor
        .draft
        .commands(b.editor.document.project())
        .unwrap();
    b.editor.commit("Пластина", &commands).unwrap();
    let plate = *b.editor.document.project().elements.keys().last().unwrap();
    [long, short, column, plate]
}

fn rule(join: Join, field: Field, condition: Condition, value: &str) -> Rule {
    Rule {
        join,
        field,
        condition,
        value: value.into(),
    }
}

#[test]
fn rules_with_and_or() {
    let mut b = bench();
    let [long, short, column, plate] = model(&mut b);
    let p = b.editor.document.project();
    // (Тип = Балка И Профиль = 30Б1* И Длина > 2000) ИЛИ Тип = Пластина.
    let g = Group {
        name: "Длинные балки и пластины".into(),
        rules: vec![
            rule(Join::And, Field::Kind, Condition::Equals, "Балка"),
            rule(Join::And, Field::Profile, Condition::Equals, "30Б1*"),
            rule(Join::And, Field::Length, Condition::Greater, "2000"),
            rule(Join::Or, Field::Kind, Condition::Equals, "пластина"),
        ],
    };
    assert!(g.matches(p, long));
    assert!(!g.matches(p, short));
    assert!(g.matches(p, plate));
    assert!(!g.matches(p, column));
    // A property the part lacks: «не равно» matches, «равно» does not.
    let missing = Group {
        name: "Без покрытия".into(),
        rules: vec![rule(
            Join::And,
            Field::Property("surface_treatment".into()),
            Condition::NotEquals,
            "*",
        )],
    };
    assert!(missing.matches(p, long));
    // Plates by thickness with a typed hyphen.
    let thick = Group {
        name: "—20".into(),
        rules: vec![rule(Join::And, Field::Profile, Condition::Equals, "-20")],
    };
    assert!(thick.matches(p, plate) && !thick.matches(p, long));
    // An empty group is every part; a grid axis is never in a group.
    let all = Group {
        name: "Все".into(),
        rules: Vec::new(),
    };
    assert!(all.matches(p, column));
    let axis = *p.axes.keys().next().unwrap();
    assert!(!all.matches(p, axis));
}

#[test]
fn representation_colours_hides_and_is_cached() {
    let mut b = bench();
    let [long, short, column, plate] = model(&mut b);
    b.execute(Cmd::ObjectRepresentation);
    assert!(b.representation_open);
    // Default groups: «Колонны», «Балки», «Пластины».
    let index = |b: &Workbench, name: &str| b.groups.iter().position(|g| g.name == name).unwrap();
    b.representation = vec![
        RepresentationRow {
            group: index(&b, "Колонны"),
            color: Some([255, 0, 0]),
            visibility: Visibility::Visible,
        },
        RepresentationRow {
            group: index(&b, "Балки"),
            color: None,
            visibility: Visibility::Transparent(70),
        },
        RepresentationRow {
            group: index(&b, "Пластины"),
            color: None,
            visibility: Visibility::Hidden,
        },
    ];
    b.representation_version += 1;
    let shared = b.applied_representation(1);
    assert_eq!(shared.appearance[&column].color, Some([1., 0., 0.]));
    assert_eq!(shared.appearance[&column].alpha, 1.);
    for beam in [long, short] {
        assert!(shared.appearance[&beam].color.is_none());
        assert!((shared.appearance[&beam].alpha - 0.3).abs() < 1e-6);
    }
    assert_eq!(*shared.hidden, BTreeSet::from([plate]));
    // Same scene and version: the same shared result.
    let again = b.applied_representation(1);
    assert!(std::sync::Arc::ptr_eq(
        &shared.appearance,
        &again.appearance
    ));
    // The first matching row wins.
    b.representation.insert(
        0,
        RepresentationRow {
            group: index(&b, "Балки"),
            color: Some([0, 0, 255]),
            visibility: Visibility::Visible,
        },
    );
    b.representation_version += 1;
    let shared = b.applied_representation(1);
    assert_eq!(shared.appearance[&long].alpha, 1.);
    assert_eq!(shared.appearance[&long].color, Some([0., 0., 1.]));
}

#[test]
fn selection_filter_by_group() {
    let mut b = bench();
    let [long, _, column, _] = model(&mut b);
    let columns = b.groups.iter().find(|g| g.name == "Колонны").cloned();
    b.filter = PropertyFilter {
        enabled: true,
        group: columns,
        ..Default::default()
    };
    assert!(b.filter.active());
    b.select_picked(Some(long), Click::Replace);
    assert!(b.editor.selected.is_empty());
    b.select_picked(Some(column), Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([column]));
}
