use open_bim_model::*;
use open_bim_sharing::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "open-bim-sharing-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn shared() -> (Temp, SharedModel) {
    let temp = Temp::new();
    let model = SharedModel::create(
        &temp.0,
        &example_steel_frame().unwrap(),
        &Owner::new("Иванов", "mac-1"),
        "Начало",
    )
    .unwrap();
    (temp, model)
}

fn element_ids(project: &Project) -> Vec<ObjectId> {
    project.elements.keys().copied().collect()
}

fn shift(project: &mut Project, id: ObjectId, dz: f64) {
    project
        .elements
        .get_mut(&id)
        .unwrap()
        .shape
        .frame_mut()
        .origin
        .0[2] += dz;
}

fn add_level(project: &mut Project, name: &str) -> ObjectId {
    let id = project.allocate_id().unwrap();
    project.levels.insert(
        id,
        Level {
            id,
            name: name.into(),
            elevation_mm: 3000.,
        },
    );
    id
}

/// An engineer joins: takes the head and an ID range of their own.
fn join(model: &SharedModel, owner: &Owner) -> (Checkout, Project) {
    let base = model.checkout().unwrap();
    let mut mine = base.project.clone();
    model.claim_id_block(owner).unwrap().adopt(&mut mine);
    (base, mine)
}

fn none() -> BTreeMap<Unit, Resolution> {
    BTreeMap::new()
}

#[test]
fn created_folder_opens_with_its_first_revision() {
    let (temp, _) = shared();
    let model = SharedModel::open(&temp.0).unwrap();
    let head = model.checkout().unwrap();
    assert_eq!(head.revision.number, 1);
    assert_eq!(head.revision.comment, "Начало");
    assert_eq!(head.project, example_steel_frame().unwrap());
    assert_eq!(model.history().unwrap().len(), 1);
    assert!(SharedModel::create(&temp.0, &head.project, &head.revision.author, "").is_err());
    assert!(SharedModel::open(std::env::temp_dir().join("open-bim-no-such-folder")).is_err());
}

#[test]
fn edits_of_different_objects_from_one_base_both_reach_the_model() {
    let (_temp, model) = shared();
    let (a, b) = (
        Owner::new("Иванов", "mac-1"),
        Owner::new("Петрова", "mac-2"),
    );
    let (base_a, mut mine_a) = join(&model, &a);
    let (base_b, mut mine_b) = join(&model, &b);
    let ids = element_ids(&mine_a);
    shift(&mut mine_a, ids[0], 100.);
    let level = add_level(&mut mine_b, "Отм. +3.000");
    shift(&mut mine_b, ids[1], -50.);

    let first = model
        .publish(&a, &base_a, &mine_a, "Колонна", &none())
        .unwrap();
    assert!(first.written);
    let second = model
        .publish(&b, &base_b, &mine_b, "Отметка", &none())
        .unwrap();
    assert_eq!(second.base.revision.number, 3);

    let head = model.checkout().unwrap().project;
    assert_eq!(head.elements[&ids[0]], mine_a.elements[&ids[0]]);
    assert_eq!(head.elements[&ids[1]], mine_b.elements[&ids[1]]);
    assert_eq!(head.levels[&level].name, "Отм. +3.000");
    assert!(level.0 >= 2 * ID_BLOCK, "Петрова creates in her own range");
    let history = model.history().unwrap();
    assert_eq!(
        history
            .iter()
            .map(|r| r.author.user.as_str())
            .collect::<Vec<_>>(),
        ["Иванов", "Иванов", "Петрова"]
    );
    assert_eq!(history[1].changed, [format!("id-{}", ids[0])]);
    assert!(history[2].changed.contains(&format!("id-{level}")));
}

#[test]
fn nothing_to_save_writes_no_revision() {
    let (_temp, model) = shared();
    let owner = Owner::new("Иванов", "mac-1");
    let (base, mine) = join(&model, &owner);
    let published = model.publish(&owner, &base, &mine, "", &none()).unwrap();
    assert!(!published.written);
    assert_eq!(model.head().unwrap().number, 1);
}

