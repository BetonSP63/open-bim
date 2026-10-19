//! Member length with start/end/both editing, selectable and editable levels,
//! orbit about the point under the cursor, trackpad input and the display
//! modes with transparency.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_desktop::interaction::grid_edit::{pick_axis, pick_level};
use open_bim_desktop::interaction::member_edit::{LengthMode, drag_handle, ends, resize};
use open_bim_desktop::interaction::orbit::{GESTURE_GAP, Orbit, split_scroll};
use open_bim_desktop::interaction::selection::{self, Click, Filter};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::property_pane::{Changes, modify};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::ObjectId;
use open_bim_renderer::overlay::{HandleKind, Projector, handles, level_line};
use open_bim_renderer::{
    Bounds, Camera, DVec3, Display, DisplayMesh, NamedView, RenderMode, TransparencyScope,
    feature_edges,
};
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

/// A 6000 mm beam along X at +3,600 from А/1 to А/2.
fn with_beam() -> (Workbench, ObjectId) {
    let mut b = bench();
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::new(0., 0., 3600.));
    let r = b.pick(DVec3::new(6000., 0., 3600.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let id = *b.editor.document.project().elements.keys().next().unwrap();
    (b, id)
}

fn beam_ends(b: &Workbench, id: ObjectId) -> (DVec3, DVec3) {
    ends(&b.editor.document.project().elements[&id].shape).unwrap()
}

fn close(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-6
}

#[test]
fn length_moves_start_end_or_both_in_one_transaction() {
    let (mut b, id) = with_beam();
    for (mode, start, end) in [
        (LengthMode::End, 0., 7000.),
        (LengthMode::Start, -1000., 6000.),
        (LengthMode::Both, -500., 6500.),
    ] {
        let depth = b.editor.document.history_depth().0;
        let project = b.editor.document.project().clone();
        let commands = modify(
            &project,
            &[id],
            &Changes {
                length: Some((7000., mode)),
                ..Default::default()
            },
        )
        .unwrap();
        b.editor.commit("Длина", &commands).unwrap();
        assert_eq!(b.editor.document.history_depth().0, depth + 1, "{mode:?}");
        let (a, e) = beam_ends(&b, id);
        assert!(close(a, DVec3::new(start, 0., 3600.)), "{mode:?} start {a}");
        assert!(close(e, DVec3::new(end, 0., 3600.)), "{mode:?} end {e}");
        b.editor.history(false).unwrap();
        let (a, e) = beam_ends(&b, id);
        assert!(close(a, DVec3::new(0., 0., 3600.)) && close(e, DVec3::new(6000., 0., 3600.)));
    }
    // The pane shows the length of the selection.
    b.editor.selected = BTreeSet::from([id]);
    let common = open_bim_desktop::ui::property_pane::common(
        b.editor.document.project(),
        &b.editor.selected,
    )
    .unwrap();
    assert_eq!(common.length.unwrap().value().unwrap(), "6000");
    // Plates have no length; zero length is refused.
    let shape = &b.editor.document.project().elements[&id].shape;
    assert!(resize(shape, 0., LengthMode::End).is_err());
}

#[test]
fn handle_drag_in_both_mode_moves_the_whole_member() {
    let (b, id) = with_beam();
    let project = b.editor.document.project().clone();
    let mut c = Camera::default();
    c.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 3600.),
    });
    let p = Projector {
        camera: &c,
        origin: DVec3::ZERO,
        min: [0., 0.],
        size: [1200., 800.],
    };
    let hs = handles(&project, &BTreeSet::from([id]), &p);
    let end = hs.iter().find(|h| h.kind == HandleKind::End).unwrap();
    let target = end.world + DVec3::new(0., 1000., 0.);
    let moved = drag_handle(&project, end, target, LengthMode::Both).unwrap();
    let Command::ReplaceElement { element } = &moved[0] else {
        panic!()
    };
    let (a, e) = ends(&element.shape).unwrap();
    assert!(close(a, DVec3::new(0., 1000., 3600.)) && close(e, DVec3::new(6000., 1000., 3600.)));
    let only_end = drag_handle(&project, end, target, LengthMode::End).unwrap();
    let Command::ReplaceElement { element } = &only_end[0] else {
        panic!()
    };
    let (a, e) = ends(&element.shape).unwrap();
    assert!(close(a, DVec3::new(0., 0., 3600.)) && close(e, target));
}

