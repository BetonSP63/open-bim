use open_bim_model::{
    Assembly, Detail, DetailKind, ElementShape, Frame, ObjectId, Point3, Project,
    example_steel_frame,
};
use open_bim_numbering::number;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn reference_groups_survive_renaming_reordering_translation_rotation_and_reopen() {
    let mut p = Project::from_json(include_str!(
        "../../../tests/fixtures/model/steel-frame.json"
    ))
    .unwrap();
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/numbering/expected-groups.json"
    ))
    .unwrap();
    let original = number(&p).unwrap();
    let mut seen = BTreeSet::new();
    for group in reference["part_groups"].as_array().unwrap() {
        let positions: BTreeSet<_> = group
            .as_array()
            .unwrap()
            .iter()
            .map(|id| original.parts[&ObjectId(id.as_u64().unwrap())].clone())
            .collect();
        assert_eq!(positions.len(), 1);
        assert!(seen.insert(positions.into_iter().next().unwrap()));
    }
    // Rigidly transform whole assembly as well as free parts; manufacture stays fixed.
    for part in p.elements.values_mut() {
        part.name = "Different model name".into();
        let f = part.shape.frame_mut();
        f.origin = Point3([
            -f.origin.0[1] + 12000.,
            f.origin.0[0] - 33000.,
            f.origin.0[2] + 45.,
        ]);
        f.axes = f.axes.map(|a| [-a[1], a[0], a[2]]);
    }
    let entries: Vec<_> = std::mem::take(&mut p.elements).into_iter().rev().collect();
    p.elements.extend(entries);
    let after = number(&p).unwrap();
    assert_eq!(original, after);
    let reopen = open_bim_persistence::decode(&open_bim_persistence::encode(&p).unwrap()).unwrap();
    assert_eq!(number(&reopen).unwrap(), after);
}

#[test]
fn material_profile_and_holes_change_manufacture_and_affected_assemblies() {
    let original = example_steel_frame().unwrap();
    let baseline = number(&original).unwrap();
    for mode in 0..3 {
        let mut p = original.clone();
        match mode {
            0 => {
                let id = p.allocate_id().unwrap();
                let mut material = p.materials[&ObjectId(1)].clone();
                material.id = id;
                material.name = "S355".into();
                p.materials.insert(id, material);
                p.elements.get_mut(&ObjectId(8)).unwrap().material = id;
            }
            1 => {
                if let ElementShape::Member { profile, .. } =
                    &mut p.elements.get_mut(&ObjectId(8)).unwrap().shape
                {
                    *profile = ObjectId(7);
                }
            }
            _ => {
                let id = p.allocate_id().unwrap();
                p.details.insert(
                    id,
                    Detail {
                        id,
                        kind: DetailKind::Hole {
                            part: ObjectId(8),
                            frame: Frame {
                                origin: Point3([0., 0., 100.]),
                                ..Frame::default()
                            },
                            diameter: 22.,
                            depth: 100.,
                        },
                    },
                );
            }
        }
        let current = number(&p).unwrap();
        assert_ne!(
            baseline.part_keys[&ObjectId(8)],
            current.part_keys[&ObjectId(8)]
        );
        assert_ne!(
            baseline.assembly_keys[&ObjectId(15)],
            current.assembly_keys[&ObjectId(15)]
        );
        assert!(current.changed_since(&baseline).contains(&ObjectId(8)));
        assert!(current.changed_since(&baseline).contains(&ObjectId(15)));
        assert_ne!(current.parts[&ObjectId(8)], current.parts[&ObjectId(9)]);
        for id in [9, 10, 11, 12, 13, 14] {
            assert_eq!(
                baseline.part_keys[&ObjectId(id)],
                current.part_keys[&ObjectId(id)]
            );
        }
    }
}

#[test]
fn identical_shape_with_new_catalog_ids_and_reversed_contour_is_equivalent() {
    let mut p = example_steel_frame().unwrap();
    let mut part = p.elements[&ObjectId(14)].clone();
    let id = p.allocate_id().unwrap();
    part.id = id;
    part.name = "copy".into();
    if let ElementShape::Plate { contour, .. } = &mut part.shape {
        contour.rotate_left(2);
        contour.reverse();
    }
    p.elements.insert(id, part);
    let profile_id = p.allocate_id().unwrap();
    let mut profile = p.profiles[&ObjectId(2)].clone();
    profile.id = profile_id;
    profile.name = "Imported equal profile".into();
    p.profiles.insert(profile_id, profile);
    if let ElementShape::Member { profile, .. } =
        &mut p.elements.get_mut(&ObjectId(9)).unwrap().shape
    {
        *profile = profile_id;
    }
    let n = number(&p).unwrap();
    assert_eq!(n.parts[&ObjectId(14)], n.parts[&id]);
    assert_eq!(n.parts[&ObjectId(8)], n.parts[&ObjectId(9)]);
}

#[test]
fn assemblies_use_main_part_multiplicity_layout_and_nested_children() {
    let mut p = example_steel_frame().unwrap();
    let first = p.assemblies[&ObjectId(15)].clone();
    let mut second = first.clone();
    second.id = p.allocate_id().unwrap();
    second.name = "Another assembly".into();
    second.main_part = ObjectId(9);
    let mut plate = p.elements[&ObjectId(14)].clone();
    plate.id = p.allocate_id().unwrap();
    let plate_id = plate.id;
    plate.shape.frame_mut().origin = p.elements[&ObjectId(9)].shape.frame().origin;
    p.elements.insert(plate_id, plate);
    second.members = BTreeSet::from([ObjectId(9), plate_id]);
    let second_id = second.id;
    p.assemblies.insert(second_id, second);
    let equal = number(&p).unwrap();
    assert_eq!(equal.assemblies[&first.id], equal.assemblies[&second_id]);
    p.assemblies.get_mut(&second_id).unwrap().main_part = plate_id;
    let changed = number(&p).unwrap();
    assert_ne!(
        changed.assemblies[&first.id],
        changed.assemblies[&second_id]
    );
    p.assemblies.get_mut(&second_id).unwrap().main_part = ObjectId(9);
    p.elements
        .get_mut(&plate_id)
        .unwrap()
        .shape
        .frame_mut()
        .origin
        .0[0] += 10.;
    assert_ne!(
        number(&p).unwrap().assemblies[&first.id],
        number(&p).unwrap().assemblies[&second_id]
    );
    let parent = p.allocate_id().unwrap();
    p.assemblies.insert(
        parent,
        Assembly {
            id: parent,
            name: "Nested".into(),
            main_part: ObjectId(10),
            members: BTreeSet::from([ObjectId(10)]),
            children: BTreeSet::from([second_id]),
            properties: BTreeMap::new(),
        },
    );
    let nested = number(&p).unwrap();
    p.assemblies
        .get_mut(&second_id)
        .unwrap()
        .members
        .insert(ObjectId(11));
    let edited = number(&p).unwrap();
    assert_ne!(nested.assembly_keys[&parent], edited.assembly_keys[&parent]);
}
