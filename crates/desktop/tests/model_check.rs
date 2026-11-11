//! The model check lists problems and can fix the obvious ones. It does
//! not run on commit.
use open_bim_commands::{Command, Document};
use open_bim_desktop::check::{self, issues};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, Level, LevelBinding, MemberEnd, MemberKind, ObjectId,
    Point3,
};

fn beam(id: ObjectId, x: f64) -> Element {
    let (frame, length) = Frame::along(Point3([x, 0., 0.]), Point3([x + 6000., 0., 0.])).unwrap();
    Element {
        id,
        name: format!("Б{id}"),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: MemberKind::Beam,
            profile: ObjectId(2),
            length,
            frame,
            position: Default::default(),
        },
    }
}

#[test]
fn ribbon_hint_names_the_shortcut() {
    use open_bim_desktop::ui::commands::{self, Cmd};
    let hint = commands::faster_hint(Cmd::ToolBeam).unwrap();
    assert!(hint.starts_with("Быстрее:"));
    assert!(commands::caption(Cmd::ToolBeam).contains("Alt+B"));
    assert!(commands::faster_hint(Cmd::ModelCheck).is_none());
}

#[test]
fn a_lone_beam_has_free_ends() {
    let mut doc = Document::new(open_bim_model::Project::default()).unwrap();
    doc.transact(
        "Балка",
        &[Command::Create {
            element: beam(ObjectId(8), 0.),
        }],
    )
    .unwrap();
    let found = issues(doc.project());
    assert!(
        found
            .iter()
            .filter(|i| i.text.contains("Свободный"))
            .count()
            >= 2
    );
}

#[test]
fn a_duplicate_can_be_deleted() {
    let mut doc = Document::new(open_bim_model::Project::default()).unwrap();
    doc.transact(
        "Балки",
        &[
            Command::Create {
                element: beam(ObjectId(8), 0.),
            },
            Command::Create {
                element: beam(ObjectId(9), 0.),
            },
        ],
    )
    .unwrap();
    let found = issues(doc.project());
    let duplicate = found.iter().find(|i| i.text.contains("Дубль")).unwrap();
    assert!(duplicate.fix.is_some());
    doc.transact("Исправить", duplicate.fix.as_ref().unwrap())
        .unwrap();
    assert_eq!(doc.project().elements.len(), 1);
    doc.undo().unwrap();
    assert_eq!(doc.project().elements.len(), 2);
}

#[test]
fn a_level_binding_that_does_not_match_is_listed_and_fixed() {
    let mut project = open_bim_model::Project::default();
    project.levels.insert(
        ObjectId(20),
        Level {
            id: ObjectId(20),
            name: "0".into(),
            elevation_mm: 0.,
        },
    );
    project.elements.insert(
        ObjectId(8),
        Element {
            id: ObjectId(8),
            name: "К".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Column,
                profile: ObjectId(2),
                length: LengthMm::new(3000.).unwrap(),
                frame: Frame {
                    origin: Point3([0., 0., 400.]),
                    ..Frame::default()
                },
                position: Default::default(),
            },
        },
    );
    project.level_ends.set(
        ObjectId(8),
        MemberEnd::Bottom,
        Some(LevelBinding {
            level: ObjectId(20),
            offset_mm: 0.,
        }),
    );
    project.next_id = 21;
    project.validate().unwrap();
    let found = check::issues(&project);
    let broken = found
        .iter()
        .find(|i| i.text.contains("ушёл с уровня"))
        .unwrap();
    let mut doc = Document::new(project).unwrap();
    doc.transact("Исправить", broken.fix.as_ref().unwrap())
        .unwrap();
    let (bottom, _, _) = open_bim_model::level_ends::column_ends(
        doc.project(),
        &doc.project().elements[&ObjectId(8)].shape,
    )
    .unwrap();
    assert!(
        bottom.0[2].abs() < 1e-6,
        "низ вернулся на уровень, {}",
        bottom.0[2]
    );
    assert_eq!(
        doc.project()
            .level_ends
            .get(ObjectId(8), MemberEnd::Bottom)
            .unwrap()
            .offset_mm,
        0.
    );
}
