//! Bolts, welds, cuts and holes of families become model details of the
//! placed object.
use open_bim_commands::*;
use open_bim_model::*;
use std::collections::BTreeMap;

fn s(v: &str) -> String {
    v.to_string()
}

fn sample(name: &str) -> Family {
    sample_families()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap()
}

fn instance(id: ObjectId, family: &str, type_name: &str) -> FamilyInstance {
    FamilyInstance {
        id,
        family: family.into(),
        type_name: type_name.into(),
        frame: Frame {
            origin: Point3([1000., 0., 3000.]),
            ..Frame::default()
        },
        length: 0.,
        values: BTreeMap::new(),
        children: BTreeMap::new(),
    }
}

fn place(family: Family, type_name: &str) -> (Document, ObjectId) {
    let mut document = Document::new(Project::default()).unwrap();
    let id = ObjectId(document.project().next_id + 100);
    let name = family.name.clone();
    document
        .transact(
            "Семейство",
            &[
                Command::SetFamily {
                    family: Box::new(family),
                },
                Command::CreateFamilyInstance {
                    instance: instance(id, &name, type_name),
                },
            ],
        )
        .unwrap();
    (document, id)
}

fn details<'a>(p: &'a Project, id: ObjectId, prefix: &str) -> Vec<&'a Detail> {
    p.family_instances[&id]
        .children
        .iter()
        .filter(|(role, _)| role.starts_with(prefix))
        .map(|(_, d)| &p.details[d])
        .collect()
}

/// Two plates face to face, bolted 2 × 2, the front one with a cut corner
/// and a hole.
fn flange() -> Family {
    let plate = |name: &str, at: &str| FamilyBody {
        name: s(name),
        material: s("С255"),
        visible: String::new(),
        array: None,
        shape: BodyShape::Plate {
            normal: Axis::Y,
            at: s(at),
            contour: vec![
                [s("0"), s("0")],
                [s("B"), s("0")],
                [s("B"), s("B")],
                [s("0"), s("B")],
            ],
            thickness: s("10"),
        },
    };
    let mut f = sample("Столик опорный");
    f.name = s("Фланец");
    f.types.clear();
    f.planes.clear();
    f.parameters = vec![FamilyParameter {
        name: s("B"),
        label: String::new(),
        kind: ParamKind::Length,
        group: String::new(),
        of_type: false,
        value: s("200"),
        formula: String::new(),
        min: None,
        max: None,
    }];
    f.bodies = vec![plate("Фланец 1", "0"), plate("Фланец 2", "10")];
    f.details = vec![
        FamilyDetail {
            name: s("Болты"),
            bodies: vec![s("Фланец 1"), s("Фланец 2")],
            visible: String::new(),
            array: None,
            shape: DetailShape::Bolts {
                at: [s("50"), s("0"), s("50")],
                normal: Axis::Y,
                columns: s("2"),
                column_step: s("B - 100"),
                rows: s("2"),
                row_step: s("B - 100"),
                diameter: s("20"),
                clearance: String::new(),
                standard: s("ГОСТ 7798-70"),
                grade: s("5.8"),
            },
        },
        FamilyDetail {
            name: s("Срез угла"),
            bodies: vec![s("Фланец 2")],
            visible: String::new(),
            array: None,
            shape: DetailShape::Cut {
                normal: Axis::Y,
                at: s("0"),
                contour: vec![
                    [s("B - 30"), s("B + 1")],
                    [s("B + 1"), s("B + 1")],
                    [s("B + 1"), s("B - 30")],
                ],
                depth: s("30"),
            },
        },
        FamilyDetail {
            name: s("Центр"),
            bodies: vec![s("Фланец 2")],
            visible: String::new(),
            array: None,
            shape: DetailShape::Hole {
                at: [s("B / 2"), s("-1"), s("B / 2")],
                normal: Axis::Y,
                diameter: s("40"),
                depth: s("30"),
            },
        },
    ];
    f
}

