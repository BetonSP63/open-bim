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
    // Footing: sole 1500x1500 from -1800 to -1500 with a 900x900
    // pedestal up to -150, one cast-in-place unit.
    b.execute(Cmd::ConcreteFooting);
    let r = b.pick(DVec3::new(0., 0., 0.));
    let ids = commit(&mut b, r);
    let (footing, pedestal) = (ids[0], ids[1]);
    let [lo, hi] = bounds(&b, footing);
    assert!(
        near(lo, [-750., -750., -1800.]) && near(hi, [750., 750., -1500.]),
        "{lo:?} {hi:?}"
    );
    let [lo, hi] = bounds(&b, pedestal);
    assert!(
        near(lo, [-450., -450., -1500.]) && near(hi, [450., 450., -150.]),
        "{lo:?} {hi:?}"
    );
    let p = b.editor.document.project();
    assert_eq!(
        p.elements[&pedestal].properties[concrete::KEY],
        PropertyValue::Text("pedestal".into())
    );
    let unit = p
        .assemblies
        .values()
        .find(|a| a.members.contains(&pedestal))
        .unwrap();
    assert_eq!(unit.members, [footing, pedestal].into());
    assert_eq!(
        unit.properties[concrete::CAST_UNIT],
        PropertyValue::Text(concrete::CAST_IN_PLACE.into())
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

#[test]
fn pour_objects_breaks_and_units() {
    use open_bim_desktop::interaction::pours;
    let mut b = bench();
    // Two touching columns under a slab, one more column apart.
    b.execute(Cmd::ConcreteColumn);
    for x in [0., 6000., 20000.] {
        let r = b.pick(DVec3::new(x, 0., 0.));
        commit(&mut b, r);
    }
    b.execute(Cmd::ConcreteSlab);
    for p in [
        [-200., -200., 3200.],
        [6200., -200., 3200.],
        [6200., 3000., 3200.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let r = b.execute(Cmd::FinishPicks);
    commit(&mut b, r);
    let objects = pours::objects(b.editor.document.project());
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].parts.len(), 3);
    assert_eq!(objects[0].grade, "В25");
    // Columns 0.4×0.4×3 = 0.48 m³ each, the triangular slab 6.4×3.2/2×0.2.
    let slab = 6.4 * 3.2 / 2. * 0.2;
    assert!(
        (objects[0].volume_m3 - (0.96 + slab)).abs() < 1e-6,
        "{}",
        objects[0].volume_m3
    );
    // A pour break between the two columns splits the first object.
    b.execute(Cmd::PourBreak);
    b.pick(DVec3::new(2000., -1000., 0.));
    let r = b.pick(DVec3::new(2000., 4000., 0.));
    commit(&mut b, r);
    assert_eq!(pours::objects(b.editor.document.project()).len(), 3);
    let r = b.execute(Cmd::CalculatePours);
    commit(&mut b, r);
    let units = &b.editor.document.project().concrete.units;
    assert_eq!(units.len(), 3);
    assert_eq!(
        units.iter().map(|u| u.number).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    // Undo the units, then the break.
    b.editor.history(false).unwrap();
    assert!(b.editor.document.project().concrete.units.is_empty());
    b.editor.history(false).unwrap();
    assert!(b.editor.document.project().concrete.breaks.is_empty());
    // Precast cast units are not poured.
    let all: std::collections::BTreeSet<_> = pours::cast_in_place(b.editor.document.project())
        .into_iter()
        .collect();
    let commands = concrete::cast_unit(b.editor.document.project(), &all, true).unwrap();
    b.editor.commit("Сборный", &commands).unwrap();
    assert!(pours::objects(b.editor.document.project()).is_empty());
    assert!(b.execute(Cmd::CalculatePours).is_none());
}

#[test]
fn concrete_classes_from_the_catalogue() {
    let classes = concrete::grades();
    assert_eq!(classes.first(), Some(&"В15"));
    assert_eq!(classes.last(), Some(&"В60"));
    let mut b = bench();
    b.execute(Cmd::ConcreteColumn);
    b.editor.draft.grade = "В60".into();
    let r = b.pick(DVec3::new(0., 0., 0.));
    let col = commit(&mut b, r)[0];
    let p = b.editor.document.project();
    let m = &p.materials[&p.elements[&col].material];
    assert_eq!((m.name.as_str(), m.density_kg_m3), ("Бетон В60", 2500.));
    assert_eq!(
        open_bim_model::library::concrete_of(&m.name).unwrap().rb,
        33.
    );
    b.editor.draft.grade = "В70".into();
    assert!(b.pick(DVec3::new(6000., 0., 0.)).is_none());
}

#[test]
fn strip_footing_raft_and_precast_parts() {
    let mut b = bench();
    // Strip: 600x500 with the top on the picked line.
    b.execute(Cmd::ConcreteStripFooting);
    b.pick(DVec3::new(0., 0., -1000.));
    let r = b.pick(DVec3::new(6000., 0., -1000.));
    let strip = commit(&mut b, r)[0];
    let [lo, hi] = bounds(&b, strip);
    assert!(
        near(lo, [0., -300., -1500.]) && near(hi, [6000., 300., -1000.]),
        "{lo:?} {hi:?}"
    );
    let p = b.editor.document.project();
    assert_eq!(
        p.elements[&strip].properties[concrete::KEY],
        PropertyValue::Text("strip_footing".into())
    );
    assert!(
        !p.assemblies
            .values()
            .any(|a| a.properties.contains_key(concrete::CAST_UNIT))
    );
    // Raft: 400 thick below the picked plane.
    b.execute(Cmd::ConcreteRaft);
    for p in [
        [0., 0., -1000.],
        [6000., 0., -1000.],
        [6000., 6000., -1000.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let r = b.execute(Cmd::FinishPicks);
    let raft = commit(&mut b, r)[0];
    let [lo, hi] = bounds(&b, raft);
    assert!((hi[2] + 1000.).abs() < 1e-6 && (lo[2] + 1400.).abs() < 1e-6);
    assert_eq!(
        b.editor.document.project().elements[&raft].properties[concrete::KEY],
        PropertyValue::Text("raft".into())
    );
    // A precast pad without a pedestal is its own precast cast unit.
    b.execute(Cmd::ConcreteFooting);
    b.editor.draft.pedestal.clear();
    b.editor.draft.precast = true;
    let r = b.pick(DVec3::new(0., 12000., 0.));
    let ids = commit(&mut b, r);
    assert_eq!(ids.len(), 1);
    let p = b.editor.document.project();
    let unit = p
        .assemblies
        .values()
        .find(|a| a.members.contains(&ids[0]))
        .unwrap();
    assert_eq!(
        unit.properties[concrete::CAST_UNIT],
        PropertyValue::Text(concrete::PRECAST.into())
    );
    // The pedestal top must be above the sole.
    b.editor.draft.pedestal = "600x600".into();
    b.editor.draft.pedestal_top = "-1600".into();
    assert!(b.pick(DVec3::new(6000., 12000., 0.)).is_none());
    // «Сборный ЖБ элемент» from the selection.
    b.editor.selected = [strip].into();
    let r = b.execute(Cmd::PrecastUnit);
    commit(&mut b, r);
    let p = b.editor.document.project();
    let unit = p
        .assemblies
        .values()
        .find(|a| a.members.contains(&strip))
        .unwrap();
    assert_eq!(
        unit.properties[concrete::CAST_UNIT],
        PropertyValue::Text(concrete::PRECAST.into())
    );
}

#[test]
fn openings_and_recesses() {
    use open_bim_desktop::interaction::trim;
    let mut b = bench();
    // Slab 6000x6000x200, top at 3000.
    b.execute(Cmd::ConcreteSlab);
    for p in [
        [0., 0., 3000.],
        [6000., 0., 3000.],
        [6000., 6000., 3000.],
        [0., 6000., 3000.],
    ] {
        b.pick(DVec3::from_array(p));
    }
    let r = b.execute(Cmd::FinishPicks);
    let slab = commit(&mut b, r)[0];
    let volume = |b: &Workbench| {
        b.editor
            .document
            .project()
            .element_volume_mm3(slab)
            .unwrap()
            / 1e9
    };
    assert!((volume(&b) - 7.2).abs() < 1e-6, "{}", volume(&b));
    let square = |x: f64, y: f64, z: f64| {
        [
            [x, y, z],
            [x + 1000., y, z],
            [x + 1000., y + 1000., z],
            [x, y + 1000., z],
        ]
        .map(DVec3::from_array)
    };
    // Opening 1000x1000 through the slab, picked on its top face.
    b.editor.selected = [slab].into();
    b.execute(Cmd::ConcreteOpening);
    for p in square(1000., 1000., 3000.) {
        b.pick(p);
    }
    let r = b.execute(Cmd::FinishPicks);
    commit(&mut b, r);
    assert!((volume(&b) - 7.0).abs() < 1e-6, "{}", volume(&b));
    // Recess 50 deep from the top face.
    b.editor.selected = [slab].into();
    b.execute(Cmd::ConcreteRecess);
    b.tools.detailing.recess_depth = "50".into();
    for p in square(3000., 3000., 3000.) {
        b.pick(p);
    }
    let r = b.execute(Cmd::FinishPicks);
    commit(&mut b, r);
    assert!((volume(&b) - 6.95).abs() < 1e-6, "{}", volume(&b));
    // ...and 100 deep from the bottom face, picked below the slab.
    let p = b.editor.document.project();
    let commands = trim::opening(p, slab, &square(4500., 1000., 2700.), Some(100.)).unwrap();
    b.editor.commit("Ниша", &commands).unwrap();
    assert!((volume(&b) - 6.85).abs() < 1e-6, "{}", volume(&b));
    // Too deep, steel parts and too few points are refused.
    let p = b.editor.document.project();
    assert!(trim::opening(p, slab, &square(0., 0., 3000.), Some(200.)).is_err());
    assert!(trim::opening(p, slab, &square(0., 0., 3000.)[..2], None).is_err());
    // A door in a wall 200 thick: the opening runs across the thickness.
    b.execute(Cmd::ConcretePanel);
    b.pick(DVec3::new(0., 10000., 0.));
    let r = b.pick(DVec3::new(6000., 10000., 0.));
    let wall = commit(&mut b, r)[0];
    let door = [
        [1000., 10100., 0.],
        [2000., 10100., 0.],
        [2000., 10100., 2100.],
        [1000., 10100., 2100.],
    ]
    .map(DVec3::from_array);
    let p = b.editor.document.project();
    let commands = trim::opening(p, wall, &door, None).unwrap();
    b.editor.commit("Проём", &commands).unwrap();
    let v = b
        .editor
        .document
        .project()
        .element_volume_mm3(wall)
        .unwrap()
        / 1e9;
    assert!((v - (6. * 3. - 1. * 2.1) * 0.2).abs() < 1e-6, "{v}");
    let steel = open_bim_model::example_steel_frame().unwrap();
    let id = *steel.elements.keys().next().unwrap();
    assert!(trim::opening(&steel, id, &square(0., 0., 0.), None).is_err());
}
