//! Property files («Сохранить как» / «Загрузить», «standard» at the
//! start of a command) and columns by levels that follow a level's new mark.
use open_bim_desktop::interaction::grid_edit::{spec_from_project, update_commands};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::column_levels::{BOTTOM_LEVEL, TOP_LEVEL};
use open_bim_desktop::tools::property_files::{
    self as files, FileKind, LevelValue, MemberValues, Values,
};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::property_pane::{PropertyPane, common};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{AtDepth, ObjectId, OnPlane, PartPosition, Point3, Project};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// A bench whose project file sits in a fresh temporary folder, so the
/// `attributes` folder is private to the test.
fn bench(test: &str) -> (Workbench, PathBuf) {
    let dir = std::env::temp_dir().join(format!("open-bim-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    b.tools.path = dir.join("project.obim").to_string_lossy().into();
    (b, dir.join("attributes"))
}

fn level(project: &Project, z: f64) -> ObjectId {
    project
        .levels
        .values()
        .find(|l| l.elevation_mm == z)
        .unwrap()
        .id
}

fn create(b: &mut Workbench, cmd: Cmd, points: &[DVec3]) -> ObjectId {
    b.execute(cmd);
    let mut request = None;
    for p in points {
        request = b.pick(*p);
    }
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("no part: {:?}", b.tools.message),
    }
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn line(project: &Project, id: ObjectId) -> (Point3, Point3) {
    let (s, e, _) = project
        .member_reference_line(&project.elements[&id].shape)
        .unwrap();
    (s, e)
}

#[test]
fn saved_beam_file_loads_into_the_dialog_by_names() {
    let (mut b, dir) = bench("beam");
    b.execute(Cmd::ToolBeam);
    let project = b.editor.document.project().clone();
    let (profile, material) = (
        *project.profiles.keys().nth(1).unwrap(),
        *project.materials.keys().last().unwrap(),
    );
    let position = PartPosition {
        plane: OnPlane::Left,
        depth: AtDepth::Behind,
        depth_offset: -20.,
        ..Default::default()
    };
    b.editor.draft.name = "Б1".into();
    b.editor.draft.profile = profile;
    b.editor.draft.material = material;
    b.editor.draft.position = position;
    let saved = files::from_draft(&project, &b.editor.draft, FileKind::Beam);
    let file = files::save(&dir, "Б-перекрытие", &saved).unwrap();
    assert!(file.ends_with("attributes/Б-перекрытие.beam.json"));
    let json = std::fs::read_to_string(&file).unwrap();
    assert!(json.contains(&project.profiles[&profile].name), "{json}");
    assert_eq!(files::list(&dir, FileKind::Beam), vec!["Б-перекрытие"]);
    assert!(files::list(&dir, FileKind::Column).is_empty());

    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolBeam);
    assert_eq!(
        b.editor.draft.name, "Б1",
        "the dialog keeps its last values"
    );
    b.editor.draft = Default::default();
    let loaded = files::load(&dir, FileKind::Beam, "Б-перекрытие").unwrap();
    assert_eq!(loaded, saved);
    let warnings = files::apply_to_draft(&project, &mut b.editor.draft, &loaded);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(b.editor.draft.name, "Б1");
    assert_eq!(b.editor.draft.profile, profile);
    assert_eq!(b.editor.draft.material, material);
    assert_eq!(b.editor.draft.position, position);
    // A file of another kind is refused, a bad name too.
    assert!(files::load(&dir, FileKind::Column, "Б-перекрытие").is_err());
    assert!(files::save(&dir, "../x", &saved).is_err());
    assert!(files::save(&dir, " ", &saved).is_err());
}

#[test]
fn unknown_profile_keeps_the_current_one_and_is_reported() {
    let (mut b, _) = bench("unknown");
    b.execute(Cmd::ToolBeam);
    let project = b.editor.document.project().clone();
    let before = b.editor.draft.profile;
    let values = Values::Beam(MemberValues {
        profile: Some("Двутавр 100500".into()),
        name: Some("Б2".into()),
        ..Default::default()
    });
    let warnings = files::apply_to_draft(&project, &mut b.editor.draft, &values);
    assert_eq!(warnings, vec!["нет профиля «Двутавр 100500»"]);
    assert_eq!(b.editor.draft.profile, before);
    assert_eq!(b.editor.draft.name, "Б2", "the rest is applied");
}

#[test]
fn standard_file_is_loaded_when_the_command_starts() {
    let (mut b, dir) = bench("standard");
    let project = b.editor.document.project().clone();
    let profile = *project.profiles.keys().last().unwrap();
    let values = Values::Column(MemberValues {
        name: Some("К-стандарт".into()),
        profile: Some(project.profiles[&profile].name.clone()),
        ..Default::default()
    });
    files::save(&dir, files::STANDARD, &values).unwrap();
    b.execute(Cmd::ToolColumn);
    assert_eq!(b.editor.draft.name, "К-стандарт");
    assert_eq!(b.editor.draft.profile, profile);
    assert!(b.tools.message.as_deref().unwrap().contains("standard"));
    // Bolts too.
    files::save(
        &dir,
        files::STANDARD,
        &Values::Bolt(files::BoltValues {
            diameter: Some("24".into()),
            grade: Some("10.9".into()),
            ..Default::default()
        }),
    )
    .unwrap();
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolBolt);
    assert_eq!(b.tools.detailing.diameter, "24");
    assert_eq!(b.tools.detailing.grade, "10.9");
    let saved = files::from_bolts(&b.tools.detailing);
    files::save(&dir, "М24", &saved).unwrap();
    assert_eq!(files::list(&dir, FileKind::Bolt), vec!["standard", "М24"]);
}

