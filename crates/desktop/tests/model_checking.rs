//! Checking the model: «Запросить объект», «Сравнить детали», «Измерить
//! угол», «Секущая плоскость по точке».
use open_bim_commands::Command;
use open_bim_desktop::interaction::inquire::angle_deg;
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, clip_facing, commands::Cmd};
use open_bim_model::{ObjectId, PropertyValue};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn beam(b: &mut Workbench, y: f64, length: f64) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::new(0., y, 0.));
    let Some(Request::Action(Action::Commit(label, commands))) = b.pick(DVec3::new(length, y, 0.))
    else {
        panic!("beam not created");
    };
    b.editor.commit(&label, &commands).unwrap();
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn row<'a>(b: &'a Workbench, key: &str) -> &'a str {
    let i = b.inquiry.as_ref().unwrap();
    &i.rows
        .iter()
        .find(|r| r[0] == key)
        .unwrap_or_else(|| panic!("{key}: {:?}", i.rows))[1]
}

#[test]
fn inquire_a_beam() {
    let mut b = bench();
    b.execute(Cmd::InquireObject);
    assert!(b.inquiry.is_none());
    let id = beam(&mut b, 0., 3000.);
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::InquireObject);
    assert_eq!(row(&b, "Тип"), "Балка");
    assert_eq!(row(&b, "Длина, мм"), "3000");
    assert!(row(&b, "Сечение").starts_with("30Б1"));
    let mass: f64 = row(&b, "Масса нетто, кг").parse().unwrap();
    let expected = b.editor.document.project().element_mass_kg(id).unwrap();
    assert!((mass - expected).abs() < 0.01);
    let area: f64 = row(&b, "Площадь поверхности, м²").parse().unwrap();
    assert!(area > 1.0 && area < 5.0, "{area}");
    assert!(!row(&b, "Позиция").is_empty());
}

#[test]
fn compare_two_parts() {
    let mut b = bench();
    let first = beam(&mut b, 0., 3000.);
    let second = beam(&mut b, 2000., 3000.);
    b.editor.selected = BTreeSet::from([first, second]);
    b.execute(Cmd::CompareParts);
    let i = b.inquiry.clone().unwrap();
    assert!(i.rows.is_empty());
    assert!(i.note.unwrap().contains("одну позицию"));

    let third = beam(&mut b, 4000., 3500.);
    b.editor
        .commit(
            "Покрытие",
            &[Command::ChangeProperty {
                id: third,
                key: "surface_treatment".into(),
                value: Some(PropertyValue::Text("Горячее цинкование".into())),
            }],
        )
        .unwrap();
    b.editor.selected = BTreeSet::from([first, third]);
    b.execute(Cmd::CompareParts);
    let i = b.inquiry.clone().unwrap();
    let labels: Vec<&str> = i.rows.iter().map(|r| r[0].as_str()).collect();
    assert!(labels.contains(&"Длина, мм"), "{labels:?}");
    assert!(labels.contains(&"Обработка поверхности"), "{labels:?}");
    let treatment = i
        .rows
        .iter()
        .find(|r| r[0] == "Обработка поверхности")
        .unwrap();
    assert_eq!(treatment[1], "—");
    assert_eq!(treatment[2], "Горячее цинкование");

    // One part is not a comparison.
    b.inquiry = None;
    b.editor.selected = BTreeSet::from([first]);
    b.execute(Cmd::CompareParts);
    assert!(b.inquiry.is_none());
}

#[test]
fn measure_an_angle() {
    let mut b = bench();
    b.execute(Cmd::MeasureAngle);
    assert!(b.picking());
    assert!(b.prompt().contains("вершину"));
    b.pick(DVec3::ZERO);
    b.pick(DVec3::new(1000., 0., 0.));
    assert!(b.prompt().contains("второй стороне"));
    b.pick(DVec3::new(1000., 1000., 0.));
    assert!(b.measured_angle.is_some());
    assert!(
        b.tools
            .message
            .clone()
            .unwrap_or_default()
            .contains("45.00°")
    );
    assert_eq!(
        angle_deg(DVec3::ZERO, DVec3::X, DVec3::Y).map(f64::round),
        Some(90.)
    );
    assert!(angle_deg(DVec3::ZERO, DVec3::ZERO, DVec3::Y).is_none());
    b.execute(Cmd::CancelTool);
    assert!(!b.picking() && b.measured_angle.is_none());
}

#[test]
fn clip_plane_through_a_picked_point() {
    let mut b = bench();
    b.execute(Cmd::ClipPlanePick);
    assert!(b.picking());
    let p = DVec3::new(0., 0., 1500.);
    assert!(matches!(b.pick(p), Some(Request::ClipAt(q)) if q == p));
    assert!(!b.picking());
    // Looking down: everything above 1500 is cut away.
    let plane = clip_facing(DVec3::new(0., 0., 10000.), DVec3::ZERO, p);
    assert!(plane.keeps(DVec3::new(0., 0., 1000.)));
    assert!(!plane.keeps(DVec3::new(0., 0., 2000.)));
    b.clip[0] = Some(plane);
    b.execute(Cmd::ClipPlaneRemove);
    assert!(b.clip[0].is_none());
}