#[test]
fn same_object_changed_twice_is_a_conflict_until_a_side_is_chosen() {
    let (_temp, model) = shared();
    let (a, b) = (
        Owner::new("Иванов", "mac-1"),
        Owner::new("Петрова", "mac-2"),
    );
    let (base_a, mut mine_a) = join(&model, &a);
    let (base_b, mut mine_b) = join(&model, &b);
    let id = element_ids(&mine_a)[0];
    shift(&mut mine_a, id, 100.);
    shift(&mut mine_b, id, 200.);
    model.publish(&a, &base_a, &mine_a, "", &none()).unwrap();

    let error = model
        .publish(&b, &base_b, &mine_b, "", &none())
        .unwrap_err();
    let SharingError::Conflicts(conflicts) = &error else {
        panic!("{error}");
    };
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].unit.lock_name(), format!("id-{id}"));
    assert!(error.to_string().contains(&format!("объект {id}")));
    assert_eq!(model.head().unwrap().number, 2);

    let mine = BTreeMap::from([(conflicts[0].unit.clone(), Resolution::Mine)]);
    model.publish(&b, &base_b, &mine_b, "", &mine).unwrap();
    assert_eq!(
        model.checkout().unwrap().project.elements[&id],
        mine_b.elements[&id]
    );
}

#[test]
fn locked_objects_refuse_other_engineers() {
    let (_temp, model) = shared();
    let (a, b) = (
        Owner::new("Иванов", "mac-1"),
        Owner::new("Петрова", "mac-2"),
    );
    let (base_a, mut mine_a) = join(&model, &a);
    let (base_b, mut mine_b) = join(&model, &b);
    let id = element_ids(&mine_a)[0];
    let unit = Unit {
        field: "elements".into(),
        key: Some(id.to_string()),
    };
    let units = BTreeSet::from([unit.clone()]);
    model.acquire(&a, &base_a, &units).unwrap();
    model.acquire(&a, &base_a, &units).unwrap();
    assert_eq!(model.locks().unwrap()[0].owner.user, "Иванов");

    let Err(SharingError::Locked(locks)) = model.acquire(&b, &base_b, &units) else {
        panic!("Петрова must not take a held lock");
    };
    assert_eq!(locks[0].owner.user, "Иванов");
    // A part's swept parameters share its lock.
    let swept = Unit {
        field: "swept_parts".into(),
        key: Some(id.to_string()),
    };
    assert_eq!(
        model.blocked(&b, &BTreeSet::from([swept])).unwrap().len(),
        1
    );

    shift(&mut mine_b, id, 10.);
    assert!(matches!(
        model.publish(&b, &base_b, &mine_b, "", &none()),
        Err(SharingError::Locked(_))
    ));
    shift(&mut mine_a, id, 30.);
    model.publish(&a, &base_a, &mine_a, "", &none()).unwrap();
    model.release(&b, &units).unwrap();
    assert_eq!(model.locks().unwrap().len(), 1, "only the owner releases");
    model.release_all(&a).unwrap();
    assert!(model.locks().unwrap().is_empty());

    // Петрова's copy is older than Иванов's saved change of that part.
    assert!(matches!(
        model.acquire(&b, &base_b, &units),
        Err(SharingError::OutOfDate(_))
    ));
    let updated = model.update(&base_b, &base_b.project, &none()).unwrap();
    model.acquire(&b, &updated.base, &units).unwrap();
    model.force_release(&unit.lock_name()).unwrap();
    assert!(model.locks().unwrap().is_empty());
}

#[test]
fn update_brings_shared_changes_and_keeps_local_edits() {
    let (_temp, model) = shared();
    let (a, b) = (
        Owner::new("Иванов", "mac-1"),
        Owner::new("Петрова", "mac-2"),
    );
    let (base_a, mut mine_a) = join(&model, &a);
    let (base_b, mut mine_b) = join(&model, &b);
    let ids = element_ids(&mine_a);
    let level_a = add_level(&mut mine_a, "А");
    model.publish(&a, &base_a, &mine_a, "", &none()).unwrap();

    shift(&mut mine_b, ids[1], 5.);
    let updated = model.update(&base_b, &mine_b, &none()).unwrap();
    assert_eq!(updated.base.revision.number, 2);
    assert!(updated.project.levels.contains_key(&level_a));
    assert_eq!(updated.project.elements[&ids[1]], mine_b.elements[&ids[1]]);
    assert_eq!(updated.project.next_id, mine_b.next_id);

    let mut next = updated.project.clone();
    let level_b = add_level(&mut next, "Б");
    assert_eq!(level_b.0, mine_b.next_id, "Петрова continues in her range");
    assert!(next.validate().is_ok());
    model
        .publish(&b, &updated.base, &next, "", &none())
        .unwrap();
    let head = model.checkout().unwrap().project;
    assert!(head.levels.contains_key(&level_a) && head.levels.contains_key(&level_b));
}