#[test]
fn plate_file_keeps_thickness_material_and_position() {
    let (mut b, dir) = bench("plate");
    b.execute(Cmd::ToolPlate);
    let project = b.editor.document.project().clone();
    b.editor.draft.thickness = "16".into();
    b.editor.draft.plate_position = open_bim_desktop::interaction::polygon::PlatePosition::Down;
    files::save(
        &dir,
        "Пл16",
        &files::from_draft(&project, &b.editor.draft, FileKind::Plate),
    )
    .unwrap();
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolPlate);
    b.editor.draft.thickness = "20".into();
    let values = files::load(&dir, FileKind::Plate, "Пл16").unwrap();
    assert!(files::apply_to_draft(&project, &mut b.editor.draft, &values).is_empty());
    assert_eq!(b.editor.draft.thickness, "16");
    assert_eq!(
        b.editor.draft.plate_position,
        open_bim_desktop::interaction::polygon::PlatePosition::Down
    );
}

#[test]
fn column_by_levels_gets_its_ends_from_level_plus_offset() {
    let (mut b, dir) = bench("column");
    let project = b.editor.document.project().clone();
    let (l0, l1) = (level(&project, 0.), level(&project, 3600.));
    b.execute(Cmd::ToolColumn);
    b.editor.draft.bottom_level = Some(l0);
    b.editor.draft.bottom = "-150".into();
    b.editor.draft.top_level = Some(l1);
    b.editor.draft.top = "-300".into();
    let saved = files::from_draft(&project, &b.editor.draft, FileKind::Column);
    match &saved {
        Values::Column(v) => {
            assert_eq!(
                v.top,
                Some(LevelValue {
                    level: Some(project.levels[&l1].name.clone()),
                    value: -300.
                })
            );
        }
        _ => panic!(),
    }
    files::save(&dir, "К по уровням", &saved).unwrap();
    b.execute(Cmd::CancelTool);
    let id = {
        b.execute(Cmd::ToolColumn);
        b.editor.draft.bottom_level = None;
        b.editor.draft.top_level = None;
        let values = files::load(&dir, FileKind::Column, "К по уровням").unwrap();
        let project = b.editor.document.project().clone();
        assert!(files::apply_to_draft(&project, &mut b.editor.draft, &values).is_empty());
        assert_eq!(b.editor.draft.top_level, Some(l1));
        assert_eq!(b.editor.draft.top, "-300");
        let r = b.pick(DVec3::new(6000., 6000., 0.));
        match r {
            Some(Request::Action(Action::Commit(label, commands))) => {
                b.editor.commit(&label, &commands).unwrap();
            }
            _ => panic!("{:?}", b.tools.message),
        }
        b.execute(Cmd::CancelTool);
        *b.editor.document.project().elements.keys().last().unwrap()
    };
    let project = b.editor.document.project();
    let (s, e) = line(project, id);
    assert_eq!((s.0[2], e.0[2]), (-150., 3300.));
    assert!(project.elements[&id].properties.contains_key(BOTTOM_LEVEL));
    assert!(project.elements[&id].properties.contains_key(TOP_LEVEL));
    // A saved file of this column keeps its levels.
    match files::from_element(project, &project.elements[&id]).unwrap() {
        Values::Column(v) => assert_eq!(v.bottom.unwrap().value, -150.),
        _ => panic!(),
    }
}

