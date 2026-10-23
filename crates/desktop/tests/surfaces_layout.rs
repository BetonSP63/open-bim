//! «Поверхности» (surface treatment with areas) and «Диспетчер разбивок»
//! (setting-out points with CSV).
use open_bim_desktop::interaction::layout_points::{csv, points};
use open_bim_desktop::interaction::surfaces::{assign, selected_area_mm2};
use open_bim_desktop::tools::Editor;
use open_bim_drawings::sample::control_project;
use open_bim_model::{
    Construction, ConstructionKind, GridAxis, Level, ObjectId, Point3, Project, PropertyValue,
};
use open_bim_reports::{SURFACE_TREATMENT, coatings};
use std::collections::BTreeSet;

#[test]
fn surfaces_assign_and_remove_a_treatment_with_its_area() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let both = BTreeSet::from([f.beam, f.plate]);
    let commands = assign(editor.document.project(), &both, Some("Горячее цинкование")).unwrap();
    assert_eq!(commands.len(), 2);
    editor.commit_data("Покрытие", &commands).unwrap();
    let p = editor.document.project();
    assert_eq!(
        p.elements[&f.beam].properties.get(SURFACE_TREATMENT),
        Some(&PropertyValue::Text("Горячее цинкование".into()))
    );
    let rows = coatings(p).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].quantity, 2);
    let area = selected_area_mm2(p, &both);
    assert!(area > 0.);
    assert!((rows[0].area_m2 - area / 1e6).abs() < 1e-3);
    // Assigning the same again changes nothing; removing clears it.
    assert!(
        assign(p, &both, Some("Горячее цинкование"))
            .unwrap()
            .is_empty()
    );
    let commands = assign(p, &BTreeSet::from([f.plate]), None).unwrap();
    editor.commit_data("Снять", &commands).unwrap();
    let rows = coatings(editor.document.project()).unwrap();
    assert_eq!(rows[0].quantity, 1);
    assert!(assign(editor.document.project(), &BTreeSet::new(), None).is_err());
}

#[test]
fn layout_points_list_construction_points_and_grid_nodes() {
    let mut project = Project {
        origin: Point3([100., 0., 0.]),
        ..Default::default()
    };
    let axis = |id, name: &str, start, end| GridAxis {
        id: ObjectId(id),
        name: name.into(),
        start: Point3(start),
        end: Point3(end),
    };
    for a in [
        axis(101, "1", [0., -1000., 0.], [0., 9000., 0.]),
        axis(102, "2", [6000., -1000., 0.], [6000., 9000., 0.]),
        axis(103, "А", [-1000., 0., 0.], [7000., 0., 0.]),
        axis(104, "Б", [-1000., 8000., 0.], [7000., 8000., 0.]),
    ] {
        project.axes.insert(a.id, a);
    }
    let level = Level {
        id: ObjectId(110),
        name: "+3,000".into(),
        elevation_mm: 3000.,
    };
    project.levels.insert(level.id, level);
    project.construction.insert(
        ObjectId(120),
        Construction {
            id: ObjectId(120),
            kind: ConstructionKind::Point {
                at: Point3([1100., 200., 0.]),
            },
        },
    );
    let rows = points(&project, Some(ObjectId(110)));
    // One point and four nodes; parallel axes do not meet.
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].name, "Т1");
    assert_eq!(rows[0].at, [1000., 200., 0.]);
    assert_eq!(rows[0].id, Some(ObjectId(120)));
    let node = rows.iter().find(|p| p.name == "Б/2").unwrap();
    assert_eq!(node.at, [5900., 8000., 3000.]);
    assert!(rows.iter().any(|p| p.name == "А/1"));
    let text = csv(&rows);
    assert!(text.starts_with("\u{feff}Точка;X, мм;Y, мм;Z, мм\r\n"));
    assert!(text.contains("Б/2;5900,0;8000,0;3000,0\r\n"));
}