#[test]
fn merge_that_breaks_references_is_refused() {
    let (_temp, model) = shared();
    let (a, b) = (
        Owner::new("Иванов", "mac-1"),
        Owner::new("Петрова", "mac-2"),
    );
    let (base_a, mut mine_a) = join(&model, &a);
    let (base_b, mut mine_b) = join(&model, &b);
    // A part nothing else refers to, so deleting it alone is valid.
    let id = element_ids(&mine_a)
        .into_iter()
        .find(|id| {
            let mut copy = mine_a.clone();
            copy.elements.remove(id);
            copy.validate().is_ok()
        })
        .unwrap();
    mine_a.elements.remove(&id);
    let assembly = mine_b.allocate_id().unwrap();
    mine_b.assemblies.insert(
        assembly,
        Assembly {
            id: assembly,
            name: "Колонна".into(),
            main_part: id,
            members: BTreeSet::from([id]),
            children: BTreeSet::new(),
            properties: BTreeMap::new(),
        },
    );
    mine_b.validate().unwrap();
    model.publish(&a, &base_a, &mine_a, "", &none()).unwrap();
    let error = model
        .publish(&b, &base_b, &mine_b, "", &none())
        .unwrap_err();
    assert!(matches!(error, SharingError::InvalidMerge(_)), "{error}");
    assert_eq!(model.head().unwrap().number, 2);
}

#[test]
fn simultaneous_saves_wait_for_each_other_and_lose_nothing() {
    let (_temp, model) = shared();
    let engineers = 8;
    let levels: Vec<ObjectId> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..engineers)
            .map(|i| {
                let model = model.clone();
                scope.spawn(move || {
                    let owner = Owner::new(format!("Инженер {i}"), "host");
                    let (base, mut mine) = join(&model, &owner);
                    let level = add_level(&mut mine, &format!("Уровень {i}"));
                    model.publish(&owner, &base, &mine, "", &none()).unwrap();
                    level
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let head = model.checkout().unwrap();
    assert_eq!(head.revision.number, 1 + engineers as u64);
    for level in levels {
        assert!(head.project.levels.contains_key(&level));
    }
    assert!(!model.root().join("locks/save.lock").exists());
}

#[test]
fn save_lock_waits_for_a_live_save_and_takes_over_a_stale_one() {
    let (_temp, model) = shared();
    let owner = Owner::new("Иванов", "mac-1");
    let (base, mut mine) = join(&model, &owner);
    add_level(&mut mine, "А");
    let lock = model.root().join("locks/save.lock");
    let mut other = Owner::new("Петрова", "mac-2");
    std::fs::write(&lock, serde_json::to_vec(&other).unwrap()).unwrap();
    let error = model
        .publish_waiting(
            &owner,
            &base,
            &mine,
            "",
            &none(),
            Duration::from_millis(200),
        )
        .unwrap_err();
    assert!(
        matches!(&error, SharingError::Busy(o) if o.user == "Петрова"),
        "{error}"
    );

    other.since = 0;
    std::fs::write(&lock, serde_json::to_vec(&other).unwrap()).unwrap();
    model.publish(&owner, &base, &mine, "", &none()).unwrap();
    assert!(!lock.exists());
}

#[test]
fn id_ranges_are_never_handed_out_twice() {
    let (_temp, model) = shared();
    let blocks: BTreeSet<u64> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let model = model.clone();
                scope.spawn(move || {
                    model
                        .claim_id_block(&Owner::new(format!("{i}"), "host"))
                        .unwrap()
                        .start
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(blocks.len(), 16);
    assert!(
        blocks
            .iter()
            .all(|start| start % ID_BLOCK == 0 && *start > 0)
    );
}

#[test]
fn ids_of_other_ranges_may_lie_above_the_counter() {
    let mut project = example_steel_frame().unwrap();
    let start = project.next_id;
    project.next_id = 3 * ID_BLOCK;
    let foreign = add_level(&mut project, "Чужой");
    project.next_id = ID_BLOCK + 10;
    assert!(project.validate().is_ok());
    let own = ObjectId(ID_BLOCK + 20);
    project.levels.insert(
        own,
        Level {
            id: own,
            name: "Свой выше счётчика".into(),
            elevation_mm: 0.,
        },
    );
    assert!(project.validate().is_err());
    project.levels.remove(&own);
    project.next_id = start;
    assert!(project.validate().is_ok(), "{foreign} is in another range");
}

#[test]
fn every_project_field_is_a_unit() {
    let project = example_steel_frame().unwrap();
    let mut changed = project.clone();
    changed.name = "Другой".into();
    changed.grid.magnetic = !changed.grid.magnetic;
    changed.revision += 5;
    let units = changed_units(&project, &changed).unwrap();
    let names: Vec<String> = units.iter().map(Unit::lock_name).collect();
    assert_eq!(names, ["field-grid", "field-name"]);
    let family = Unit {
        field: "families".into(),
        key: Some("Балка".into()),
    };
    assert!(
        family
            .lock_name()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    );
}
