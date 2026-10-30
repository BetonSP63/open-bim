use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Project, SweptPart,
};
use std::collections::BTreeMap;

fn member(project: &mut Project, swept: SweptPart) -> ObjectId {
    let id = project.allocate_id().unwrap();
    project.elements.insert(
        id,
        Element {
            id,
            name: "Гнутая балка".into(),
            material: ObjectId(1),
            properties: BTreeMap::new(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(7),
                length: LengthMm::new(1000.).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
        },
    );
    project.swept_parts.insert(id, swept);
    id
}

#[test]
fn curved_beam_volume_mass_and_roundtrip() {
    let mut p = Project::default();
    let id = member(
        &mut p,
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: std::f64::consts::FRAC_PI_2,
        },
    );
    let volume = 100. * 50. * 1000. * std::f64::consts::FRAC_PI_2;
    assert!((p.element_volume_mm3(id).unwrap() - volume).abs() < 1e-5);
    assert!((p.element_mass_kg(id).unwrap() - volume * 7850. / 1e9).abs() < 1e-9);
    assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
}

#[test]
fn polybeam_rounding_developed_length_and_volume() {
    let mut p = Project::default();
    let id = member(
        &mut p,
        SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.], [1000., 1000.]],
            bend_radius: 200.,
        },
    );
    let length = 1600. + 200. * std::f64::consts::FRAC_PI_2;
    assert!((p.part_developed_length(id).unwrap() - length).abs() < 1e-8);
    let volume = p.element_volume_mm3(id).unwrap();
    assert!((volume - 5000. * length).abs() / volume < 0.0001);
}

#[test]
fn bent_plate_flat_pattern_mass_and_body() {
    let mut p = Project::default();
    let id = p.allocate_id().unwrap();
    p.elements.insert(
        id,
        Element {
            id,
            name: "Гнутый лист".into(),
            material: ObjectId(1),
            properties: BTreeMap::new(),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [400., 0.], [400., 300.], [0., 300.]],
                thickness: LengthMm::new(10.).unwrap(),
                frame: Frame::default(),
            },
        },
    );
    p.swept_parts.insert(
        id,
        SweptPart::BentPlate {
            width: 300.,
            first_length: 100.,
            second_length: 200.,
            radius: 20.,
            angle: std::f64::consts::FRAC_PI_2,
        },
    );
    let length = 300. + 25. * std::f64::consts::FRAC_PI_2;
    let volume = length * 300. * 10.;
    assert!((p.part_developed_length(id).unwrap() - length).abs() < 1e-9);
    assert!((p.element_volume_mm3(id).unwrap() - volume).abs() < 1e-6);
    assert!((p.element_mass_kg(id).unwrap() - volume * 7850. / 1e9).abs() < 1e-9);
    let body_volume = p.element_body(id).unwrap().volume_mm3().unwrap();
    assert!((body_volume - volume).abs() / volume < 0.0001);
    assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
}

#[test]
fn invalid_swept_data_is_rejected() {
    let mut p = Project::default();
    let id = member(
        &mut p,
        SweptPart::PolyBeam {
            points: vec![[0., 0.], [10., 0.], [10., 10.]],
            bend_radius: 100.,
        },
    );
    assert!(p.validate().is_err());
    p.elements.remove(&id);
    assert!(p.validate().is_err());
}

#[test]
fn catalogue_area_controls_mass_for_symmetric_swept_sections() {
    for swept in [
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
        SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.], [1000., 1000.]],
            bend_radius: 200.,
        },
    ] {
        let mut p = Project::default();
        p.profiles.get_mut(&ObjectId(7)).unwrap().area_mm2 = Some(5500.);
        let id = member(&mut p, swept);
        let expected = 5500. * p.part_developed_length(id).unwrap();
        assert!((p.element_volume_mm3(id).unwrap() - expected).abs() < 1e-6);
        assert!((p.element_mass_kg(id).unwrap() - expected * 7850. / 1e9).abs() < 1e-9);
        p.validate().unwrap();
    }
}

#[test]
fn validation_rejects_unsweepable_sections_and_unknown_catalogue_centroid() {
    let arc = || SweptPart::CurvedBeam {
        center: [0., 0.],
        radius: 1000.,
        start_angle: 0.,
        sweep_angle: 1.,
    };
    for profile in [ObjectId(5), ObjectId(6)] {
        let mut p = Project::default();
        let id = member(&mut p, arc());
        if let ElementShape::Member {
            profile: selected, ..
        } = &mut p.elements.get_mut(&id).unwrap().shape
        {
            *selected = profile;
        }
        assert!(p.validate().is_err());
        assert!(p.element_body(id).is_err());
    }
    for profile in [ObjectId(3), ObjectId(4)] {
        let mut p = Project::default();
        let id = member(&mut p, arc());
        if let ElementShape::Member {
            profile: selected, ..
        } = &mut p.elements.get_mut(&id).unwrap().shape
        {
            *selected = profile;
        }
        let shape_area = p.profiles[&profile].shape.area_mm2().unwrap();
        p.profiles.get_mut(&profile).unwrap().area_mm2 = Some(shape_area + 10.);
        assert!(p.validate().is_err());
        assert!(p.element_mass_kg(id).is_err());
    }
    let mut p = Project::default();
    let id = member(
        &mut p,
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 10.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
    );
    assert!(p.validate().is_err());
    assert!(p.element_body(id).is_err());
}
