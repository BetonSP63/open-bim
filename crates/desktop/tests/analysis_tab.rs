//! «Анализ» tab: models, load groups, loads placed by picks, nodes, rigid
//! links, merged nodes and part analysis properties, each one undo step.
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{LoadKind, LoadShape, PartRole};
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
fn loads_nodes_links_by_picks() {
    let mut b = bench();
    let before = open_bim_analysis::build(
        b.editor.document.project(),
        &open_bim_desktop::interaction::analysis::model(b.editor.document.project(), None),
    );
    assert!(!before.bars.is_empty());
    // A point load: the first group is made on the fly.
    b.execute(Cmd::AddLoad);
    assert!(b.analysis.show && b.analysis.load_open);
    let r = b.pick(DVec3::new(0., 0., 3000.));
    commit(&mut b, r);
    let a = &b.editor.document.project().analysis;
    assert_eq!(a.load_groups.len(), 1);
    assert_eq!(a.load_groups[0].kind, LoadKind::Dead);
    assert!(matches!(
        a.loads[0].shape,
        LoadShape::Point {
            force: [0., 0., -10.],
            ..
        }
    ));
    // An area load closed with Enter.
    b.analysis.form = open_bim_desktop::ui::analysis_ui::LoadForm::Area;
    b.start_load();
    for p in [[0., 0., 3000.], [6000., 0., 3000.], [6000., 6000., 3000.]] {
        assert!(b.pick(DVec3::from_array(p)).is_none());
    }
    let r = b.execute(Cmd::FinishPicks);
    commit(&mut b, r);
    assert_eq!(b.editor.document.project().analysis.loads.len(), 2);
    // A user node, then a rigid link from it to a free point.
    b.execute(Cmd::AnalysisNode);
    let r = b.pick(DVec3::new(-3000., 0., 0.));
    commit(&mut b, r);
    let a = b.analytical();
    assert_eq!(a.nodes.len(), before.nodes.len() + 1);
    b.execute(Cmd::RigidLink);
    b.pick(DVec3::new(-3000., 0., 0.));
    let first_foot = before.nodes[0];
    let r = b.pick(DVec3::from_array(first_foot) + DVec3::new(30., 0., 0.));
    commit(&mut b, r);
    assert_eq!(b.analytical().links.len(), 1);
    // Merging the user node into that foot removes the link (one node).
    b.execute(Cmd::MergeNodes);
    b.pick(DVec3::from_array(first_foot));
    let r = b.pick(DVec3::new(-3000., 0., 0.));
    commit(&mut b, r);
    let a = b.analytical();
    assert_eq!(a.nodes.len(), before.nodes.len());
    assert!(a.links.is_empty());
    // Undo brings the link back.
    b.editor.history(false).unwrap();
    assert_eq!(b.analytical().links.len(), 1);
}

#[test]
fn part_properties_reset_and_copy() {
    let mut b = bench();
    let project = b.editor.document.project();
    let ids: Vec<_> = project.elements.keys().copied().take(3).collect();
    b.editor.selected = [ids[0]].into();
    b.analysis.part.role = PartRole::Truss;
    let props = b.analysis.part.clone();
    let commands = open_bim_desktop::interaction::analysis::set_parts(
        b.editor.document.project(),
        &b.editor.selected,
        &props,
    )
    .unwrap();
    b.editor.commit("Свойства", &commands).unwrap();
    // «Получить расчётные свойства»: the others take those of the first.
    b.editor.selected = [ids[1], ids[2]].into();
    b.execute(Cmd::CopyAnalysis);
    let r = b.pick_object(Some(ids[0]));
    commit(&mut b, r);
    let parts = &b.editor.document.project().analysis.parts;
    assert!(ids.iter().all(|id| parts[id].role == PartRole::Truss));
    // «Сбросить изменения».
    b.editor.selected = [ids[1]].into();
    let r = b.execute(Cmd::ResetAnalysis);
    commit(&mut b, r);
    assert!(
        !b.editor
            .document
            .project()
            .analysis
            .parts
            .contains_key(&ids[1])
    );
    // Numbers toggle and switch the model display on.
    b.analysis.show = false;
    b.execute(Cmd::NodeNumbers);
    assert!(b.analysis.node_numbers && b.analysis.show);
}

#[test]
fn models_and_groups() {
    let b = bench();
    let p = b.editor.document.project();
    let (commands, n) = open_bim_desktop::interaction::analysis::add_model(p).unwrap();
    assert_eq!(n, 1);
    let mut e = Editor::new(p.clone()).unwrap();
    e.commit("М", &commands).unwrap();
    let (commands, g) =
        open_bim_desktop::interaction::analysis::add_group(e.document.project(), LoadKind::Snow)
            .unwrap();
    e.commit("Г", &commands).unwrap();
    let group = &e.document.project().analysis.load_groups[0];
    assert_eq!((group.number, group.factor), (g, 1.4));
    let mut m = e.document.project().analysis.models[0].clone();
    m.load_groups.insert(g);
    let commands =
        open_bim_desktop::interaction::analysis::put_model(e.document.project(), m).unwrap();
    e.commit("М", &commands).unwrap();
    // Removing the group removes it from the model too.
    let commands =
        open_bim_desktop::interaction::analysis::remove_group(e.document.project(), g).unwrap();
    e.commit("Г", &commands).unwrap();
    assert!(
        e.document.project().analysis.models[0]
            .load_groups
            .is_empty()
    );
    // Saved and read back.
    let json = serde_json::to_string(e.document.project()).unwrap();
    let back: open_bim_model::Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.analysis, e.document.project().analysis);
}
