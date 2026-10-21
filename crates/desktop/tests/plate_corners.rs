//! «Фаски углов» of a plate: straight cuts by two legs and roundings by a
//! radius at chosen corners.
use open_bim_desktop::interaction::corners::{
    Corner, CornerKind, CornerUi, chamfer, parse_corners,
};
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::Editor;
use open_bim_model::{Element, ElementShape, Frame, LengthMm, ObjectId, Project, polygon_area};

const SQUARE: [[f64; 2]; 4] = [[0., 0.], [400., 0.], [400., 300.], [0., 300.]];

#[test]
fn straight_cut_and_rounding_change_the_area() {
    let line = Corner {
        kind: CornerKind::Line,
        x: 50.,
        y: 30.,
    };
    let c = chamfer(&SQUARE, &[0], line).unwrap();
    // X along the side to the previous point (0,300), Y to the next (400,0).
    assert_eq!(c[0], [0., 50.]);
    assert_eq!(c[1], [30., 0.]);
    assert_eq!(c.len(), 5);
    let area = polygon_area(&c).unwrap().abs();
    assert!((area - (400. * 300. - 50. * 30. / 2.)).abs() < 1e-6);

    let round = Corner {
        kind: CornerKind::Round,
        x: 40.,
        y: 40.,
    };
    let all = parse_corners("все", 4).unwrap();
    let c = chamfer(&SQUARE, &all, round).unwrap();
    // Each corner loses r² − πr²/4; the arc is a polygon, so a little less.
    let lost = 4. * (40. * 40. - std::f64::consts::PI * 40. * 40. / 4.);
    let area = polygon_area(&c).unwrap().abs();
    // 15° chords lose about 14 mm² per corner.
    assert!((400. * 300. - lost - area).abs() < 60., "{area}");
    assert!(c.iter().all(|p| p[0] >= -1e-9 && p[0] <= 400. + 1e-9));
}

#[test]
fn cuts_meeting_on_a_side_and_too_long_cuts() {
    // 150 + 150 on the 300 mm side: the cuts meet in one point.
    let c = chamfer(
        &SQUARE,
        &[1, 2],
        Corner {
            kind: CornerKind::Line,
            x: 150.,
            y: 150.,
        },
    )
    .unwrap();
    assert_eq!(c.iter().filter(|p| **p == [400., 150.]).count(), 1);
    assert_eq!(c.len(), 5);
    assert!(polygon_area(&c).is_ok());
    let err = chamfer(
        &SQUARE,
        &[0, 1],
        Corner {
            kind: CornerKind::Line,
            x: 250.,
            y: 250.,
        },
    )
    .unwrap_err();
    assert!(err.contains("длиннее стороны 1–2"), "{err}");
    assert_eq!(parse_corners("1, 3", 4).unwrap(), [0, 2]);
    assert!(parse_corners("5", 4).is_err());
}

#[test]
fn corners_of_a_selected_plate_with_undo() {
    let project = with_default_grid(Project::default()).unwrap();
    let mut editor = Editor::new(project).unwrap();
    let id = ObjectId(editor.document.project().next_id);
    let element = Element {
        id,
        name: "Пластина".into(),
        material: ObjectId(1),
        shape: ElementShape::Plate {
            contour: SQUARE.to_vec(),
            thickness: LengthMm::new(20.).unwrap(),
            frame: Frame::default(),
        },
        properties: Default::default(),
    };
    editor
        .commit(
            "Пластина",
            &[open_bim_commands::Command::Create { element }],
        )
        .unwrap();
    let ui = CornerUi {
        kind: CornerKind::Line,
        corners: "1 2".into(),
        x: "20".into(),
        y: "20".into(),
    };
    let before = editor.document.project().element_volume_mm3(id).unwrap();
    let commands = ui.commands(editor.document.project(), &[id]).unwrap();
    editor.commit("Фаски углов", &commands).unwrap();
    let after = editor.document.project().element_volume_mm3(id).unwrap();
    assert!((before - after - 2. * 200. * 20.).abs() < 1e-3);
    editor.document.undo().unwrap();
    assert_eq!(
        editor.document.project().element_volume_mm3(id).unwrap(),
        before
    );
    // A zero leg is refused.
    let wrong = CornerUi {
        x: "0".into(),
        ..ui
    };
    assert!(wrong.commands(editor.document.project(), &[id]).is_err());
}