fn top() -> Camera {
    let mut c = Camera::default();
    c.set_view(NamedView::Front);
    c.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 3600.),
    });
    c
}

#[test]
fn levels_are_selected_by_label_or_line_and_edited() {
    let mut b = bench();
    let project = b.editor.document.project().clone();
    let c = top();
    let p = Projector {
        camera: &c,
        origin: DVec3::ZERO,
        min: [0., 0.],
        size: [1200., 800.],
    };
    let upper = project
        .levels
        .values()
        .find(|l| l.name == "+3,600")
        .unwrap()
        .clone();
    let (a, bb) = level_line(&project, &upper).unwrap();
    let mid = p.screen((a + bb) * 0.5).unwrap();
    // On the level line: the level, not the grid line copy.
    assert_eq!(
        pick_level(&project, &p, [mid[0], mid[1] + 2.], 6.),
        Some(upper.id)
    );
    // On its label, left of the line start.
    let start = p.screen(a).unwrap();
    assert_eq!(
        pick_level(&project, &p, [start[0] - 40., start[1]], 6.),
        Some(upper.id)
    );
    // The zero level line is the grid axis itself: the axis wins there.
    let zero = project.levels.values().find(|l| l.name == "0,000").unwrap();
    let (z0, z1) = level_line(&project, zero).unwrap();
    let zm = p.screen((z0 + z1) * 0.5).unwrap();
    assert_eq!(pick_level(&project, &p, zm, 6.), None);
    assert!(pick_axis(&project, &p, zm, 6.).is_some());
    // Click selection respects the «Уровни» switch.
    b.select_clicked(None, Some(upper.id), Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([upper.id]));
    b.editor.selected.clear();
    b.select.levels = false;
    b.select_clicked(None, Some(upper.id), Click::Replace);
    assert!(b.editor.selected.is_empty());
    // Area selection crossing the level line.
    let only_levels = Filter {
        parts: false,
        grids: false,
        levels: true,
        ..Filter::default()
    };
    let ids = selection::area(
        &project,
        &p,
        &BTreeSet::new(),
        [mid[0] + 20., mid[1] + 20.],
        [mid[0] - 20., mid[1] - 20.],
        only_levels,
    );
    assert_eq!(ids, BTreeSet::from([upper.id]));
    let none = Filter {
        levels: false,
        ..only_levels
    };
    assert!(
        selection::area(
            &project,
            &p,
            &BTreeSet::new(),
            [mid[0] + 20., mid[1] + 20.],
            [mid[0] - 20., mid[1] - 20.],
            none
        )
        .is_empty()
    );
    // Elevation 3600 → 4200 in one undoable transaction.
    let mut level = upper.clone();
    level.elevation_mm = 4200.;
    level.name = "+4,200".into();
    let depth = b.editor.document.history_depth().0;
    b.editor
        .commit("Изменить уровень", &[Command::ReplaceLevel { level }])
        .unwrap();
    assert_eq!(b.editor.document.history_depth().0, depth + 1);
    assert_eq!(
        b.editor.document.project().levels[&upper.id].elevation_mm,
        4200.
    );
    b.editor.history(false).unwrap();
    assert_eq!(
        b.editor.document.project().levels[&upper.id].elevation_mm,
        3600.
    );
}

