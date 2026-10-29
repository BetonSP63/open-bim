//! «Выбор профиля»: the profile catalogue in its own window. The tree finds
//! sections by search, the right side shows dimensions, «Применить» keeps the
//! window open, «ОК» takes the profile and closes it.
use eframe::egui::{self, accesskit::Role};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::ui::catalog_picker::Pick;
use open_bim_desktop::ui::profile_dialog::{
    Info, Outcome, ProfileDialog, Selected, profile_field, properties,
};
use open_bim_model::{Project, library};

fn frame(
    ctx: &egui::Context,
    dialog: &mut ProfileDialog,
    project: &Project,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, Option<Pick<open_bim_model::Profile>>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280., 800.),
        )),
        // A second apart, so clicks never merge into double clicks.
        time: Some(ctx.input(|i| i.time) + 1.),
        events,
        ..Default::default()
    };
    let mut picked = None;
    let mut output = ctx.run_ui(input, |ui| {
        picked = dialog.show(ui.ctx(), "test", project);
    });
    output.textures_delta.clear();
    (output, picked)
}

/// Centre of the first accessible node with this role and label.
fn centre(output: &egui::FullOutput, role: Role, label: &str) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, n)| {
        (n.role() == role && n.label() == Some(label)).then(|| {
            let r = n.bounds().unwrap();
            egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32)
        })
    })
}

fn labels(output: &egui::FullOutput) -> Vec<String> {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().or(n.value()).map(str::to_string))
        .collect()
}

fn click(at: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(at),
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
    ]
}

#[test]
fn opens_on_the_current_profile_and_picks_a_catalogue_section() {
    let project = default_project().unwrap();
    let current = *project.profiles.keys().next().unwrap();
    let mut d = ProfileDialog::default();
    d.open_on(&project, Some(current));
    assert!(d.open);
    assert_eq!(d.selected, Some(Selected::Project(current)));
    assert_eq!(d.pick(&project), Some(Pick::Project(current)));

    // «Применить» returns the section and keeps the window open.
    d.selected = Some(Selected::Catalog("20Б1".into()));
    let Some(Pick::New(p)) = d.finish(&project, Outcome::Apply) else {
        panic!("20Б1 is not in the project yet");
    };
    assert_eq!(p.name, "20Б1");
    assert_eq!(p.id.0, project.next_id);
    assert!(d.open);
    // «Отмена» closes without a profile.
    assert_eq!(d.finish(&project, Outcome::Cancel), None);
    assert!(!d.open);
    // An unknown name gives nothing to take.
    d.selected = Some(Selected::Catalog("нет такого".into()));
    assert_eq!(d.pick(&project), None);
}

#[test]
fn properties_show_dimensions_area_and_mass_with_decimal_comma() {
    let e = library::section("20Б1").unwrap();
    let rows = properties(&Info {
        name: e.name.clone(),
        shape: e.shape.clone(),
        standard: Some(e.standard.into()),
        area_mm2: Some(e.area_mm2),
        radius: Some(e.radius),
    });
    let get = |k: &str| rows.iter().find(|(l, _)| *l == k).unwrap().1.clone();
    assert_eq!(get("h, мм"), "200");
    assert_eq!(get("b, мм"), "100");
    assert_eq!(get("s, мм"), "5,6");
    assert_eq!(get("t, мм"), "8,5");
    assert_eq!(get("A, см²"), "28,49");
    assert_eq!(get("Масса, кг/м"), "22,36");
}

#[test]
fn search_then_click_and_ok_returns_the_section_and_closes_the_window() {
    let project = default_project().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut d = ProfileDialog::default();
    d.open_on(&project, None);
    d.query = "40Ш1".into();
    let mut out = frame(&ctx, &mut d, &project, vec![]).0;
    for _ in 0..2 {
        out = frame(&ctx, &mut d, &project, vec![]).0;
    }
    let names = labels(&out);
    for name in ["ОК", "Применить", "Отмена", "Поиск"] {
        assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
    }
    // The window keeps its size from frame to frame (an auto-sized window
    // must not grow with its own separators).
    let again = frame(&ctx, &mut d, &project, vec![]).0;
    assert_eq!(
        centre(&again, Role::Button, "ОК"),
        centre(&out, Role::Button, "ОК")
    );
    // ОК is disabled until a row is chosen.
    let ok = centre(&out, Role::Button, "ОК").unwrap();
    let (_, picked) = frame(&ctx, &mut d, &project, click(ok));
    assert_eq!(picked, None);
    assert!(d.open);

    let row = centre(&out, Role::Button, "40Ш1")
        .or_else(|| centre(&out, Role::CheckBox, "40Ш1"))
        .or_else(|| {
            // Selectable labels are toggle buttons in AccessKit.
            let update = out.platform_output.accesskit_update.as_ref()?;
            update.nodes.iter().find_map(|(_, n)| {
                (n.label() == Some("40Ш1")).then(|| {
                    let r = n.bounds().unwrap();
                    egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32)
                })
            })
        })
        .expect("40Ш1 row in the tree");
    frame(&ctx, &mut d, &project, click(row));
    assert_eq!(d.selected, Some(Selected::Catalog("40Ш1".into())));
    frame(&ctx, &mut d, &project, vec![]);
    let out = frame(&ctx, &mut d, &project, vec![]).0;
    // The right side shows the section's standard and height.
    let names = labels(&out);
    assert!(names.iter().any(|n| n == "ГОСТ 26020-83"), "{names:?}");
    let ok = centre(&out, Role::Button, "ОК").unwrap();
    let (_, picked) = frame(&ctx, &mut d, &project, click(ok));
    let Some(Pick::New(p)) = picked else {
        panic!("no profile from ОК: {d:?}")
    };
    assert_eq!(p.name, "40Ш1");
    assert!(!d.open);
}

#[test]
fn the_field_is_a_named_button_that_opens_the_window() {
    let project = default_project().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let run = |events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                profile_field(ui, "field", &project, None, "30Б1", "Профиль");
            });
        });
        out.textures_delta.clear();
        out
    };
    let out = run(vec![]);
    let at = centre(&out, Role::Button, "Профиль").expect("named field");
    run(click(at));
    let out = run(vec![]);
    assert!(
        labels(&out).iter().any(|n| n == "Выбор профиля"),
        "window title"
    );
}