#[test]
fn the_seat_welds_its_ribs_and_drills_the_holes() {
    let (mut document, id) = place(sample("Столик опорный"), "С-2");
    let p = document.project();
    let children = &p.family_instances[&id].children;
    let welds = details(p, id, "detail-0-");
    assert_eq!(welds.len(), 2);
    for (k, weld) in welds.iter().enumerate() {
        let DetailKind::Weld {
            parts, path, sides, ..
        } = &weld.kind
        else {
            panic!()
        };
        // Each weld joins the plate and its own rib.
        let rib = children[&format!("body-1-{k}")];
        assert_eq!(parts.len(), 2);
        assert!(parts.contains(&children["body-0-0"]) && parts.contains(&rib));
        assert_eq!(*sides, WeldSides::Both);
        assert_eq!(path[0].0[2], 3000. - 16.);
        assert!((path[1].0[1] - path[0].0[1] - 120.).abs() < 1e-9);
    }
    let holes = details(p, id, "detail-1-");
    assert_eq!(holes.len(), 2);
    assert!(holes.iter().all(|h| matches!(
        h.kind,
        DetailKind::Hole { diameter, part, .. } if diameter == 22. && part == children["body-0-0"]
    )));

    // No ribs, no rib welds; no holes when switched off.
    let mut changed = p.family_instances[&id].clone();
    changed.values.insert(s("n"), s("0"));
    changed.values.insert(s("отв"), s("нет"));
    document
        .transact(
            "Без рёбер",
            &[Command::ReplaceFamilyInstance {
                instance: changed.clone(),
            }],
        )
        .unwrap();
    let p = document.project();
    assert!(details(p, id, "detail-").is_empty());
    assert!(p.details.is_empty());
    document.undo().unwrap();
    assert_eq!(document.project().details.len(), 4);
}

