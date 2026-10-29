//! Welds by the Russian standards in the app: a joint from the standard's
//! table, a welding method, an intermittent weld; the weld pane fills its
//! fields from a selected weld and changes it; lists count the welded length.
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{DetailKind, ObjectId, WeldProcess, WeldSides, WeldType};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(
        Editor::new(with_default_grid(open_bim_model::Project::default()).unwrap()).unwrap(),
    )
}

fn apply(b: &mut Workbench, request: Option<Request>) {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("no commit"),
    }
}

fn member(b: &mut Workbench, tool: Cmd, points: &[DVec3]) -> ObjectId {
    b.execute(tool);
    let mut r = None;
    for q in points {
        r = b.pick(*q);
    }
    apply(b, r);
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn weld(b: &Workbench) -> (ObjectId, DetailKind) {
    let (id, d) = b
        .editor
        .document
        .project()
        .details
        .iter()
        .find(|(_, d)| matches!(d.kind, DetailKind::Weld { .. }))
        .unwrap();
    (*id, d.kind.clone())
}

#[test]
fn gost_weld_is_created_changed_and_listed() {
    let mut b = bench();
    let column = member(&mut b, Cmd::ToolColumn, &[DVec3::new(0., 0., 0.)]);
    let beam = member(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)],
    );
    let d = &mut b.tools.detailing;
    d.weld_process = WeldProcess::GasShielded;
    d.weld_type = WeldType::Fillet;
    d.weld_sides = WeldSides::Both;
    d.weld_method = "УП".into();
    d.weld_size = "5".into();
    b.execute(Cmd::ToolWeld);
    assert!(b.pick_object(Some(column)).is_none());
    let r = b.pick_object(Some(beam));
    apply(&mut b, r);
    b.execute(Cmd::CancelTool);
    let (id, kind) = weld(&b);
    assert_eq!(kind.weld_mark().unwrap(), "ГОСТ 14771-76-Т3-УП-⊿5");

    // The pane loads the weld, the engineer picks Т8 from the table and an
    // intermittent staggered weld; «Изменить» replaces the weld.
    let mut d = open_bim_desktop::tools::detailing_ui::DetailingUi::default();
    d.load_weld(&kind);
    assert_eq!((d.weld_method.as_str(), d.weld_size.as_str()), ("УП", "5"));
    d.weld_joint = "Т3".into();
    d.weld_intermittent = true;
    d.weld_segment = "50".into();
    d.weld_pitch = "100".into();
    d.weld_staggered = true;
    let changed = d.apply_weld(&kind).unwrap();
    assert_eq!(
        changed.weld_mark().unwrap(),
        "ГОСТ 14771-76-Т3-УП-⊿5-50Z100"
    );
    b.editor
        .commit(
            "Изменить швы",
            &[open_bim_commands::Command::ReplaceDetail {
                detail: open_bim_model::Detail { id, kind: changed },
            }],
        )
        .unwrap();
    let (_, kind) = weld(&b);
    let (length, welded) = kind.weld_lengths().unwrap();
    assert!(welded < length);
    let r = open_bim_reports::generate(b.editor.document.project()).unwrap();
    let row = r.fasteners.iter().find(|f| f.kind == "Сварка").unwrap();
    assert_eq!(
        row.description,
        "Шов угловой ГОСТ 14771-76 Т3 УП 50Z100 k=5 заводской"
    );
    assert!((row.length_mm - (welded * 10.).round() / 10.).abs() < 0.11);

    // A bad pitch is refused with a plain message.
    d.weld_pitch = "40".into();
    assert!(d.apply_weld(&kind).unwrap_err().contains("Шаг"));
}
