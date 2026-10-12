use open_bim_commands::{Command, project_hash};
use open_bim_desktop::tools::{Edit, Editor, SnapIndex, SnapMode, ToolKind};
use open_bim_model::{ElementShape, GridAxis, ObjectId, Point3, Project, PropertyValue};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;
fn coords(v: [f64; 3]) -> [String; 3] {
    v.map(|x| x.to_string())
}
fn create(editor: &mut Editor, kind: ToolKind, start: [f64; 3], end: [f64; 3]) -> ObjectId {
    editor.begin(kind);
    editor.draft.start = coords(start);
    editor.draft.end = coords(end);
    editor.create().unwrap();
    *editor.selected.first().unwrap()
}
fn assert_point(actual: Point3, expected: [f64; 3]) {
    assert!(
        DVec3::from_array(actual.0).distance(DVec3::from_array(expected)) <= 0.01,
        "{actual:?} != {expected:?}"
    );
}
#[test]
fn numeric_creation_axes_members_and_plate_save_open() {
    let mut editor = Editor::new(Project::default()).unwrap();
    let axis = create(&mut editor, ToolKind::Axis, [0.0; 3], [6000.0, 0.0, 0.0]);
    let column = create(&mut editor, ToolKind::Column, [0.0; 3], [0.0, 0.0, 3000.0]);
    let beam = create(
        &mut editor,
        ToolKind::Beam,
        [0.0, 0.0, 3000.0],
        [6000.0, 0.0, 3000.0],
    );
    editor.draft.contour = "0,0; 0,400; 300,200; 400,0".into();
    let plate = create(
        &mut editor,
        ToolKind::Plate,
        [-200.0, -200.0, 0.0],
        [0.0; 3],
    );
    let project = editor.document.project();
    assert_eq!(project.axes.len(), 1);
    assert_eq!(project.elements.len(), 3);
    assert_point(project.axes[&axis].end, [6000.0, 0.0, 0.0]);
    for (id, expected) in [(column, 3000.0), (beam, 6000.0)] {
        let ElementShape::Member { length, .. } = &project.elements[&id].shape else {
            panic!("member")
        };
        assert!((length.value() - expected).abs() < 1e-8);
    }
    let ElementShape::Plate {
        contour, thickness, ..
    } = &project.elements[&plate].shape
    else {
        panic!("plate")
    };
    assert!((open_bim_model::polygon_area(contour).unwrap() - 100_000.0).abs() < 1e-8);
    assert_eq!(thickness.value(), 20.0);
    let path = std::env::temp_dir().join(format!("open-bim-flow-{}.obim", std::process::id()));
    let expected = project_hash(project).unwrap();
    editor.save(&path).unwrap();
    editor.replace(Project::default(), "reset").unwrap();
    editor.open(&path).unwrap();
    assert_eq!(project_hash(editor.document.project()).unwrap(), expected);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn snap_known_coordinates_end_mid_axis_intersection_and_hidden() {
    let mut project = Project::default();
    for axis in [
        GridAxis {
            id: ObjectId(8),
            name: "A".into(),
            start: Point3([0.0, 450.75, 500.0]),
            end: Point3([1000.0, 450.75, 500.0]),
        },
        GridAxis {
            id: ObjectId(9),
            name: "1".into(),
            start: Point3([120.125, 0.0, 500.0]),
            end: Point3([120.125, 1000.0, 500.0]),
        },
    ] {
        project.axes.insert(axis.id, axis);
    }
    project.next_id = 10;
    let index = SnapIndex::new(&project);
    let hidden = BTreeSet::new();
    for (mode, query, expected) in [
        (SnapMode::End, [3.0, 451.0, 500.0], [0.0, 450.75, 500.0]),
        (
            SnapMode::Midpoint,
            [498.0, 451.0, 500.0],
            [500.0, 450.75, 500.0],
        ),
        (
            SnapMode::Axis,
            [800.125, 454.0, 500.0],
            [800.125, 450.75, 500.0],
        ),
        (
            SnapMode::Intersection,
            [121.0, 449.0, 500.0],
            [120.125, 450.75, 500.0],
        ),
        (
            SnapMode::All,
            [121.0, 449.0, 500.0],
            [120.125, 450.75, 500.0],
        ),
    ] {
        let hit = index.snap(Point3(query), 10.0, mode, &hidden).unwrap();
        assert_point(hit.point, expected);
    }
    assert!(
        index
            .snap(
                Point3([120.125, 450.75, 500.0]),
                10.0,
                SnapMode::Intersection,
                &BTreeSet::from([ObjectId(8)])
            )
            .is_none()
    );
    project.axes.get_mut(&ObjectId(9)).unwrap().start.0[2] = 502.0;
    project.axes.get_mut(&ObjectId(9)).unwrap().end.0[2] = 502.0;
    assert!(
        SnapIndex::new(&project)
            .snap(
                Point3([120.125, 450.75, 500.0]),
                10.0,
                SnapMode::Intersection,
                &hidden
            )
            .is_none()
    );
}
#[test]
fn all_group_edits_are_atomic_and_reversible() {
    let mut editor = Editor::new(Project::default()).unwrap();
    let a = create(&mut editor, ToolKind::Beam, [0.0; 3], [6000.0, 0.0, 0.0]);
    let b = create(
        &mut editor,
        ToolKind::Column,
        [6000.0, 0.0, 0.0],
        [6000.0, 0.0, 3000.0],
    );
    editor.selected = BTreeSet::from([a, b]);
    for edit in [
        Edit::Move(Point3([120.0, 30.0, -5.0])),
        Edit::Rotate {
            axis: Point3([0.0, 0.0, 1.0]),
            degrees: 90.0,
            pivot: Point3([0.0; 3]),
        },
        Edit::Mirror {
            normal: Point3([1.0, 1.0, 0.0]),
            offset: 750.0,
        },
        Edit::Property {
            key: "Марка".into(),
            value: PropertyValue::Text("К1".into()),
        },
        Edit::Copy(Point3([0.0, 4000.0, 0.0])),
    ] {
        let before = project_hash(editor.document.project()).unwrap();
        let history = editor.document.history_depth();
        editor.edit(&edit).unwrap();
        let after = project_hash(editor.document.project()).unwrap();
        assert_ne!(before, after);
        assert_eq!(editor.document.history_depth().0, history.0 + 1);
        editor.history(false).unwrap();
        assert_eq!(project_hash(editor.document.project()).unwrap(), before);
        editor.history(true).unwrap();
        assert_eq!(project_hash(editor.document.project()).unwrap(), after);
    }
    assert_eq!(editor.document.project().elements.len(), 4);
    assert!(
        editor
            .document
            .project()
            .elements
            .values()
            .all(|e| e.properties["Марка"] == PropertyValue::Text("К1".into()))
    );
    let before = project_hash(editor.document.project()).unwrap();
    editor.selected.insert(ObjectId(999));
    assert!(editor.edit(&Edit::Move(Point3([1.0, 0.0, 0.0]))).is_err());
    assert_eq!(project_hash(editor.document.project()).unwrap(), before);
}
#[test]
fn mirror_matches_reflected_geometry_for_asymmetric_sections_and_plate() {
    let mut editor = Editor::new(Project::default()).unwrap();
    editor.draft.profile = ObjectId(3);
    let angle = create(
        &mut editor,
        ToolKind::Column,
        [100.0, 20.0, 0.0],
        [100.0, 20.0, 3000.0],
    );
    editor.draft.contour = "0,0; 400,0; 300,200; 0,400".into();
    let plate = create(
        &mut editor,
        ToolKind::Plate,
        [100.0, 200.0, 500.0],
        [0.0; 3],
    );
    editor.selected = BTreeSet::from([angle, plate]);
    let old = editor.document.project().clone();
    let n = DVec3::new(1.0, 2.0, 3.0).normalize();
    editor
        .edit(&Edit::Mirror {
            normal: Point3(n.to_array()),
            offset: 500.0,
        })
        .unwrap();
    for id in [angle, plate] {
        let before = &old.elements[&id];
        let after = &editor.document.project().elements[&id];
        let length = match &before.shape {
            ElementShape::Member { length, .. } => length.value(),
            ElementShape::Plate { thickness, .. } => thickness.value(),
        };
        after.shape.frame().validate().unwrap();
        for local in [
            [0.0, 0.0, 0.0],
            [25.0, -15.0, 0.0],
            [70.0, 45.0, length * 0.7],
            [0.0, 0.0, length],
        ] {
            let p = DVec3::from_array(before.shape.frame().world(Point3(local)).0);
            let expected = p - 2.0 * (n.dot(p) - 500.0) * n;
            let actual = after
                .shape
                .frame()
                .world(Point3([local[0], local[1], length - local[2]]));
            assert_point(actual, expected.to_array());
        }
    }
}
#[test]
fn invalid_and_cancelled_tools_preserve_document_and_redo() {
    let mut editor = Editor::new(Project::default()).unwrap();
    create(&mut editor, ToolKind::Beam, [0.0; 3], [6000.0, 0.0, 0.0]);
    editor.history(false).unwrap();
    let before = project_hash(editor.document.project()).unwrap();
    let history = editor.document.history_depth();
    editor.begin(ToolKind::Beam);
    editor.draft.end[0] = "NaN".into();
    assert!(editor.create().is_err());
    assert_eq!(project_hash(editor.document.project()).unwrap(), before);
    assert_eq!(editor.document.history_depth(), history);
    editor.begin(ToolKind::Plate);
    editor.draft.thickness = "-20".into();
    assert!(editor.create().is_err());
    editor.draft.thickness = "20".into();
    editor.draft.contour = "0,0;100,100;0,100;100,0".into();
    assert!(editor.create().is_err());
    editor.accept_point(Point3([1.0, 2.0, 0.0]));
    editor.cancel();
    assert!(!editor.active && editor.picked.is_empty());
    assert!(editor.create().is_err());
    assert_eq!(project_hash(editor.document.project()).unwrap(), before);
    assert_eq!(editor.document.history_depth(), history);
    editor.history(true).unwrap();
}
#[test]
fn axis_creation_and_transform_participate_in_document_history() {
    let mut editor = Editor::new(Project::default()).unwrap();
    let axis = create(&mut editor, ToolKind::Axis, [0.0; 3], [1000.0, 0.0, 0.0]);
    let original = project_hash(editor.document.project()).unwrap();
    editor
        .edit(&Edit::Rotate {
            axis: Point3([0.0, 0.0, 1.0]),
            degrees: 90.0,
            pivot: Point3([0.0; 3]),
        })
        .unwrap();
    assert_point(
        editor.document.project().axes[&axis].end,
        [0.0, 1000.0, 0.0],
    );
    editor.history(false).unwrap();
    assert_eq!(project_hash(editor.document.project()).unwrap(), original);
    let duplicate = editor.document.project().axes[&axis].clone();
    assert!(
        editor
            .commit("duplicate", &[Command::CreateAxis { axis: duplicate }])
            .is_err()
    );
    assert_eq!(project_hash(editor.document.project()).unwrap(), original);
}

#[test]
fn gusset_and_stiffener_shapes_edit_atomically_and_persist() {
    let mut editor = Editor::new(Project::default()).unwrap();
    for (kind, role) in [
        (ToolKind::Gusset, "gusset"),
        (ToolKind::Stiffener, "stiffener"),
    ] {
        let id = create(&mut editor, kind, [0.; 3], [0.; 3]);
        let before = project_hash(editor.document.project()).unwrap();
        let element = &editor.document.project().elements[&id];
        assert_eq!(
            element.properties["fabrication_role"],
            PropertyValue::Text(role.into())
        );
        editor
            .edit(&Edit::PlateShape {
                contour: vec![[0., 0.], [120., 0.], [0., 80.]],
                thickness: 15.,
            })
            .unwrap();
        assert!((editor.document.project().element_volume_mm3(id).unwrap() - 72000.).abs() < 1e-6);
        let after = project_hash(editor.document.project()).unwrap();
        editor.history(false).unwrap();
        assert_eq!(project_hash(editor.document.project()).unwrap(), before);
        editor.history(true).unwrap();
        assert_eq!(project_hash(editor.document.project()).unwrap(), after);
        assert!(
            editor
                .edit(&Edit::PlateShape {
                    contour: vec![[0., 0.], [1., 0.], [0., 1.]],
                    thickness: f64::NAN
                })
                .is_err()
        );
        assert_eq!(project_hash(editor.document.project()).unwrap(), after);
        let bytes = open_bim_persistence::encode(editor.document.project()).unwrap();
        assert_eq!(
            open_bim_persistence::decode(&bytes).unwrap(),
            *editor.document.project()
        );
    }
}

#[test]
fn linked_bolts_and_weld_follow_whole_group_rigid_edits() {
    use open_bim_commands::Document;
    use open_bim_desktop::tools::{Draft, edit_commands};
    use open_bim_model::{Detail, DetailKind, Frame, WeldType};
    let mut doc = Document::new(Project::default()).unwrap();
    for z in [0., 20.] {
        let draft = Draft {
            kind: ToolKind::Plate,
            start: coords([0., 0., z]),
            ..Draft::default()
        };
        doc.transact("Plate", &draft.commands(doc.project()).unwrap())
            .unwrap();
    }
    let parts = BTreeSet::from([ObjectId(8), ObjectId(9)]);
    let mut commands = Vec::new();
    for (id, part, z) in [(10, 8, -1.), (11, 9, -21.)] {
        commands.push(Command::CreateDetail {
            detail: Detail {
                id: ObjectId(id),
                kind: DetailKind::Hole {
                    part: ObjectId(part),
                    frame: Frame {
                        origin: Point3([50., 50., z]),
                        ..Frame::default()
                    },
                    diameter: 22.,
                    depth: 42.,
                },
            },
        });
    }
    commands.push(Command::CreateDetail {
        detail: Detail {
            id: ObjectId(12),
            kind: DetailKind::BoltGroup {
                parts: parts.clone(),
                holes: BTreeSet::from([ObjectId(10), ObjectId(11)]),
                frame: Frame {
                    origin: Point3([50., 50., -1.]),
                    ..Frame::default()
                },
                diameter: 20.,
                clearance: 2.,
                steps_x: vec![],
                steps_y: vec![],
                standard: "ISO 4014".into(),
                grade: "8.8".into(),
            },
        },
    });
    commands.push(Command::CreateDetail {
        detail: Detail {
            id: ObjectId(13),
            kind: DetailKind::Weld {
                parts: parts.clone(),
                weld_type: WeldType::Fillet,
                size: 6.,
                path: vec![Point3([0., 0., 20.]), Point3([100., 0., 20.])],
                shop: true,
            },
        },
    });
    doc.transact("Connection", &commands).unwrap();
    let original = project_hash(doc.project()).unwrap();
    assert!(
        edit_commands(
            doc.project(),
            &BTreeSet::from([ObjectId(8)]),
            &Edit::Move(Point3([10., 0., 0.]))
        )
        .is_err()
    );
    for edit in [
        Edit::Move(Point3([100., 200., 300.])),
        Edit::Rotate {
            axis: Point3([0., 0., 1.]),
            degrees: 90.,
            pivot: Point3([0.; 3]),
        },
    ] {
        let commands = edit_commands(doc.project(), &parts, &edit).unwrap();
        doc.transact("Transform all", &commands).unwrap();
        doc.project().validate().unwrap();
        let DetailKind::BoltGroup { frame, .. } = &doc.project().details[&ObjectId(12)].kind else {
            unreachable!()
        };
        if matches!(edit, Edit::Move(_)) {
            assert_point(frame.origin, [150., 250., 299.]);
        } else {
            assert_point(frame.origin, [-250., 150., 299.]);
        }
    }
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), original);
}
