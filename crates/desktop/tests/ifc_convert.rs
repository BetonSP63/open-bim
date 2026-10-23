//! «Преобразовать объекты IFC»: chosen kinds become native parts, objects
//! already in the model (by IFC GUID) are skipped.
use open_bim_desktop::interaction::ifc_convert::{KINDS, census, convert, kind};
use open_bim_desktop::tools::Editor;
use open_bim_model::Project;
use std::collections::BTreeSet;

fn file() -> Project {
    let frame = open_bim_model::example_steel_frame().unwrap();
    let (text, _) = open_bim_ifc::export(&frame).unwrap();
    open_bim_ifc::import(&text).unwrap().0
}

#[test]
fn chosen_kinds_convert_once_and_repeat_skips_them() {
    let imported = file();
    let mut editor = Editor::new(open_bim_model::example_steel_frame().unwrap()).unwrap();
    // The model has no GUIDs yet, so nothing is known.
    let counts = census(editor.document.project(), &imported);
    let columns = counts[KINDS[0]].0;
    assert!(columns > 0 && counts[KINDS[0]].1 == 0);
    let before = editor.document.project().elements.len();

    let c = convert(
        editor.document.project(),
        &imported,
        &BTreeSet::from([KINDS[0]]),
    )
    .unwrap();
    assert_eq!((c.parts.len(), c.skipped), (columns, 0));
    editor.commit("IFC", &c.commands).unwrap();
    let p = editor.document.project();
    assert_eq!(p.elements.len(), before + columns);
    assert!(c.parts.iter().all(|id| kind(&p.elements[id]) == KINDS[0]));
    p.validate().unwrap();

    // Converting again: the columns are known by GUID and skipped.
    assert_eq!(census(p, &imported)[KINDS[0]].1, columns);
    let all: BTreeSet<&str> = KINDS.into_iter().collect();
    let c = convert(p, &imported, &all).unwrap();
    assert_eq!(c.skipped, columns);
    assert_eq!(c.parts.len(), imported.elements.len() - columns);
    editor.commit("IFC", &c.commands).unwrap();
    editor.document.project().validate().unwrap();
    assert!(convert(editor.document.project(), &imported, &all).is_err());
    assert!(convert(editor.document.project(), &imported, &BTreeSet::new()).is_err());
}
