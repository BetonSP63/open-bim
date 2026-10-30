use open_bim_commands::{Command, Document};
use open_bim_desktop::tools::{Edit, edit_commands};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Project, SweptPart,
};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn copying_preserves_sweep_and_mirroring_is_rejected() {
    let mut p = Project::default();
    let id = p.allocate_id().unwrap();
    p.elements.insert(
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
    p.swept_parts.insert(id, swept.clone());
    let selected = BTreeSet::from([id]);
    assert!(
        edit_commands(
            &p,
            &selected,
            &Edit::Mirror {
                normal: Point3([1., 0., 0.]),
                offset: 0.
            }
        )
        .is_err()
    );
    let commands = edit_commands(&p, &selected, &Edit::Copy(Point3([10., 20., 30.]))).unwrap();
    let copy = match &commands[0] {
        Command::CreateSwept {
            element,
            swept: actual,
        } => {
            assert_eq!(actual, &swept);
            element.id
        }
        _ => panic!("нет параметров заметания"),
    };
    let mut doc = Document::new(p).unwrap();
    doc.transact("Копия", &commands).unwrap();
    assert_eq!(doc.project().swept_parts.get(&copy), Some(&swept));
    assert_eq!(
        doc.project().elements[&copy].shape.frame().origin,
        Point3([10., 20., 30.])
    );
    doc.undo().unwrap();
    assert!(!doc.project().swept_parts.contains_key(&copy));
}
