//! «Бетон» tab: concrete columns, beams, panels, slabs and footings placed
//! by picks with a section and grade; «ЖБ элемент» makes a cast unit.
use open_bim_desktop::interaction::concrete;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{ObjectId, ProfileShape, PropertyValue};
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
                    open_bim_commands::Command::Create { element } => Some(element.id),
                    _ => None,
                })
                .collect();
            b.editor.commit(&label, &commands).unwrap();
            ids
        }
        _ => panic!("no commit: {:?}", b.tools.message),
    }
}

fn bounds(b: &Workbench, id: ObjectId) -> [[f64; 3]; 2] {
    b.editor.document.project().element_bounds(id).unwrap()
}

fn near(a: [f64; 3], e: [f64; 3]) -> bool {
    a.iter().zip(e).all(|(x, y)| (x - y).abs() < 1e-6)
}

#[test]
fn concrete_parts_by_picks() {
    let mut b = bench();
    // Column: one pick, 400x400 from 0 to 3000, grade В25 added once.
    b.execute(Cmd::ConcreteColumn);
    let r = b.pick(DVec3::new(0., 0., 0.));
    let ids = commit(&mut b, r);
    let col = ids[0];
    let p = b.editor.document.project();
    assert_eq!(
        p.elements[&col].properties[concrete::KEY],
        PropertyValue::Text("column".into())
    );
    assert_eq!(p.materials[&p.elements[&col].material].name, "Бетон В25");
    let [lo, hi] = bounds(&b, col);
    assert!(
        near(lo, [-200., -200., 0.]) && near(hi, [200., 200., 3000.]),
        "{lo:?} {hi:?}"
    );
    // A second column reuses the section and the material.
    let (profiles, materials) = (p.profiles.len(), p.materials.len());
    let r = b.pick(DVec3::new(6000., 0., 0.));
    commit(&mut b, r);
    let p = b.editor.document.project();
    assert_eq!((p.profiles.len(), p.materials.len()), (profiles, materials));
    // Beam: top face on the picked line.
    b.execute(Cmd::ConcreteBeam);
    b.pick(DVec3::new(0., 0., 3000.));
    let r = b.pick(DVec3::new(6000., 0., 3000.));
    let beam = commit(&mut b, r)[0];
    let [lo, hi] = bounds(&b, beam);
    assert!(
        (hi[2] - 3000.).abs() < 1e-6 && (lo[2] - 2400.).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // Panel: a wall 200 thick standing on the line.
    b.execute(Cmd::ConcretePanel);
    b.pick(DVec3::new(0., 3000., 0.));
    let r = b.pick(DVec3::new(6000., 3000., 0.));
    let panel = commit(&mut b, r)[0];
    let [lo, hi] = bounds(&b, panel);
    assert!(
        near(lo, [0., 2900., 0.]) && near(hi, [6000., 3100., 3000.]),
        "{lo:?} {hi:?}"
    );
    // Slab: the top face on the picked plane, 200 thick.
    b.execute(Cmd::ConcreteSlab);
    for p in [[0., 0., 3000.], [6000., 0., 3000.], [6000., 6000., 3000.]] {
        assert!(b.pick(DVec3::from_array(p)).is_none());
    }
    let r = b.execute(Cmd::FinishPicks);
    let slab = commit(&mut b, r)[0];
    let [lo, hi] = bounds(&b, slab);
    assert!((hi[2] - 3000.).abs() < 1e-6 && (lo[2] - 2800.).abs() < 1e-6);
    assert!(
        !b.editor.document.project().elements[&slab]
            .properties
            .contains_key("fabrication_role")
    );
    // Footing: 1500x1500, 600 below zero.
    b.execute(Cmd::ConcreteFooting);
    let r = b.pick(DVec3::new(0., 0., 0.));
    let footing = commit(&mut b, r)[0];
    let [lo, hi] = bounds(&b, footing);
    assert!(
        near(lo, [-750., -750., -600.]) && near(hi, [750., 750., 0.]),
        "{lo:?} {hi:?}"
    );
    // Another grade and section from the tool properties.
    b.execute(Cmd::ConcreteColumn);
    b.editor.draft.grade = "В30".into();
    b.editor.draft.section = "500х600".into();
    let r = b.pick(DVec3::new(0., 6000., 0.));
    let c2 = commit(&mut b, r)[0];
    let p = b.editor.document.project();
    let e = &p.elements[&c2];
    assert_eq!(p.materials[&e.material].name, "Бетон В30");
    let open_bim_model::ElementShape::Member { profile, .. } = e.shape else {
        panic!()
    };
    assert!(matches!(
        p.profiles[&profile].shape,
        ProfileShape::Rectangle {
            width: 500.,
            height: 600.
        }
    ));
    // Undo removes the part and the new profile and material together.
    b.editor.history(false).unwrap();
    let p = b.editor.document.project();
    assert!(!p.materials.values().any(|m| m.name == "Бетон В30"));
    // «ЖБ элемент»: the column, beam and footing make a cast unit.
    b.editor.selected = [col, beam, footing].into();
    let r = b.execute(Cmd::CastUnit);
    commit(&mut b, r);
    let p = b.editor.document.project();
    let unit = p
        .assemblies
        .values()
        .find(|a| a.properties.contains_key(concrete::CAST_UNIT))
        .unwrap();
    assert_eq!(unit.members.len(), 3);
    // Steel parts alone are refused.
    let mut s =
        Workbench::new(Editor::new(open_bim_model::example_steel_frame().unwrap()).unwrap());
    s.editor.selected = s
        .editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .collect();
    assert!(s.execute(Cmd::CastUnit).is_none());
}

#[test]
fn section_parsing() {
    assert_eq!(concrete::section("400x600"), Ok((400., 600.)));
    assert_eq!(concrete::section(" 300 × 500 "), Ok((300., 500.)));
    assert!(concrete::section("400").is_err());
    assert!(concrete::section("0x400").is_err());
}
