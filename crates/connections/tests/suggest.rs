use open_bim_connections::suggest::suggest;
use open_bim_model::{Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project};

fn member(id: ObjectId, kind: MemberKind, start: Point3, end: Point3) -> Element {
    let (frame, length) = Frame::along(start, end).unwrap();
    Element {
        id,
        name: format!("{id}"),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: kind,
            profile: ObjectId(2),
            length,
            frame,
            position: Default::default(),
        },
    }
}

#[test]
fn a_beam_meeting_a_column_offers_three_recipes_in_file_order() {
    let mut project = Project::default();
    project.elements.insert(
        ObjectId(8),
        member(
            ObjectId(8),
            MemberKind::Column,
            Point3([0., 0., 0.]),
            Point3([0., 0., 3000.]),
        ),
    );
    project.elements.insert(
        ObjectId(9),
        member(
            ObjectId(9),
            MemberKind::Beam,
            Point3([0., 0., 3000.]),
            Point3([4000., 0., 3000.]),
        ),
    );
    project.next_id = 10;
    project.validate().unwrap();
    let ideas = suggest(&project, ObjectId(8), ObjectId(9));
    assert_eq!(
        ideas.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        [
            "Шарнир на пластине",
            "Торцевая пластина",
            "Уголковое соединение"
        ]
    );
    assert_eq!(ideas[0].primary, ObjectId(8));
    assert_eq!(ideas[0].secondary, ObjectId(9));
    assert!(ideas.len() <= 3);
}