#[test]
fn absolute_column_has_no_level_binding() {
    let (mut b, _) = bench("absolute");
    let id = create(&mut b, Cmd::ToolColumn, &[DVec3::new(0., 0., 0.)]);
    let project = b.editor.document.project();
    assert_eq!(line(project, id).1.0[2], 3600.);
    assert!(project.elements[&id].properties.is_empty());
}

#[test]
fn top_below_bottom_is_refused() {
    let (mut b, _) = bench("refused");
    let project = b.editor.document.project().clone();
    b.execute(Cmd::ToolColumn);
    b.editor.draft.bottom_level = Some(level(&project, 3600.));
    b.editor.draft.bottom = "0".into();
    b.editor.draft.top_level = Some(level(&project, 0.));
    b.editor.draft.top = "0".into();
    assert!(b.pick(DVec3::new(0., 0., 0.)).is_none());
    assert!(b.tools.message.as_deref().unwrap().contains("Верх"));
}

/// Two columns from 0 to +3600 with a −300 top offset: one stays on the
/// levels, the other is moved by hand to the absolute top 5000.
fn bound_columns(b: &mut Workbench) -> (ObjectId, ObjectId) {
    let project = b.editor.document.project().clone();
    let column = |b: &mut Workbench, x: f64| {
        b.execute(Cmd::ToolColumn);
        b.editor.draft.bottom_level = Some(level(&project, 0.));
        b.editor.draft.bottom = "0".into();
        b.editor.draft.top_level = Some(level(&project, 3600.));
        b.editor.draft.top = "-300".into();
        match b.pick(DVec3::new(x, 0., 0.)) {
            Some(Request::Action(Action::Commit(label, commands))) => {
                b.editor.commit(&label, &commands).unwrap();
            }
            _ => panic!("{:?}", b.tools.message),
        }
        b.execute(Cmd::CancelTool);
        *b.editor.document.project().elements.keys().last().unwrap()
    };
    let follows = column(b, 0.);
    let moved = column(b, 6000.);
    let project = b.editor.document.project();
    let mut e = project.elements[&moved].clone();
    e.shape = project
        .replace_member(
            &e.shape,
            Some((Point3([6000., 0., 0.]), Point3([6000., 0., 5000.]))),
            None,
            None,
        )
        .unwrap();
    b.editor
        .commit(
            "Переместить",
            &[open_bim_commands::Command::ReplaceElement { element: e }],
        )
        .unwrap();
    (follows, moved)
}

#[test]
fn bound_columns_follow_a_level_changed_in_the_grid_dialog() {
    let (mut b, _) = bench("grid");
    let (follows, moved) = bound_columns(&mut b);
    let project = b.editor.document.project().clone();
    let upper = level(&project, 3600.);
    let mut spec = spec_from_project(&project).unwrap();
    assert_eq!(spec.z, "0 3600");
    spec.z = "0 4200".into();
    let commands = update_commands(&project, &spec).unwrap();
    b.editor.commit("Изменить: сетка", &commands).unwrap();
    let after = b.editor.document.project();
    assert_eq!(
        after.levels[&upper].elevation_mm, 4200.,
        "the level keeps its ID"
    );
    assert_eq!(line(after, follows).1.0[2], 3900.);
    assert_eq!(line(after, follows).0.0[2], 0.);
    assert_eq!(
        line(after, moved).1.0[2],
        5000.,
        "moved off the level: stays"
    );
    // One undo restores both the level and the column.
    b.editor.history(false).unwrap();
    assert_eq!(line(b.editor.document.project(), follows).1.0[2], 3300.);
}

#[test]
fn bound_columns_follow_a_level_changed_in_its_properties() {
    let (mut b, _) = bench("level-pane");
    let (follows, _) = bound_columns(&mut b);
    let project = b.editor.document.project().clone();
    let upper = level(&project, 3600.);
    let elevations = std::collections::BTreeMap::from([(upper, 3000.)]);
    let mut commands = vec![open_bim_commands::Command::ReplaceLevel {
        level: open_bim_model::Level {
            elevation_mm: 3000.,
            ..project.levels[&upper].clone()
        },
    }];
    commands.extend(open_bim_desktop::tools::column_levels::follow(&project, &elevations).unwrap());
    assert_eq!(commands.len(), 2, "one column follows");
    b.editor.commit("Изменить уровень", &commands).unwrap();
    assert_eq!(line(b.editor.document.project(), follows).1.0[2], 2700.);
    // Lowering it under the column bottom is refused.
    let project = b.editor.document.project().clone();
    let too_low = std::collections::BTreeMap::from([(upper, 200.)]);
    assert!(open_bim_desktop::tools::column_levels::follow(&project, &too_low).is_err());
}

