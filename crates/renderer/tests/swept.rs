use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Project, SweptPart,
};
use open_bim_renderer::{DVec3, Ray, Scene, ViewState};

#[test]
fn swept_member_uses_actual_mesh_for_display_bounds_and_picking() {
    let mut project = Project::default();
    let id = project.allocate_id().unwrap();
    project.elements.insert(
        id,
        Element {
            id,
            name: "Гнутая балка".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(7),
                length: LengthMm::new(1000. * std::f64::consts::FRAC_PI_2).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
        },
    );
    project.swept_parts.insert(
        id,
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: std::f64::consts::FRAC_PI_2,
        },
    );
    let scene = Scene::from_project(&project).unwrap();
    assert!(scene.bounds.max.z < 101.);
    assert!(scene.bounds.max.x > 950. && scene.bounds.max.y > 950.);
    assert_eq!(
        scene.pick(
            Ray {
                origin: DVec3::new(700., 700., 1000.),
                direction: -DVec3::Z
            },
            &ViewState::default()
        ),
        Some(id)
    );
    assert_eq!(
        scene.pick(
            Ray {
                origin: DVec3::new(0., 0., 1000.),
                direction: -DVec3::Z
            },
            &ViewState::default()
        ),
        None
    );
}
