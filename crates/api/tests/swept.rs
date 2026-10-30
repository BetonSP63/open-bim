use open_bim_api::Session;
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Project, SweptPart,
};
use std::collections::BTreeMap;

#[test]
fn copies_swept_data_and_refuses_straight_member_projection() {
    let mut project = Project::default();
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
    let swept = SweptPart::CurvedBeam {
        center: [0., 0.],
        radius: 1000.,
        start_angle: 0.,
        sweep_angle: 1.,
    };
    project.swept_parts.insert(id, swept.clone());
    let mut session = Session::new(project).unwrap();
    assert!(session.select_member(id).is_err());
    let copy = session.copy_by(&[id], [10., 20., 30.]).unwrap()[0];
    assert_eq!(session.project().swept_parts.get(&copy), Some(&swept));
    assert_eq!(
        session.project().elements[&copy].shape.frame().origin,
        Point3([10., 20., 30.])
    );
    session.commit("Копия").unwrap();
    session.undo().unwrap();
    assert!(!session.project().swept_parts.contains_key(&copy));
}
