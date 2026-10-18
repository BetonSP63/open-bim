//! Grid lines are selectable and editable objects: click and area selection,
//! delete with undo, ГОСТ naming rules, handle drag, and editing the whole grid
//! through the dialog in one transaction.
use open_bim_commands::project_hash;
use open_bim_desktop::interaction::grid_edit::{
    pick_axis, spec_from_project, update_commands, validate_axis_name,
};
use open_bim_desktop::interaction::selection::{self, Click, Filter};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::ObjectId;
use open_bim_renderer::overlay::{HandleKind, Projector, handles};
use open_bim_renderer::{Bounds, Camera, DVec3, NamedView};
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        _ => false,
    }
}

fn axis(b: &Workbench, name: &str) -> ObjectId {
    b.editor
        .document
        .project()
        .axes
        .values()
        .find(|a| a.name == name)
        .unwrap()
        .id
}

fn top() -> Camera {
    let mut c = Camera::default();
    c.set_view(NamedView::Top);
    c.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 3600.),
    });
    c
}

fn projector(c: &Camera) -> Projector<'_> {
    Projector {
        camera: c,
        origin: DVec3::ZERO,
        min: [0., 0.],
        size: [1200., 800.],
    }
}

#[test]
fn click_near_a_grid_line_selects_only_that_line() {
    let mut b = bench();
    let c = top();
    let p = projector(&c);
    let on = p.screen(DVec3::new(6000., 9000., 0.)).unwrap();
    let cursor = [on[0] + 3., on[1]];
    let project = b.editor.document.project().clone();
    let hit = pick_axis(&project, &p, cursor, 6.);
    assert_eq!(hit, Some(axis(&b, "2")));
    b.select_clicked(None, hit, Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([axis(&b, "2")]));
    // Far from every line: nothing.
    let empty = p.screen(DVec3::new(3000., 3000., 0.)).unwrap();
    assert_eq!(pick_axis(&project, &p, empty, 6.), None);
    // Switch off: grid lines are not selected.
    b.editor.selected.clear();
    b.select.grids = false;
    b.select_clicked(None, hit, Click::Replace);
    assert!(b.editor.selected.is_empty());
    // A part under the cursor wins over the grid line.
    b.select.grids = true;
    b.execute(Cmd::ToolColumn);
    let r = b.pick(DVec3::new(6000., 6000., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let column = *b.editor.document.project().elements.keys().next().unwrap();
    b.select_clicked(Some(column), hit, Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([column]));
}

#[test]
fn crossing_selects_grid_lines_and_delete_is_undoable() {
    let mut b = bench();
    let c = top();
    let p = projector(&c);
    let project = b.editor.document.project().clone();
    let s = |x: f64, y: f64| p.screen(DVec3::new(x, y, 0.)).unwrap();
    // Right→left crossing box around x 5000…7000, y 2000…4000: touches line «2» only…
    let ids = selection::area(
        &project,
        &p,
        &BTreeSet::new(),
        s(7000., 4000.),
        s(5000., 2000.),
        Filter::default(),
    );
    assert_eq!(ids, BTreeSet::from([axis(&b, "2")]));
    // …and a taller one also crosses line «Б» (y = 6000).
    let ids = selection::area(
        &project,
        &p,
        &BTreeSet::new(),
        s(7000., 7000.),
        s(5000., 2000.),
        Filter::default(),
    );
    assert_eq!(ids, BTreeSet::from([axis(&b, "2"), axis(&b, "Б")]));
    let before = project_hash(&project).unwrap();
    b.editor.selected = ids;
    let r = b.execute(Cmd::DeleteSelected);
    assert!(apply(&mut b, r));
    assert_eq!(b.editor.document.project().axes.len(), 7);
    b.editor.history(false).unwrap();
    assert_eq!(project_hash(b.editor.document.project()).unwrap(), before);
}

#[test]
fn axis_names_follow_gost_and_rename_is_one_transaction() {
    for bad in [
        "Е", "З", "Й", "О", "Х", "Ц", "Ч", "Щ", "Ъ", "Ы", "Ь", "I", "O", "б", "",
    ] {
        assert!(validate_axis_name(bad).is_err(), "{bad:?} must be rejected");
    }
    for good in ["А", "Ж", "ББ", "12", "1/1", "Б/2", "A"] {
        validate_axis_name(good).unwrap();
    }
    let mut b = bench();
    let id = axis(&b, "Г");
    let mut renamed = b.editor.document.project().axes[&id].clone();
    renamed.name = "Д".into();
    let depth = b.editor.document.history_depth().0;
    b.editor
        .commit(
            "Изменить ось",
            &[open_bim_commands::Command::ReplaceAxis { axis: renamed }],
        )
        .unwrap();
    assert_eq!(b.editor.document.history_depth().0, depth + 1);
    assert_eq!(b.editor.document.project().axes[&id].name, "Д");
}

#[test]
fn grid_dialog_reads_the_grid_and_rebuilds_it_in_one_step() {
    let mut b = bench();
    // A column on node 2/Б must stay where it is when the grid changes.
    b.execute(Cmd::ToolColumn);
    let r = b.pick(DVec3::new(6000., 6000., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let project = b.editor.document.project().clone();
    let spec = spec_from_project(&project).unwrap();
    assert_eq!(spec.x, "0 4*6000");
    assert_eq!(spec.labels_x, "1 2 3 4 5");
    assert_eq!(spec.y, "0 3*6000");
    assert_eq!(spec.labels_y, "А Б В Г");
    assert_eq!(spec.z, "0 3600");
    assert_eq!(spec.extension, "2000");
    let before = project_hash(&project).unwrap();
    let mut wider = spec.clone();
    wider.x = "0 4*7200".into();
    let commands = update_commands(&project, &wider).unwrap();
    let depth = b.editor.document.history_depth().0;
    b.editor.commit("Изменить: сетка", &commands).unwrap();
    assert_eq!(
        b.editor.document.history_depth().0,
        depth + 1,
        "one transaction"
    );
    let after = b.editor.document.project();
    assert_eq!(after.axes[&axis(&b, "5")].start.0[0], 28_800.);
    assert_eq!(after.elements, project.elements, "parts untouched");
    assert_eq!(spec_from_project(after).unwrap().x, "0 4*7200");
    b.editor.history(false).unwrap();
    assert_eq!(project_hash(b.editor.document.project()).unwrap(), before);
    // Labels with a forbidden letter are refused.
    let mut bad = spec;
    bad.labels_y = "А Б В Е".into();
    assert!(update_commands(&project, &bad).is_err());
}

#[test]
fn grid_line_end_handle_drags_like_a_part_handle() {
    let mut b = bench();
    let id = axis(&b, "1");
    let c = top();
    let p = projector(&c);
    let project = b.editor.document.project().clone();
    let hs = handles(&project, &BTreeSet::from([id]), &p);
    let end = hs.iter().find(|h| h.kind == HandleKind::End).unwrap();
    let commands = selection::move_handle(&project, end, DVec3::new(0., 22_000., 0.)).unwrap();
    b.editor.commit("Ручка оси", &commands).unwrap();
    let a = &b.editor.document.project().axes[&id];
    assert_eq!(a.end.0, [0., 22_000., 0.]);
    assert_eq!(a.start.0, [0., -2000., 0.], "start unchanged");
}