#[test]
fn orbit_turns_about_the_point_under_the_cursor() {
    let mut c = Camera::default();
    c.fit(Bounds {
        min: DVec3::new(0., 0., 0.),
        max: DVec3::new(24000., 18000., 3600.),
    });
    let p = |c: &Camera, w: DVec3| {
        Projector {
            camera: c,
            origin: DVec3::ZERO,
            min: [0., 0.],
            size: [1200., 800.],
        }
        .screen(w)
        .unwrap()
    };
    let pivot = DVec3::new(6000., 6000., 3600.);
    let before = p(&c, pivot);
    let distance = c.distance;
    c.orbit_about(pivot, 120., -40.);
    let after = p(&c, pivot);
    assert!(
        (before[0] - after[0]).hypot(before[1] - after[1]) < 1e-6,
        "pivot stays on screen: {before:?} → {after:?}"
    );
    assert!((c.distance - distance).abs() < 1e-9);
    // The pivot is the surface hit at the start and stays for the gesture.
    let mut o = Orbit::default();
    let hit = DVec3::new(1., 2., 3.);
    assert_eq!(o.pivot(0.0, Some(hit), DVec3::ZERO), hit);
    assert_eq!(o.pivot(0.1, Some(DVec3::X), DVec3::ZERO), hit);
    // A new gesture over empty space: the view centre.
    assert_eq!(o.pivot(0.2 + GESTURE_GAP + 0.1, None, DVec3::Y), DVec3::Y);
    assert!(o.active(10.).is_none());
}

#[test]
fn trackpad_scroll_rotates_and_wheel_zooms() {
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0., 3.),
        phase: egui::TouchPhase::Move,
        modifiers: Default::default(),
    };
    let pad = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(12., -5.),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::SHIFT,
    };
    let zoom = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0., 30.),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::COMMAND,
    };
    let s = split_scroll(&[wheel, pad, zoom]);
    assert_eq!(s.wheel, 120.);
    assert_eq!(s.trackpad, [12., -5.]);
    assert!(s.shift, "Shift + two fingers pans");
}

#[test]
fn display_modes_and_transparency() {
    let d = Display {
        mode: RenderMode::Transparent,
        transparency: 0.5,
        scope: TransparencyScope::Unselected,
    };
    assert_eq!(d.alpha(false), 0.5);
    assert_eq!(d.alpha(true), 1.0, "selection stays opaque");
    let all = Display {
        scope: TransparencyScope::All,
        transparency: 0.95,
        ..d
    };
    assert!((all.alpha(true) - 0.1).abs() < 1e-6, "capped at 90 %");
    assert_eq!(
        Display {
            mode: RenderMode::Shaded,
            ..all
        }
        .alpha(false),
        1.0
    );
    // A box mesh has 12 feature edges (face diagonals are not edges).
    let positions: Vec<DVec3> = (0..8)
        .map(|i| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64))
        .collect();
    let quads = [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ];
    let triangles = quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    let mesh = DisplayMesh {
        positions,
        triangles,
    };
    assert_eq!(feature_edges(&mesh, 25.).len(), 12);
    // Ribbon/shortcut commands switch the mode.
    let mut b = bench();
    b.execute(Cmd::DisplayWireframe);
    assert_eq!(b.display.mode, RenderMode::Wireframe);
    b.execute(Cmd::DisplayTransparent);
    assert_eq!(b.display.mode, RenderMode::Transparent);
    b.execute(Cmd::DisplayShaded);
    assert_eq!(b.display.mode, RenderMode::Shaded);
}

#[test]
fn double_click_target_and_member_length_label_text() {
    use open_bim_desktop::interaction::member_edit::length_text;
    assert_eq!(length_text(6000.), "6000");
    assert_eq!(length_text(6000.25), "6000,3");
    // The grid dialog opened from a grid line edits the existing grid.
    let mut b = bench();
    b.execute(Cmd::GridDialog);
    assert!(b.grid_editing, "existing grid opens for editing");
    assert_eq!(b.grid.as_ref().unwrap().labels_z, "0,000 +3,600");
}
