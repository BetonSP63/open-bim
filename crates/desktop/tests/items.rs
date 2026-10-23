//! Shapes and items: «Создать форму из геометрии», «Преобразовать деталь в
//! элемент», «Элемент» and «Добавленный материал».
use open_bim_desktop::interaction::items;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{ObjectId, PropertyValue};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(Editor::new(open_bim_model::example_steel_frame().unwrap()).unwrap())
}

fn commit(b: &mut Workbench, r: Option<Request>) {
    match r {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("no commit: {:?}", b.tools.message),
    }
}

#[test]
fn shapes_and_items() {
    let mut b = bench();
    // An empty catalogue: «Элемент» explains and opens the window.
    assert!(b.execute(Cmd::Item).is_none());
    assert!(b.shapes.open);
    let ids: Vec<ObjectId> = b
        .editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .take(2)
        .collect();
    b.editor.selected = ids.iter().copied().collect();
    let p = b.editor.document.project();
    let lows: Vec<[f64; 3]> = ids
        .iter()
        .map(|id| p.element_bounds(*id).unwrap()[0])
        .collect();
    let base: [f64; 3] =
        std::array::from_fn(|k| lows.iter().map(|l| l[k]).fold(f64::MAX, f64::min));
    let r = b.execute(Cmd::CreateShape);
    commit(&mut b, r);
    let shape = &b.editor.document.project().shapes.shapes[0];
    assert_eq!((shape.name.as_str(), shape.parts.len()), ("Форма 1", 2));
    // Placing it at a point: the copies keep their layout relative to the base.
    let before = b.editor.document.project().elements.len();
    b.execute(Cmd::Item);
    let at = DVec3::new(20_000., 0., 0.);
    let r = b.pick(at);
    commit(&mut b, r);
    let p = b.editor.document.project();
    assert_eq!(p.elements.len(), before + 2);
    let item = p
        .assemblies
        .values()
        .find(|a| a.properties.get(items::ITEM) == Some(&PropertyValue::Text("Форма 1".into())))
        .expect("item");
    assert_eq!(item.members.len(), 2);
    let mut new_lows: Vec<[f64; 3]> = item
        .members
        .iter()
        .map(|id| p.element_bounds(*id).unwrap()[0])
        .collect();
    let mut expected: Vec<[f64; 3]> = lows
        .iter()
        .map(|l| std::array::from_fn(|k| l[k] - base[k] + at.to_array()[k]))
        .collect();
    let key = |v: &[f64; 3]| (v[0] * 1000.).round() as i64;
    new_lows.sort_by_key(key);
    expected.sort_by_key(key);
    for (a, e) in new_lows.iter().zip(&expected) {
        assert!((0..3).all(|k| (a[k] - e[k]).abs() < 1e-6), "{a:?} {e:?}");
    }
    // Converting parts makes «Форма 2» and an item of them.
    b.editor.selected = [ids[0]].into();
    let r = b.execute(Cmd::ConvertToItem);
    commit(&mut b, r);
    let p = b.editor.document.project();
    assert!(p.shapes.get("Форма 2").is_some());
    assert!(p.assemblies.values().any(|a| a.members.contains(&ids[0])
        && a.properties.get(items::ITEM) == Some(&PropertyValue::Text("Форма 2".into()))));
    // Rename and remove; undo restores.
    let c = items::rename(p, "Форма 2", "Опора").unwrap();
    b.editor.commit("Имя", &c).unwrap();
    assert!(items::rename(b.editor.document.project(), "Опора", "Форма 1").is_err());
    let c = items::remove(b.editor.document.project(), "Опора").unwrap();
    b.editor.commit("Удалить", &c).unwrap();
    assert_eq!(b.editor.document.project().shapes.shapes.len(), 1);
    b.editor.history(false).unwrap();
    assert_eq!(b.editor.document.project().shapes.shapes.len(), 2);
}

#[test]
fn added_material_joins_the_host() {
    let mut b = bench();
    let host = *b.editor.document.project().elements.keys().next().unwrap();
    b.execute(Cmd::AddedMaterial);
    assert!(b.pick_object(Some(host)).is_none());
    for p in [[0., 0., 0.], [100., 0., 0.], [100., 100., 0.]] {
        assert!(b.pick(DVec3::from_array(p)).is_none());
    }
    let before = b.editor.document.project().elements.len();
    let r = b.execute(Cmd::FinishPicks);
    commit(&mut b, r);
    let p = b.editor.document.project();
    assert_eq!(p.elements.len(), before + 1);
    let added = p
        .elements
        .values()
        .find(|e| e.properties.contains_key(items::ADDED))
        .unwrap();
    assert_eq!(added.material, p.elements[&host].material);
    assert_eq!(
        added.properties[items::ADDED],
        PropertyValue::Number(host.0 as f64)
    );
    assert!(
        p.assemblies
            .values()
            .any(|a| a.members.contains(&host) && a.members.contains(&added.id))
    );
}
