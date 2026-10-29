//! A family placed «как изделие» shows in the form 7 specification of the
//! assembly that holds it as one line under «Прочие изделия».
use open_bim_commands::{Command, Document};
use open_bim_drawings::tables::{Row, assembly_specification};
use open_bim_model::*;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn an_item_family_is_one_line_of_its_host_assembly() {
    let mut d = Document::new(Project::default()).unwrap();
    let p = d.project();
    let profile = *p.profiles.keys().next().unwrap();
    let material = *p.materials.keys().next().unwrap();
    let column = ObjectId(p.next_id);
    let shape = p
        .new_member(
            MemberKind::Column,
            profile,
            Point3([0., 0., 0.]),
            Point3([0., 0., 3000.]),
            PartPosition::default(),
        )
        .unwrap();
    let mut seat = sample_families().remove(0);
    seat.spec = SpecMode::Item;
    let instance = ObjectId(column.0 + 1);
    d.transact(
        "Колонна и столик",
        &[
            Command::Create {
                element: Element {
                    id: column,
                    name: "Колонна".into(),
                    material,
                    shape,
                    properties: BTreeMap::new(),
                },
            },
            Command::SetFamily {
                family: Box::new(seat),
            },
            Command::CreateFamilyInstance {
                instance: FamilyInstance {
                    id: instance,
                    family: "Столик опорный".into(),
                    type_name: "С-2".into(),
                    frame: Frame {
                        origin: Point3([0., 150., 2000.]),
                        ..Frame::default()
                    },
                    length: 0.,
                    values: BTreeMap::new(),
                    children: BTreeMap::new(),
                },
            },
        ],
    )
    .unwrap();
    let own = d.project().family_instances[&instance].children["assembly"];
    let host = ObjectId(d.project().next_id);
    d.transact(
        "Сборка",
        &[Command::CreateAssembly {
            assembly: Assembly {
                id: host,
                name: "Колонна".into(),
                main_part: column,
                members: BTreeSet::from([column]),
                children: BTreeSet::from([own]),
                properties: BTreeMap::new(),
            },
        }],
    )
    .unwrap();
    let p = d.project();
    let rows = assembly_specification(p, host).unwrap();
    let heading = rows
        .iter()
        .position(|r| *r == Row::Heading("Прочие изделия".into()))
        .expect("no items section");
    let Row::Cells(item) = &rows[heading + 1] else {
        panic!()
    };
    assert_eq!(item[2], "Столик опорный С-2");
    assert_eq!(item[3], "1");
    let parts: f64 = p.family_instances[&instance]
        .children
        .iter()
        .filter(|(role, _)| role.starts_with("body-"))
        .map(|(_, id)| p.element_mass_kg(*id).unwrap())
        .sum();
    assert_eq!(item[4], open_bim_drawings::tables::mass(parts));
    // «Детали» lists the column only.
    let details = rows[1..heading]
        .iter()
        .filter(|r| matches!(r, Row::Cells(_)))
        .count();
    assert_eq!(details, 1);
    // The item's own specification still lists its plates.
    let own_rows = assembly_specification(p, own).unwrap();
    assert!(!own_rows.contains(&Row::Heading("Прочие изделия".into())));
    assert_eq!(own_rows.len(), 1 + 2);
}
