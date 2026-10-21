//! «Контрольные номера» of assemblies and weld numbers with the weld list.
use open_bim_commands::Command;
use open_bim_desktop::interaction::control_numbers::{
    CONTROL_NUMBER, ControlNumbers, Order, assemblies_of, clear,
};
use open_bim_desktop::interaction::inquire::inquire;
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project, PropertyValue,
};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

fn bench() -> Workbench {
    let project = with_default_grid(Project::default()).unwrap();
    Workbench::new(Editor::new(project).unwrap())
}

fn p(b: &Workbench) -> &Project {
    b.editor.document.project()
}

fn plate(b: &mut Workbench, x: f64, z: f64) -> ObjectId {
    let id = ObjectId(p(b).next_id);
    let element = Element {
        id,
        name: "Пластина".into(),
        material: ObjectId(1),
        shape: ElementShape::Plate {
            contour: vec![[0., 0.], [400., 0.], [400., 400.], [0., 400.]],
            thickness: LengthMm::new(20.).unwrap(),
            frame: Frame {
                origin: Point3([x, 0., z]),
                ..Frame::default()
            },
        },
        properties: Default::default(),
    };
    b.editor
        .commit("Пластина", &[Command::Create { element }])
        .unwrap();
    id
}

/// Two plates welded in the shop: one assembly, one weld.
fn welded_pair(b: &mut Workbench, x: f64, z: f64) -> ObjectId {
    let (lower, upper) = (plate(b, x, z), plate(b, x, z + 20.));
    b.tools.detailing.shop = true;
    b.execute(Cmd::ToolWeldPolygon);
    b.pick_object(Some(lower));
    b.pick_object(Some(upper));
    for q in [[x, 0.], [x + 400., 0.]] {
        b.pick(DVec3::new(q[0], q[1], z + 20.));
    }
    let Some(Request::Action(Action::Commit(label, commands))) = b.execute(Cmd::FinishPicks) else {
        panic!("no weld: {:?}", b.tools.message);
    };
    b.editor.commit(&label, &commands).unwrap();
    b.execute(Cmd::CancelTool);
    lower
}

fn control(b: &Workbench, part: ObjectId) -> String {
    let a = p(b)
        .assemblies
        .values()
        .find(|a| a.members.contains(&part))
        .unwrap();
    match a.properties.get(CONTROL_NUMBER) {
        Some(PropertyValue::Text(t)) => t.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn control_numbers_follow_the_erection_order() {
    let mut b = bench();
    let high = welded_pair(&mut b, 0., 3000.);
    let right = welded_pair(&mut b, 2000., 0.);
    let left = welded_pair(&mut b, 0., 0.);
    let all = assemblies_of(p(&b), &BTreeSet::new());
    assert_eq!(all.len(), 3);
    let numbers = ControlNumbers::default();
    let commands = numbers.commands(p(&b), &all).unwrap();
    b.editor.commit("Контрольные номера", &commands).unwrap();
    // Bottom to top, then along X.
    assert_eq!(control(&b, left), "М1");
    assert_eq!(control(&b, right), "М2");
    assert_eq!(control(&b, high), "М3");
    // Along X first, prefix and step of 10, only the selected part's assembly.
    let numbers = ControlNumbers {
        prefix: "К-".into(),
        start: "10".into(),
        step: "10".into(),
        order: Order::Xyz,
    };
    let commands = numbers.commands(p(&b), &all).unwrap();
    b.editor.commit("Контрольные номера", &commands).unwrap();
    assert_eq!(control(&b, left), "К-10");
    assert_eq!(control(&b, high), "К-20");
    assert_eq!(control(&b, right), "К-30");
    let one = assemblies_of(p(&b), &BTreeSet::from([right]));
    assert_eq!(one.len(), 1);
    let rows = inquire(p(&b), *one.first().unwrap()).unwrap();
    assert!(
        rows.contains(&("Контрольный номер".into(), "К-30".into())),
        "{rows:?}"
    );
    b.editor.commit("Сбросить", &clear(p(&b), &one)).unwrap();
    assert!(p(&b).assemblies[one.first().unwrap()].properties.is_empty());
    // Bad step and no assemblies are refused.
    let bad = ControlNumbers {
        step: "0".into(),
        ..Default::default()
    };
    assert!(bad.commands(p(&b), &all).is_err());
    assert!(numbers.commands(p(&b), &BTreeSet::new()).is_err());
}

#[test]
fn weld_numbers_are_kept_and_listed() {
    let mut b = bench();
    welded_pair(&mut b, 0., 3000.);
    welded_pair(&mut b, 2000., 0.);
    let numbering = open_bim_numbering::number(p(&b)).unwrap();
    let weld_at = |b: &Workbench, z: f64| {
        *p(b)
            .details
            .values()
            .find(|d| matches!(&d.kind, DetailKind::Weld { path, .. } if (path[0].0[2] - z).abs() < 1.))
            .map(|d| &d.id)
            .unwrap()
    };
    let (low, high) = (weld_at(&b, 20.), weld_at(&b, 3020.));
    assert_eq!(numbering.welds[&low], "1");
    assert_eq!(numbering.welds[&high], "2");
    // Not stored yet: no weld list.
    assert!(open_bim_reports::generate(p(&b)).unwrap().welds.is_empty());
    let marks = open_bim_numbering::stored_marks(&numbering);
    b.editor
        .commit("Нумерация", &[Command::SetNumbering { marks }])
        .unwrap();
    let r = open_bim_reports::generate(p(&b)).unwrap();
    assert_eq!(r.welds.len(), 2);
    assert_eq!(r.welds[0].number, "1");
    assert!((r.welds[0].length_mm - 400.).abs() < 1e-6);
    assert!(r.welds[0].shop);
    assert!(open_bim_reports::csv::welds(&r).contains("заводской"));
    let rows = inquire(p(&b), low).unwrap();
    assert!(rows.contains(&("Номер шва".into(), "1".into())), "{rows:?}");
    // A new weld lower than both gets the next number; the old ones stay.
    welded_pair(&mut b, 4000., -1000.);
    let numbering = open_bim_numbering::number(p(&b)).unwrap();
    let newest = weld_at(&b, -980.);
    assert_eq!(numbering.welds[&low], "1");
    assert_eq!(numbering.welds[&high], "2");
    assert_eq!(numbering.welds[&newest], "3");
    assert!(open_bim_numbering::outdated(p(&b), &numbering).contains(&newest));
    // «Перенумеровать всё» orders them again from the bottom.
    let fresh = open_bim_numbering::renumber_all(p(&b)).unwrap();
    assert_eq!(fresh.welds[&newest], "1");
}
