//! Component editor: a user component made from a catalogue one with plain
//! hints, explained mistakes with one-click fixes, a preview on a sample
//! column and beam, and saving under «Мои компоненты».
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::component_editor::{
    self as ce, Draft, GROUP, UserComponent, View, check, fields, fix_all, get, set,
};
use open_bim_desktop::ui::components::{catalog, search, search_in};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{Element, ElementShape, Frame, MemberKind, ObjectId, Point3, TemplateKind};

fn bench(test: &str) -> (Workbench, std::path::PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("open-bim-components-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut b = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    b.tools.path = dir.join("project.obim").to_string_lossy().into();
    (b, dir)
}

fn member(b: &mut Workbench, kind: MemberKind, start: [f64; 3], end: [f64; 3]) -> ObjectId {
    let project = b.editor.document.project();
    let id = ObjectId(project.next_id);
    let (frame, length) = Frame::along(Point3(start), Point3(end)).unwrap();
    let element = Element {
        id,
        name: "Деталь".into(),
        material: ObjectId(1),
        shape: ElementShape::Member {
            member_kind: kind,
            profile: ObjectId(2),
            length,
            frame,
            position: Default::default(),
        },
        properties: Default::default(),
    };
    b.editor
        .commit("Деталь", &[open_bim_commands::Command::Create { element }])
        .unwrap();
    id
}

#[test]
fn every_catalogue_component_previews_on_its_sample() {
    let project = default_project().unwrap();
    for c in catalog() {
        let t = ce::base_template(&c);
        assert!(check(&t).is_empty(), "{}: {:?}", c.name, check(&t));
        for view in View::ALL {
            let p = ce::preview(&project, &t, view);
            assert_eq!(p.error, None, "{} ({:?})", c.name, view);
            assert!(p.plates > 0 && !p.parts.is_empty(), "{}", c.name);
            assert!(!p.parents.is_empty(), "{}: the joined parts show", c.name);
            let (lo, hi) = p.window;
            for s in &p.parents {
                for q in s {
                    assert!(q[0] >= lo[0] - 1e-6 && q[0] <= hi[0] + 1e-6);
                    assert!(q[1] >= lo[1] - 1e-6 && q[1] <= hi[1] + 1e-6);
                }
            }
        }
    }
}

#[test]
fn every_field_reads_back_what_was_set() {
    for c in catalog() {
        let mut t = ce::base_template(&c);
        for f in fields(&c.kind) {
            assert!(!f.hint.is_empty() && !f.label.is_empty());
            let v = if f.unit == "шт" { 3. } else { 41.5 };
            assert!(set(&mut t, f.key, v), "{} {}", c.name, f.key);
            assert_eq!(get(&t, f.key), Some(v), "{} {}", c.name, f.key);
        }
        assert!(!set(&mut t, "no-such-field", 1.));
    }
    // Welded-only nodes show no bolt fields, the clip angle no weld.
    let keys = |k: &TemplateKind| fields(k).iter().map(|f| f.key).collect::<Vec<_>>();
    assert!(
        !keys(&TemplateKind::Haunch {
            length: 1.,
            depth: 1.
        })
        .contains(&"bolt_diameter")
    );
    assert!(
        !keys(&TemplateKind::ClipAngle {
            leg: 1.,
            height: 1.,
            pitch: 1.
        })
        .contains(&"weld_size")
    );
}

/// Tiny deterministic generator so the check is repeatable.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[test]
fn issues_appear_exactly_when_the_template_is_invalid_and_fixes_repair_it() {
    let mut rng = Lcg(7);
    let specials = [0., -5., f64::NAN, 1e9, 0.5, 13.];
    for round in 0..3000 {
        let c = &catalog()[round % catalog().len()];
        let mut t = ce::base_template(c);
        for f in fields(&c.kind) {
            let r = rng.next();
            let v = if r < 0.15 {
                specials[(rng.next() * specials.len() as f64) as usize % specials.len()]
            } else if f.unit == "шт" {
                (rng.next() * 15.).floor()
            } else {
                (rng.next() * 150.).round()
            };
            set(&mut t, f.key, v);
        }
        if rng.next() < 0.05 {
            t.bolt_grade.clear();
        }
        let issues = check(&t);
        assert_eq!(
            issues.is_empty(),
            t.validate().is_ok(),
            "{}: {t:?} {issues:?}",
            c.name
        );
        for i in &issues {
            assert!(!i.text.is_empty());
        }
        fix_all(&mut t);
        assert!(
            t.validate().is_ok(),
            "{}: fixes leave {:?}",
            c.name,
            check(&t)
        );
    }
}

#[test]
fn a_small_bolt_pitch_is_explained_with_a_fix() {
    let fin = search("шарнир").remove(0);
    let mut t = ce::base_template(&fin);
    set(&mut t, "pitch", 20.);
    let issues = check(&t);
    let pitch = issues
        .iter()
        .find(|i| i.key == "pitch")
        .expect("pitch issue");
    assert!(pitch.text.contains("Ø22"), "{}", pitch.text);
    let fix = pitch.fix.unwrap();
    assert!(fix > 22.);
    set(&mut t, "pitch", fix);
    assert!(check(&t).iter().all(|i| i.key != "pitch"));
    // A bigger bolt grows the edge distance when the hole needs it.
    t.edge_distance = 16.;
    ce::set_bolt(&mut t, 30.);
    assert_eq!((t.bolt_diameter, t.clearance), (30., 3.));
    assert!(t.edge_distance > 16.5);
}

#[test]
fn a_plate_taller_than_the_web_is_caught_by_the_preview() {
    let project = default_project().unwrap();
    let fin = search("шарнир").remove(0);
    let mut t = ce::base_template(&fin);
    set(&mut t, "height", 2000.);
    assert!(check(&t).is_empty(), "the values themselves are fine");
    let p = ce::preview(&project, &t, View::Side);
    let e = p.error.expect("does not fit");
    assert!(e.starts_with("На примере узел не строится"), "{e}");
}

#[test]
fn a_saved_component_is_placed_with_all_its_values() {
    let (mut b, dir) = bench("place");
    b.execute(Cmd::ComponentEditor);
    assert!(b.component_editor.draft.is_some());
    let fin = search("шарнир").remove(0);
    b.new_component(&fin);
    let draft = b.component_editor.draft.as_mut().unwrap();
    assert_eq!(draft.template.name, "Шарнир на пластине (мой)");
    draft.template.name = "Шарнир М16 на 3 болтах".into();
    draft.description = "Для балок I 30".into();
    ce::set_bolt(&mut draft.template, 16.);
    set(&mut draft.template, "rows", 3.);
    // Three bolts do not fit the default plate; the issue says how tall.
    let issue = draft.issues().remove(0);
    assert_eq!((issue.key, issue.fix), ("height", Some(230.)));
    set(&mut draft.template, "height", 230.);
    assert!(draft.issues().is_empty(), "{:?}", draft.issues());
    let placed = b.save_component(true).unwrap();
    assert!(b.component_editor.draft.is_none());
    assert_eq!(placed.group, GROUP);
    assert!(b.components.armed.is_some());

    // The catalogue finds it next to the built-in ones.
    let saved = b.user_components();
    assert_eq!(saved.len(), 1);
    let mine: Vec<_> = saved.iter().map(UserComponent::component).collect();
    let found = search_in(mine, "М16");
    assert_eq!(found[0].name, "Шарнир М16 на 3 болтах");
    assert_eq!(found[0].description, "Для балок I 30");

    let column = member(&mut b, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]);
    let beam = member(
        &mut b,
        MemberKind::Beam,
        [0., 165., 3000.],
        [0., 6000., 3000.],
    );
    assert!(b.pick_object(Some(beam)).is_none(), "waits for the support");
    match b.pick_object(Some(column)) {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("nothing to commit"),
    }
    let project = b.editor.document.project();
    let c = project.connections.values().next().unwrap();
    assert_eq!(c.template.name, "Шарнир М16 на 3 болтах");
    assert_eq!(c.template.bolt_diameter, 16.);
    assert!(matches!(
        c.template.kind,
        TemplateKind::FinPlate { rows: 3, .. }
    ));
    assert_eq!(c.children.len(), 3 + 2 * 3);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rename_copy_and_delete_keep_one_file_per_component() {
    let (mut b, dir) = bench("files");
    let base = search("опорная").remove(0);
    b.new_component(&base);
    b.save_component(false).unwrap();
    assert_eq!(b.user_components()[0].name(), "Опорная плита (мой)");

    // A copy gets a free name.
    let first = b.user_components()[0].component();
    b.new_component(&first);
    assert_eq!(
        b.component_editor.draft.as_ref().unwrap().template.name,
        "Опорная плита (мой) 2"
    );
    b.save_component(false).unwrap();
    assert_eq!(b.user_components().len(), 2);

    // Renaming onto an existing name is refused, a free name moves the file.
    b.edit_component("Опорная плита (мой) 2");
    let draft = b.component_editor.draft.as_mut().unwrap();
    draft.template.name = "Опорная плита (мой)".into();
    let e = b.save_component(false).unwrap_err();
    assert!(e.contains("уже есть"), "{e}");
    b.component_editor.draft.as_mut().unwrap().template.name = "База 400".into();
    b.save_component(false).unwrap();
    let names: Vec<String> = b
        .user_components()
        .iter()
        .map(|u| u.name().into())
        .collect();
    assert_eq!(names, ["База 400", "Опорная плита (мой)"]);

    // A name that cannot be a file name is explained.
    b.edit_component("База 400");
    b.component_editor.draft.as_mut().unwrap().template.name = "a/b".into();
    let issues = b.component_editor.draft.as_ref().unwrap().issues();
    assert_eq!(issues[0].key, "name");
    assert!(b.save_component(false).is_err());
    b.component_editor.draft = None;

    b.delete_component("База 400").unwrap();
    assert_eq!(b.user_components().len(), 1);
    // A broken file is skipped and reported, not fatal.
    std::fs::write(dir.join("components/Плохой.component.json"), "{").unwrap();
    let (ok, errors) = ce::load_all(&dir.join("components"));
    assert_eq!((ok.len(), errors.len()), (1, 1));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn switching_the_base_resets_values_and_keeps_the_name() {
    let fin = search("шарнир").remove(0);
    let mut d = Draft::based_on(&fin, &[]);
    d.template.name = "Мой узел".into();
    let haunch = search("вута").remove(0);
    d.rebase(&haunch);
    assert_eq!(d.template.name, "Мой узел");
    assert!(matches!(d.template.kind, TemplateKind::Haunch { .. }));
    set(&mut d.template, "length", 0.);
    assert!(!d.issues().is_empty());
    d.reset();
    assert!(d.issues().is_empty());
}

#[test]
fn clipping_keeps_the_part_inside_the_box() {
    let window = ([0., 0.], [10., 10.]);
    assert_eq!(
        ce::clip([[-5., 5.], [15., 5.]], window),
        Some([[0., 5.], [10., 5.]])
    );
    assert_eq!(ce::clip([[-5., -5.], [-1., 20.]], window), None);
    assert_eq!(
        ce::clip([[2., 2.], [3., 3.]], window),
        Some([[2., 2.], [3., 3.]])
    );
    assert_eq!(ce::mm(22.), "22");
    assert_eq!(ce::mm(12.5), "12,5");
}

#[test]
fn the_editor_window_draws_with_named_controls_and_a_preview() {
    use eframe::egui;
    let (mut b, dir) = bench("window");
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let fin = search("шарнир").remove(0);
    b.new_component(&fin);
    let frame = |b: &mut Workbench| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let _ = b.panels(ui);
        });
        output.textures_delta.clear();
        output
    };
    let mut output = frame(&mut b);
    // The preview is built in the background; wait for it (at most 60 s).
    let start = std::time::Instant::now();
    loop {
        let names: Vec<String> = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|u| u.nodes.iter())
            .filter_map(|(_, n)| n.label().or(n.value()).map(str::to_string))
            .collect();
        let has = |s: &str| names.iter().any(|n| n.contains(s));
        assert!(has("На основе какого узла"), "{names:?}");
        assert!(has("Шаг болтов") && has("М20") && has("Сохранить"));
        if has("Деталей узла") {
            break;
        }
        assert!(start.elapsed().as_secs() < 60, "preview never arrived");
        std::thread::sleep(std::time::Duration::from_millis(50));
        output = frame(&mut b);
    }
    // The catalogue window with a saved component draws its buttons.
    b.component_editor.draft.as_mut().unwrap().template.name = "Мой шарнир".into();
    b.save_component(false).unwrap();
    b.execute(Cmd::ComponentCatalog);
    let output = frame(&mut b);
    let text: Vec<String> = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|u| u.nodes.iter())
        .filter_map(|(_, n)| n.label().or(n.value()).map(str::to_string))
        .collect();
    for s in [
        "Мой шарнир",
        "Изменить",
        "Копия",
        "Удалить",
        "Создать свой компонент",
    ] {
        assert!(text.iter().any(|t| t.contains(s)), "{s}: {text:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn catalogue_values_meet_gost_and_sp_16() {
    for c in catalog() {
        let t = ce::base_template(&c);
        assert_eq!(t.bolt_standard, "ГОСТ 7798-70");
        assert_eq!(ce::advice(&t), vec![], "{}", c.name);
    }
}

#[test]
fn gost_bolts_and_sp_16_spacing_are_advised_with_fixes() {
    let fin = search("шарнир").remove(0);
    let mut t = ce::base_template(&fin);
    // High-strength bolts: class 10.9 only, hole +3.
    ce::set_standard(&mut t, ce::bolt_standard("ГОСТ Р 52644-2006").unwrap());
    assert_eq!((t.bolt_grade.as_str(), t.clearance), ("10.9", 3.));
    t.bolt_grade = "8.8".into();
    let a = ce::advice(&t);
    assert!(
        a.iter()
            .any(|a| a.key == "bolt_grade" && a.text.contains("только 10.9")),
        "{a:?}"
    );
    // Edge 35 mm with a Ø23 hole: enough across the force only.
    t.bolt_grade = "10.9".into();
    t.edge_distance = 35.;
    let edge = ce::advice(&t)
        .into_iter()
        .find(|a| a.key == "edge_distance")
        .unwrap();
    assert!(edge.text.contains("2d = 46"), "{}", edge.text);
    assert_eq!(edge.fix, Some(50.));
    // Bolts closer than 2,5d.
    set(&mut t, "pitch", 40.);
    assert!(ce::advice(&t).iter().any(|a| a.key == "pitch"));
    // A thin weld on a 20 mm plate.
    t.thickness = 20.;
    t.weld_size = 5.;
    assert!(
        ce::advice(&t)
            .iter()
            .any(|a| a.key == "weld_size" && a.fix == Some(7.))
    );
    // A foreign standard is named.
    t.bolt_standard = "DIN 931".into();
    assert!(ce::advice(&t).iter().any(|a| a.key == "bolt_standard"));
    ce::bring_to_norms(&mut t);
    assert!(check(&t).is_empty());
    assert_eq!(ce::advice(&t), vec![]);
    assert_eq!(t.bolt_standard, "ГОСТ 7798-70");
}

#[test]
fn bringing_to_norms_keeps_every_template_valid() {
    let mut rng = Lcg(11);
    for round in 0..600 {
        let c = &catalog()[round % catalog().len()];
        let mut t = ce::base_template(c);
        for f in fields(&c.kind) {
            let v = if f.unit == "шт" {
                (rng.next() * 6.).floor() + 1.
            } else {
                (rng.next() * 120.).round() + 1.
            };
            set(&mut t, f.key, v);
        }
        ce::bring_to_norms(&mut t);
        assert!(check(&t).is_empty(), "{}: {:?}", c.name, check(&t));
        assert_eq!(ce::advice(&t), vec![], "{}: {t:?}", c.name);
    }
}
