//! Slab area reinforcement from the «Армирование» window: a slab type lays
//! meshes, U-bars and frogs into a new slab, and a shape type updates its bar.
use open_bim_commands::Command;
use open_bim_desktop::interaction::rebar::{self, RebarSettings};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::concrete::{MeshLayer, REBAR_COVER, SlabRebarType};
use open_bim_model::{ObjectId, PropertyValue, RebarKind};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(Editor::new(open_bim_model::Project::default()).unwrap())
}

fn commit(b: &mut Workbench, r: Option<Request>) -> Vec<ObjectId> {
    match r {
        Some(Request::Action(Action::Commit(label, commands))) => {
            let ids = commands
                .iter()
                .filter_map(|c| match c {
                    Command::Create { element } => Some(element.id),
                    _ => None,
                })
                .collect();
            b.editor.commit(&label, &commands).unwrap();
            ids
        }
        _ => panic!("no commit: {:?}", b.tools.message),
    }
}

fn slab_type() -> SlabRebarType {
    let mut ty = SlabRebarType::new("slab-mesh", "Перекрытие с сетками");
    ty.bottom = Some(MeshLayer::new(12., 200., 0.));
    ty.top = Some(MeshLayer::new(12., 200., 0.));
    ty.default_for_new = true;
    ty.edge_diameter = 12.;
    ty.frog_diameter = 8.;
    ty.frog_spacing = 600.;
    ty
}

#[test]
fn a_new_slab_of_the_default_type_is_reinforced() {
    let mut b = bench();
    let commands = rebar::save_slab_type(b.editor.document.project(), slab_type()).unwrap();
    b.editor.commit("Тип плиты", &commands).unwrap();
    b.execute(Cmd::ConcreteSlab);
    for p in [
        [0., 0., 3000.],
        [6000., 0., 3000.],
        [6000., 4000., 3000.],
        [0., 4000., 3000.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let finish = b.execute(Cmd::FinishPicks);
    let slab = commit(&mut b, finish)[0];
    let p = b.editor.document.project();
    assert_eq!(
        p.elements[&slab].properties.get(REBAR_COVER),
        Some(&PropertyValue::Number(30.))
    );
    let zone = p.concrete.zones.iter().find(|z| z.host == slab).unwrap();
    assert_eq!(zone.mark, "1, 2, 3, 4, 5, 6");
    assert!(
        p.concrete
            .rebars
            .iter()
            .any(|r| r.role == Some(open_bim_model::concrete::ZoneRole::BottomMain))
    );
    assert!(p.concrete.rebars.iter().any(|r| r.kind == RebarKind::Edge));
    assert!(p.concrete.rebars.iter().any(|r| r.kind == RebarKind::Frog));
    let bounds = p.element_bounds(slab).unwrap();
    for bar in p.concrete.rebars.iter().flat_map(|r| &r.bars) {
        for q in bar {
            assert!((0..3).all(|k| q.0[k] > bounds[0][k] - 1. && q.0[k] < bounds[1][k] + 1.));
        }
    }
    // A thicker slab moves the top mesh. The zone mark stays at the centre.
    let mut element = p.elements[&slab].clone();
    if let open_bim_model::ElementShape::Plate { thickness, .. } = &mut element.shape {
        *thickness = open_bim_model::LengthMm::new(300.).unwrap();
    }
    b.editor
        .commit("Толщина", &[Command::ReplaceElement { element }])
        .unwrap();
    let p = b.editor.document.project();
    let top = p
        .concrete
        .rebars
        .iter()
        .find(|r| r.role == Some(open_bim_model::concrete::ZoneRole::TopMain))
        .unwrap();
    assert!(
        (top.bars[0][0].0[2] - (bounds[0][2] + 300. - 30. - 6.)).abs() < 1.,
        "{:?}",
        top.bars[0][0]
    );
    assert!((p.concrete.zones[0].mark_at.0[2] - (bounds[0][2] + 150.)).abs() < 1.);
}

#[test]
fn reinforce_and_a_shape_bar_follow_later_edits() {
    let mut b = bench();
    b.execute(Cmd::ConcreteSlab);
    for p in [
        [0., 0., 0.],
        [4000., 0., 0.],
        [4000., 3000., 0.],
        [0., 3000., 0.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let finish = b.execute(Cmd::FinishPicks);
    let slab = commit(&mut b, finish)[0];
    let mut ty = slab_type();
    ty.default_for_new = false;
    let commands = rebar::save_slab_type(b.editor.document.project(), ty).unwrap();
    b.editor.commit("Тип плиты", &commands).unwrap();
    let commands = rebar::reinforce(b.editor.document.project(), &[slab], "slab-mesh").unwrap();
    b.editor.commit("Раскладка", &commands).unwrap();
    assert!(
        b.editor
            .document
            .project()
            .concrete
            .zones
            .iter()
            .any(|z| z.host == slab)
    );
    // Shape bar at the slab centre, then a longer leg on the type.
    let shape = open_bim_model::rebar::builtin_shapes()
        .into_iter()
        .find(|s| s.id == "gost.l")
        .unwrap();
    let commands = rebar::place_shape(b.editor.document.project(), &[slab], shape.clone()).unwrap();
    b.editor.commit("Стержень", &commands).unwrap();
    let leg = |b: &Workbench| {
        let bar = b
            .editor
            .document
            .project()
            .concrete
            .rebars
            .iter()
            .find(|r| r.kind == RebarKind::Shape)
            .unwrap();
        let (a, c) = (&bar.bars[0][0], &bar.bars[0][1]);
        (0..3)
            .map(|k| (c.0[k] - a.0[k]).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    assert!((leg(&b) - 1000.).abs() < 1., "{}", leg(&b));
    let mut edited = shape;
    edited.segments[0].length = 800.;
    let commands = rebar::save_shape(b.editor.document.project(), edited).unwrap();
    b.editor.commit("Правка формы", &commands).unwrap();
    assert!((leg(&b) - 800.).abs() < 1., "{}", leg(&b));
    let _ = RebarSettings::default();
}
