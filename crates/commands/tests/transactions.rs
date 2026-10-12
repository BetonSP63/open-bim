use open_bim_commands::*;
use open_bim_model::*;
use std::collections::BTreeSet;

#[test]
fn canonical_hash_matches_independent_sha256_fixture() {
    let expected = include_str!("../../../tests/command_sequences/steel-frame.sha256").trim();
    assert_eq!(
        project_hash(&example_steel_frame().unwrap()).unwrap(),
        expected
    );
}

#[test]
fn rigid_transform_changes_origin_and_local_axes() {
    let mut document = Document::new(example_steel_frame().unwrap()).unwrap();
    let rotation = Frame {
        origin: Point3([10.0, 20.0, 30.0]),
        axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    };
    document
        .transact(
            "Rotate and move",
            &[Command::Transform {
                ids: vec![ObjectId(9)],
                rotation,
                pivot: Point3([0.0; 3]),
            }],
        )
        .unwrap();
    let frame = document.project().elements[&ObjectId(9)].shape.frame();
    assert_eq!(frame.origin, Point3([10.0, 6020.0, 30.0]));
    assert_eq!(frame.axes[0], [0.0, 1.0, 0.0]);
    assert_eq!(frame.axes[1], [-1.0, 0.0, 0.0]);
    document.project().validate().unwrap();
}

#[test]
fn hundred_command_sequence_round_trips_original_and_final_hashes() {
    let sequence: Vec<Command> = serde_json::from_str(include_str!(
        "../../../tests/command_sequences/100-commands.json"
    ))
    .unwrap();
    assert_eq!(sequence.len(), 100);
    let project = example_steel_frame().unwrap();
    let original = project_hash(&project).unwrap();
    let mut document = Document::new(project).unwrap();
    for (i, command) in sequence.iter().enumerate() {
        document
            .transact(format!("Command {i}"), std::slice::from_ref(command))
            .unwrap();
    }
    let final_hash = project_hash(document.project()).unwrap();
    assert_ne!(original, final_hash);
    for _ in 0..100 {
        document.undo().unwrap();
    }
    assert_eq!(project_hash(document.project()).unwrap(), original);
    assert_eq!(document.history_depth(), (0, 100));
    for _ in 0..100 {
        document.redo().unwrap();
    }
    assert_eq!(project_hash(document.project()).unwrap(), final_hash);
    assert_eq!(document.history_depth(), (100, 0));
}

#[test]
fn failed_operation_rolls_back_model_history_and_redo() {
    let mut document = Document::new(example_steel_frame().unwrap()).unwrap();
    document
        .transact(
            "Rename",
            &[Command::RenameProject {
                name: "Changed".into(),
            }],
        )
        .unwrap();
    document.undo().unwrap();
    let hash = project_hash(document.project()).unwrap();
    let depth = document.history_depth();
    let mut element = document.project().elements[&ObjectId(8)].clone();
    element.id = ObjectId(100);
    let commands = [
        Command::Create { element },
        Command::ChangeProperty {
            id: ObjectId(999),
            key: "X".into(),
            value: Some(PropertyValue::Number(10.0)),
        },
    ];
    assert!(document.transact("Fail after create", &commands).is_err());
    assert_eq!(document.history_depth(), depth);
    assert_eq!(project_hash(document.project()).unwrap(), hash);
    document.redo().unwrap();
    assert_eq!(document.project().name, "Changed");
}

#[test]
fn deletion_requires_explicit_dependency_policy_and_restores_identity() {
    let mut document = Document::new(example_steel_frame().unwrap()).unwrap();
    let hash = project_hash(document.project()).unwrap();
    assert!(matches!(
        document.transact(
            "Delete parent",
            &[Command::Delete {
                id: ObjectId(15),
                policy: DeletePolicy::RejectDependencies
            }]
        ),
        Err(CommandError::Dependencies(ObjectId(15)))
    ));
    let event = document
        .transact(
            "Delete assembly",
            &[Command::Delete {
                id: ObjectId(15),
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
    assert_eq!(
        event.touched,
        BTreeSet::from([ObjectId(8), ObjectId(14), ObjectId(15)])
    );
    assert!(!document.project().elements.contains_key(&ObjectId(8)));
    assert!(!document.project().elements.contains_key(&ObjectId(14)));
    document.project().validate().unwrap();
    document.undo().unwrap();
    assert_eq!(project_hash(document.project()).unwrap(), hash);
}

#[test]
fn change_events_invalidate_element_and_assembly_caches_only() {
    let mut document = Document::new(example_steel_frame().unwrap()).unwrap();
    let mut cache = DependencyCache::default();
    cache.insert("render-8".into(), BTreeSet::from([ObjectId(8)]), "old mesh");
    cache.insert(
        "assembly-drawing".into(),
        BTreeSet::from([ObjectId(15)]),
        "old drawing",
    );
    cache.insert(
        "unrelated".into(),
        BTreeSet::from([ObjectId(9)]),
        "untouched",
    );
    let event = document
        .transact(
            "Material property",
            &[Command::ChangeProperty {
                id: ObjectId(8),
                key: "Марка".into(),
                value: Some(PropertyValue::Text("К-2".into())),
            }],
        )
        .unwrap();
    assert_eq!(event.touched, BTreeSet::from([ObjectId(8), ObjectId(15)]));
    assert_eq!(cache.invalidate(&event), 2);
    assert!(cache.get("render-8").is_none());
    assert!(cache.get("assembly-drawing").is_none());
    assert_eq!(cache.get("unrelated"), Some(&"untouched"));
    let undo_event = document.undo().unwrap();
    assert!(undo_event.serial > event.serial);
    assert_eq!(undo_event.touched, event.touched);
}

#[test]
fn new_edit_clears_redo_and_invalid_transform_is_atomic() {
    let mut document = Document::new(example_steel_frame().unwrap()).unwrap();
    document
        .transact("Rename", &[Command::RenameProject { name: "A".into() }])
        .unwrap();
    document.undo().unwrap();
    let hash = project_hash(document.project()).unwrap();
    let mut rotation = Frame::default();
    rotation.axes[0][0] = 2.0;
    assert!(
        document
            .transact(
                "Scale is not rigid",
                &[Command::Transform {
                    ids: vec![ObjectId(8)],
                    rotation,
                    pivot: Point3([0.0; 3])
                }]
            )
            .is_err()
    );
    assert_eq!(project_hash(document.project()).unwrap(), hash);
    assert_eq!(document.history_depth(), (0, 1));
    document
        .transact(
            "Rename again",
            &[Command::RenameProject { name: "B".into() }],
        )
        .unwrap();
    assert_eq!(document.history_depth(), (1, 0));
    assert_eq!(document.redo(), Err(CommandError::NoHistory));
}

#[test]
fn material_delete_cascades_all_dependants_without_dangling_references() {
    let mut document = Document::new(example_steel_frame().unwrap()).unwrap();
    let hash = project_hash(document.project()).unwrap();
    assert!(
        document
            .transact(
                "Reject material",
                &[Command::Delete {
                    id: ObjectId(1),
                    policy: DeletePolicy::RejectDependencies
                }]
            )
            .is_err()
    );
    let event = document
        .transact(
            "Delete material",
            &[Command::Delete {
                id: ObjectId(1),
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
    assert!(document.project().elements.is_empty());
    assert!(document.project().assemblies.is_empty());
    assert_eq!(event.touched.len(), 9);
    document.project().validate().unwrap();
    document.undo().unwrap();
    assert_eq!(project_hash(document.project()).unwrap(), hash);
}