#[test]
fn family_details_keep_their_ids_and_cannot_be_edited_directly() {
    let (mut document, id) = place(sample("Столик опорный"), "С-2");
    let before = document.project().family_instances[&id].children.clone();
    let mut changed = document.project().family_instances[&id].clone();
    changed.type_name = s("С-3");
    document
        .transact(
            "С-3",
            &[Command::ReplaceFamilyInstance { instance: changed }],
        )
        .unwrap();
    let p = document.project();
    assert_eq!(p.family_instances[&id].children, before);
    let weld = p.details[&before["detail-0-0"]].clone();
    let DetailKind::Weld { path, .. } = &weld.kind else {
        panic!()
    };
    assert_eq!(path[0].0[2], 3000. - 20.);

    let mut edited = weld.clone();
    if let DetailKind::Weld { size, .. } = &mut edited.kind {
        *size = 10.;
    }
    let refused = document.transact(
        "Шов",
        &[Command::ReplaceDetail {
            detail: edited.clone(),
        }],
    );
    assert!(refused.is_err(), "{refused:?}");

    // Deleting a weld removes the whole object.
    document
        .transact(
            "Удалить шов",
            &[Command::Delete {
                id: weld.id,
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
    let p = document.project();
    assert!(p.family_instances.is_empty());
    assert!(p.details.is_empty() && p.elements.is_empty());
}

#[test]
fn bolts_drill_every_part_they_join_and_cuts_go_into_their_bodies() {
    let (document, id) = place(flange(), "");
    let p = document.project();
    let children = &p.family_instances[&id].children;
    let (a, b) = (children["body-0-0"], children["body-1-0"]);
    let bolts = details(p, id, "detail-0-0");
    let group = bolts
        .iter()
        .find(|d| matches!(d.kind, DetailKind::BoltGroup { .. }))
        .unwrap();
    let DetailKind::BoltGroup {
        parts,
        holes,
        frame,
        diameter,
        clearance,
        steps_x,
        steps_y,
        ..
    } = &group.kind
    else {
        panic!()
    };
    assert_eq!(parts.iter().copied().collect::<Vec<_>>(), {
        let mut v = vec![a, b];
        v.sort();
        v
    });
    assert_eq!(holes.len(), 8);
    assert_eq!((*diameter, *clearance), (20., 2.));
    assert_eq!(
        (steps_x.as_slice(), steps_y.as_slice()),
        (&[100.][..], &[100.][..])
    );
    assert_eq!(frame.origin, Point3([1050., 0., 3050.]));
    // Bolts run along Y.
    assert!((frame.axes[2][1].abs() - 1.).abs() < 1e-9);

    let cut = details(p, id, "detail-1-0");
    assert_eq!(cut.len(), 1);
    assert!(matches!(cut[0].kind, DetailKind::PrismCut { part, .. } if part == b));
    let hole = details(p, id, "detail-2-0");
    assert!(
        matches!(hole[0].kind, DetailKind::Hole { part, diameter, .. } if part == b && diameter == 40.)
    );

    // The back plate is built with the four bolt holes through it.
    let plain = 200. * 200. * 10.;
    let removed = 4. * std::f64::consts::PI * 11. * 11. * 10.;
    let volume = p.element_volume_mm3(a).unwrap();
    assert!(
        (volume - (plain - removed)).abs() < removed * 0.05,
        "{volume} vs {}",
        plain - removed
    );
}

#[test]
fn bolts_with_too_small_a_step_or_an_unknown_body_are_refused() {
    let mut f = flange();
    if let DetailShape::Bolts { column_step, .. } = &mut f.details[0].shape {
        *column_step = s("20");
    }
    let mut document = Document::new(Project::default()).unwrap();
    let id = ObjectId(document.project().next_id + 100);
    let error = document
        .transact(
            "Фланец",
            &[
                Command::SetFamily {
                    family: Box::new(f.clone()),
                },
                Command::CreateFamilyInstance {
                    instance: instance(id, "Фланец", ""),
                },
            ],
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("шаг столбцов"), "{error}");

    let mut f = flange();
    f.details[1].bodies = vec![s("Нет такого")];
    assert!(f.validate().unwrap_err().to_string().contains("Нет такого"));
}

#[test]
fn a_nested_family_brings_its_welds() {
    let seat = sample("Столик опорный");
    let mut host = flange();
    host.name = s("Фланец со столиком");
    host.nested = vec![seat.clone()];
    host.bodies.push(FamilyBody {
        name: s("Столик"),
        material: s("С255"),
        visible: String::new(),
        array: None,
        shape: BodyShape::Nested {
            family: seat.name.clone(),
            type_name: s("С-1"),
            at: [s("B / 2"), s("20"), s("B")],
            rotation: String::new(),
            values: BTreeMap::new(),
        },
    });
    // Weld the nested seat to the front plate.
    host.details.push(FamilyDetail {
        name: s("Шов столика"),
        bodies: vec![s("Фланец 2"), s("Столик")],
        visible: String::new(),
        array: None,
        shape: DetailShape::Weld {
            path: vec![
                [s("B / 2 - 75"), s("20"), s("B")],
                [s("B / 2 + 75"), s("20"), s("B")],
            ],
            weld_type: WeldType::Fillet,
            size: s("6"),
            shop: true,
            sides: WeldSides::One,
            around: false,
            process: WeldProcess::Manual,
            gost: WeldGost::default(),
        },
    });
    let (document, id) = place(host, "");
    let p = document.project();
    let children = &p.family_instances[&id].children;
    // The seat's own rib welds and holes come along.
    assert_eq!(details(p, id, "body-2-0.detail-0-").len(), 2);
    assert_eq!(details(p, id, "body-2-0.detail-1-").len(), 2);
    let weld = &p.details[&children["detail-3-0"]];
    let DetailKind::Weld { parts, .. } = &weld.kind else {
        panic!()
    };
    // The front plate and every part of the nested seat.
    assert_eq!(parts.len(), 1 + 3);
    assert!(parts.contains(&children["body-2-0.body-0-0"]));
}
