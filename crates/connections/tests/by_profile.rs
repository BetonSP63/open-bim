//! Sizes picked from the section of the main part are valid templates for
//! every kind, grow with the section, and skip sections they do not know.
use open_bim_connections::by_profile::{SHEETS, bolt_for_height, fit_to_profile};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Profile, ProfileShape, Project,
    TemplateKind,
};

fn project(shape: ProfileShape) -> Project {
    let mut p = Project::default();
    let profile = ObjectId(50);
    p.profiles.insert(
        profile,
        Profile {
            id: profile,
            name: "Сечение".into(),
            shape,
            source: "Тест".into(),
            standard: None,
            area_mm2: None,
        },
    );
    let (frame, length) = Frame::along(Point3([0.; 3]), Point3([0., 6000., 0.])).unwrap();
    p.elements.insert(
        ObjectId(9),
        Element {
            id: ObjectId(9),
            name: "Балка".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile,
                length,
                frame,
                position: Default::default(),
            },
        },
    );
    p
}

fn instance(kind: TemplateKind) -> ConnectionInstance {
    ConnectionInstance {
        id: ObjectId(20),
        primary: ObjectId(9),
        secondary: None,
        at_end: false,
        template: ConnectionTemplate {
            version: 1,
            name: "Узел".into(),
            kind,
            thickness: 10.,
            edge_distance: 45.,
            bolt_diameter: 20.,
            clearance: 2.,
            bolt_standard: "ГОСТ 7798-70".into(),
            bolt_grade: "5.8".into(),
            weld_size: 6.,
        },
        children: Default::default(),
        general: Default::default(),
    }
}

fn i_section(height: f64, width: f64, web: f64, flange: f64) -> ProfileShape {
    ProfileShape::ISection {
        height,
        width,
        web,
        flange,
    }
}

fn kinds() -> Vec<TemplateKind> {
    vec![
        TemplateKind::BasePlate { overhang: 70. },
        TemplateKind::EndPlate { overhang: 40. },
        TemplateKind::ClipAngle {
            leg: 100.,
            height: 170.,
            pitch: 80.,
        },
        TemplateKind::FinPlate {
            height: 200.,
            lap: 80.,
            gap: 15.,
            pitch: 70.,
            rows: 3,
        },
        TemplateKind::ColumnStiffeners { chamfer: 20. },
        TemplateKind::BeamSplice {
            rows: 2,
            pitch: 70.,
        },
        TemplateKind::Haunch {
            length: 600.,
            depth: 300.,
        },
        TemplateKind::Seat {
            length: 150.,
            width: 200.,
            depth: 150.,
        },
    ]
}

#[test]
fn every_kind_gets_a_valid_template_that_grows_with_the_section() {
    let small = project(i_section(200., 100., 5.6, 8.5));
    let large = project(i_section(600., 300., 12., 20.));
    for kind in kinds() {
        let a = fit_to_profile(&small, &instance(kind.clone())).unwrap();
        let b = fit_to_profile(&large, &instance(kind.clone())).unwrap();
        for t in [&a, &b] {
            t.validate().unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(SHEETS.contains(&t.thickness), "{t:?}");
        }
        assert_eq!((a.bolt_diameter, b.bolt_diameter), (16., 24.));
        assert!(b.thickness >= a.thickness, "{kind:?}");
        assert!(b.edge_distance > a.edge_distance);
    }
    assert_eq!(bolt_for_height(300.), 20.);
    assert_eq!(bolt_for_height(900.), 27.);
}

#[test]
fn a_gusset_takes_an_angle_and_other_sections_are_left_alone() {
    let gusset = || {
        instance(TemplateKind::Gusset {
            bolts: 1,
            pitch: 70.,
        })
    };
    let angle = project(ProfileShape::Angle {
        height: 100.,
        width: 75.,
        thickness: 8.,
    });
    let t = fit_to_profile(&angle, &gusset()).unwrap();
    t.validate().unwrap();
    assert!(matches!(t.kind, TemplateKind::Gusset { bolts: 2, .. }));
    assert_eq!((t.bolt_diameter, t.thickness), (20., 10.));
    // A gusset on an I-beam, or an end plate on an angle: not picked.
    let beam = project(i_section(300., 150., 8., 12.));
    assert!(fit_to_profile(&beam, &gusset()).is_none());
    assert!(fit_to_profile(&angle, &instance(TemplateKind::EndPlate { overhang: 40. })).is_none());
    let tube = project(ProfileShape::CircularHollow {
        diameter: 114.,
        thickness: 4.5,
    });
    assert!(fit_to_profile(&tube, &instance(TemplateKind::EndPlate { overhang: 40. })).is_none());
}