#[test]
fn property_pane_loads_a_file_and_modify_applies_it() {
    let (mut b, dir) = bench("pane");
    let id = create(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)],
    );
    let project = b.editor.document.project().clone();
    let profile = *project.profiles.keys().last().unwrap();
    let values = Values::Beam(MemberValues {
        name: Some("Б-кровля".into()),
        profile: Some(project.profiles[&profile].name.clone()),
        position: Some(PartPosition {
            depth: AtDepth::Behind,
            ..Default::default()
        }),
        ..Default::default()
    });
    files::save(&dir, "кровля", &values).unwrap();
    let selected = BTreeSet::from([id]);
    let c = common(&project, &selected).unwrap();
    let mut pane = PropertyPane::default();
    pane.sync(&c);
    let loaded = files::load(&dir, FileKind::Beam, "кровля").unwrap();
    assert!(pane.apply_file(&project, &loaded).is_empty());
    let changes = pane.changes(&c).unwrap();
    let commands =
        open_bim_desktop::ui::property_pane::modify(&project, &c.parts, &changes).unwrap();
    b.editor.commit("Изменить", &commands).unwrap();
    let after = b.editor.document.project();
    assert_eq!(after.elements[&id].name, "Б-кровля");
    assert_eq!(line(after, id), line(&project, id), "same reference line");
    match &after.elements[&id].shape {
        open_bim_model::ElementShape::Member {
            profile: p,
            position,
            ..
        } => {
            assert_eq!(*p, profile);
            assert_eq!(position.depth, AtDepth::Behind);
        }
        _ => panic!(),
    }
}

#[test]
fn each_tool_keeps_its_own_properties() {
    let (mut b, dir) = bench("per-tool");
    // «standard» is loaded on the first start only.
    files::save(
        &dir,
        files::STANDARD,
        &Values::Beam(MemberValues {
            name: Some("Б-стандарт".into()),
            ..Default::default()
        }),
    )
    .unwrap();
    b.execute(Cmd::ToolBeam);
    assert_eq!(b.editor.draft.name, "Б-стандарт");
    b.editor.draft.name = "Б1".into();
    b.editor.draft.position.depth = AtDepth::Behind;
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolColumn);
    assert_eq!(b.editor.draft.name, "Колонна");
    b.editor.draft.name = "К1".into();
    b.editor.draft.position = PartPosition::default();
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolBeam);
    assert_eq!(b.editor.draft.name, "Б1");
    assert_eq!(b.editor.draft.position.depth, AtDepth::Behind);
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolColumn);
    assert_eq!(b.editor.draft.name, "К1");
    assert_eq!(b.editor.draft.position.depth, AtDepth::Middle);
}

#[test]
fn catalogue_profile_and_steel_are_copied_into_the_project_on_load() {
    let (mut b, dir) = bench("catalogue");
    let project = b.editor.document.project().clone();
    assert!(files::find_profile(&project, "30Б1").is_none());
    let steel = open_bim_model::library::steel(ObjectId(0), "С345")
        .unwrap()
        .name;
    assert!(files::find_material(&project, &steel).is_none());
    let values = Values::Beam(MemberValues {
        profile: Some("30Б1".into()),
        material: Some(steel.clone()),
        ..Default::default()
    });
    files::save(&dir, "Б-30", &values).unwrap();
    b.execute(Cmd::ToolBeam);
    let loaded = files::load(&dir, FileKind::Beam, "Б-30").unwrap();
    files::import_catalogue(&mut b.editor, &loaded).unwrap();
    let project = b.editor.document.project().clone();
    let warnings = files::apply_to_draft(&project, &mut b.editor.draft, &loaded);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(project.profiles[&b.editor.draft.profile].name, "30Б1");
    assert_eq!(project.materials[&b.editor.draft.material].name, steel);
    // Loading again copies nothing more.
    assert!(files::catalogue_commands(&project, &loaded).is_empty());
    // The new beam uses them.
    let id = create(
        &mut b,
        Cmd::ToolBeam,
        &[DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)],
    );
    let after = b.editor.document.project();
    assert_eq!(after.elements[&id].material, b.editor.draft.material);
}
