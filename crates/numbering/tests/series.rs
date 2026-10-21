//! Numbering series (prefix + start) and keeping the marks of the last run.
use open_bim_model::{
    ElementShape, LengthMm, MemberKind, ObjectId, Project, PropertyValue, example_steel_frame,
};
use open_bim_numbering::{
    SERIES_PREFIX, SERIES_START, number, outdated, renumber_all, stored_marks,
};
use std::collections::BTreeSet;

fn ids(p: &Project, kind: MemberKind) -> Vec<ObjectId> {
    p.elements
        .values()
        .filter(|e| matches!(&e.shape, ElementShape::Member { member_kind, .. } if *member_kind == kind))
        .map(|e| e.id)
        .collect()
}

fn set_series(p: &mut Project, id: ObjectId, prefix: &str, start: f64) {
    let e = p.elements.get_mut(&id).unwrap();
    e.properties
        .insert(SERIES_PREFIX.into(), PropertyValue::Text(prefix.into()));
    e.properties
        .insert(SERIES_START.into(), PropertyValue::Number(start));
}

fn lengthen(p: &mut Project, id: ObjectId, by: f64) {
    if let ElementShape::Member { length, .. } = &mut p.elements.get_mut(&id).unwrap().shape {
        *length = LengthMm::new(length.value() + by).unwrap();
    }
}

#[test]
fn series_prefix_and_start_number_within_the_series() {
    let mut p = example_steel_frame().unwrap();
    let columns = ids(&p, MemberKind::Column);
    assert!(columns.len() >= 2);
    let plain = number(&p).unwrap();
    // Default series: plain numbers from 1.
    assert!(plain.parts.values().all(|m| m.parse::<u64>().is_ok()));
    assert!(plain.parts.values().any(|m| m == "1"));
    for id in &columns {
        set_series(&mut p, *id, "К", 101.);
    }
    let n = number(&p).unwrap();
    let marks: BTreeSet<_> = columns.iter().map(|id| n.parts[id].clone()).collect();
    assert_eq!(
        marks,
        BTreeSet::from(["К101".to_string()]),
        "equal columns share a mark"
    );
    // The other parts keep a series of their own, numbered from 1.
    let others: BTreeSet<_> = n
        .parts
        .iter()
        .filter(|(id, _)| !columns.contains(id))
        .map(|(_, m)| m.clone())
        .collect();
    assert!(others.contains("1"), "{others:?}");
    assert!(others.iter().all(|m| m.parse::<u64>().is_ok()));
}

#[test]
fn stored_marks_are_kept_changed_parts_get_new_numbers() {
    let mut p = example_steel_frame().unwrap();
    let columns = ids(&p, MemberKind::Column);
    let first = number(&p).unwrap();
    p.numbering = stored_marks(&first);
    let highest = first
        .parts
        .values()
        .map(|m| m.parse::<u64>().unwrap())
        .max()
        .unwrap();
    assert!(outdated(&p, &number(&p).unwrap()).is_empty());

    // One of several equal columns changes: it gets the next free number,
    // the others keep theirs.
    lengthen(&mut p, columns[0], 100.);
    let changed = number(&p).unwrap();
    assert_eq!(changed.parts[&columns[0]], (highest + 1).to_string());
    for id in &columns[1..] {
        assert_eq!(changed.parts[id], first.parts[id]);
    }
    for (id, mark) in &first.parts {
        if *id != columns[0] {
            assert_eq!(&changed.parts[id], mark, "unchanged parts keep marks");
        }
    }
    assert!(outdated(&p, &changed).contains(&columns[0]));

    // Changed back: the column rejoins its group and its old mark.
    lengthen(&mut p, columns[0], -100.);
    assert_eq!(number(&p).unwrap().parts, first.parts);

    // A part alone in its group keeps its number when it changes.
    lengthen(&mut p, columns[0], 100.);
    p.numbering = stored_marks(&number(&p).unwrap());
    let alone = number(&p).unwrap().parts[&columns[0]].clone();
    lengthen(&mut p, columns[0], 50.);
    assert_eq!(number(&p).unwrap().parts[&columns[0]], alone);

    // «Перенумеровать всё» ignores the stored marks: numbers from 1 again.
    let fresh = renumber_all(&p).unwrap();
    let mut numbers: Vec<u64> = fresh
        .parts
        .values()
        .map(|m| m.parse().unwrap())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    numbers.sort();
    assert_eq!(numbers, (1..=numbers.len() as u64).collect::<Vec<_>>());
}

#[test]
fn stored_marks_survive_save_and_reopen() {
    let mut p = example_steel_frame().unwrap();
    p.numbering = stored_marks(&number(&p).unwrap());
    let reopened = Project::from_json(&p.to_json().unwrap()).unwrap();
    assert_eq!(reopened.numbering, p.numbering);
    assert_eq!(number(&reopened).unwrap().parts, number(&p).unwrap().parts);
}
