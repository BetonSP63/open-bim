use open_bim_model::{ElementShape, MemberKind, SweptPart, example_steel_frame};
use open_bim_numbering::part_key;

#[test]
fn bend_parameters_change_the_manufacturing_key() {
    let mut project = example_steel_frame().unwrap();
    let id = project
        .elements
        .values()
        .find(|e| {
            matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    ..
                }
            )
        })
        .unwrap()
        .id;
    let straight = part_key(&project, id).unwrap();
    project.swept_parts.insert(
        id,
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
    );
    let curved = part_key(&project, id).unwrap();
    assert_ne!(straight, curved);
    if let SweptPart::CurvedBeam { radius, .. } = project.swept_parts.get_mut(&id).unwrap() {
        *radius = 1200.;
    }
    assert_ne!(curved, part_key(&project, id).unwrap());
}

#[test]
fn swept_keys_ignore_placeholder_dimensions_but_keep_manufacturing_data() {
    use open_bim_model::{LengthMm, ObjectId};
    let mut project = example_steel_frame().unwrap();
    let member = ObjectId(12);
    project.swept_parts.insert(
        member,
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
    );
    let before = part_key(&project, member).unwrap();
    if let ElementShape::Member { length, .. } =
        &mut project.elements.get_mut(&member).unwrap().shape
    {
        *length = LengthMm::new(1234.).unwrap();
    } else {
        panic!("ожидалась балка");
    }
    assert_eq!(before, part_key(&project, member).unwrap());
    if let ElementShape::Member { profile, .. } =
        &mut project.elements.get_mut(&member).unwrap().shape
    {
        *profile = ObjectId(7);
    }
    assert_ne!(before, part_key(&project, member).unwrap());

    let plate = ObjectId(14);
    project.swept_parts.insert(
        plate,
        SweptPart::BentPlate {
            width: 300.,
            first_length: 100.,
            second_length: 200.,
            radius: 20.,
            angle: 1.,
        },
    );
    let before = part_key(&project, plate).unwrap();
    if let ElementShape::Plate { contour, .. } =
        &mut project.elements.get_mut(&plate).unwrap().shape
    {
        *contour = vec![[0., 0.], [500., 0.], [500., 600.], [0., 600.]];
    } else {
        panic!("ожидался лист");
    }
    assert_eq!(before, part_key(&project, plate).unwrap());
    if let ElementShape::Plate { thickness, .. } =
        &mut project.elements.get_mut(&plate).unwrap().shape
    {
        *thickness = LengthMm::new(30.).unwrap();
    }
    assert_ne!(before, part_key(&project, plate).unwrap());
}

#[test]
fn trajectory_key_is_independent_of_local_placement() {
    use open_bim_model::ObjectId;
    let mut project = example_steel_frame().unwrap();
    let id = ObjectId(12);
    let mut set = |sweep| {
        project.swept_parts.insert(id, sweep);
        part_key(&project, id).unwrap()
    };
    assert_eq!(
        set(SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.
        }),
        set(SweptPart::CurvedBeam {
            center: [30., 20.],
            radius: 1000.,
            start_angle: 2.,
            sweep_angle: 1.
        })
    );
    assert_eq!(
        set(SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.], [1000., 1000.]],
            bend_radius: 200.
        }),
        set(SweptPart::PolyBeam {
            points: vec![[50., 20.], [50., 1020.], [-950., 1020.]],
            bend_radius: 200.
        })
    );
    assert_eq!(
        set(SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.]],
            bend_radius: 0.
        }),
        set(SweptPart::PolyBeam {
            points: vec![[30., 20.], [530., 20.], [1030., 20.]],
            bend_radius: 200.
        })
    );
    assert_ne!(
        set(SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.], [1000., 1000.]],
            bend_radius: 200.
        }),
        set(SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.], [1000., -1000.]],
            bend_radius: 200.
        })
    );
}
