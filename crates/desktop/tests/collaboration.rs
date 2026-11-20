use open_bim_desktop::sharing_session::SharingSession;
use open_bim_model::{ObjectId, Project, PropertyValue, example_steel_frame};
use open_bim_sharing::{Owner, SharedModel};

#[test]
fn local_copies_keep_offline_edits_and_publish_different_objects() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Сетевая модель");
    let source = example_steel_frame().unwrap();
    let ids: Vec<ObjectId> = source.elements.keys().copied().take(2).collect();
    let owner_a = Owner::new("Анна", "mac");
    let owner_b = Owner::new("Борис", "pc");
    let shared = SharedModel::create(&root, &source, &owner_a, "Начало").unwrap();
    let path_a = tmp.path().join("Рабочая копия А.obim");
    let path_b = tmp.path().join("Рабочая копия Б.obim");
    let (mut a, mut project_a) = SharingSession::open(&root, &path_a, owner_a).unwrap();
    let (b, mut project_b) = SharingSession::open(&root, &path_b, owner_b.clone()).unwrap();
    assert_ne!(project_a.next_id, project_b.next_id);

    let flag = |project: &mut Project, id: ObjectId| {
        project
            .elements
            .get_mut(&id)
            .unwrap()
            .properties
            .insert("Проверено".into(), PropertyValue::Boolean(true));
    };
    flag(&mut project_a, ids[0]);
    flag(&mut project_b, ids[1]);
    open_bim_persistence::save_atomic(&path_b, &project_b).unwrap();
    drop(b);
    let offline = tmp.path().join("Отключённая папка");
    std::fs::rename(&root, &offline).unwrap();
    let mut b = SharingSession::resume(&path_b, owner_b).unwrap().unwrap();
    assert_eq!(b.base_revision(), 1);
    assert!(b.head_revision().is_err());
    project_b = open_bim_persistence::load(&path_b).unwrap();
    std::fs::rename(&offline, &root).unwrap();

    let published_a = a.publish(&project_a, "Изменение А").unwrap();
    assert_eq!(a.base_revision(), 2);
    assert_eq!(b.head_revision().unwrap(), 2);
    let published_b = b.publish(&project_b, "Изменение Б").unwrap();
    let updated_a = a.update(&published_a).unwrap();
    assert_eq!(a.base_revision(), 3);
    assert_eq!(b.base_revision(), 3);
    let head = shared.checkout().unwrap().project;
    assert_eq!(updated_a.elements, head.elements);
    assert_eq!(published_b.elements, head.elements);
    for id in ids {
        assert_eq!(
            head.elements[&id].properties["Проверено"],
            PropertyValue::Boolean(true)
        );
    }
}
