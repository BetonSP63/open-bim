//! Chrome driven headlessly through real pointer and keyboard
//! events: every ribbon button runs a registry command, unavailable concrete
//! and rebar tools explain themselves, quick launch runs commands and the
//! light theme keeps WCAG contrast.
use eframe::egui::{self, accesskit::Role};
use open_bim_desktop::interaction::selection::Click;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::chrome::{self, RibbonTab};
use open_bim_desktop::ui::{Request, Workbench, theme};
use serde_json::json;

fn bench() -> Workbench {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    let mut b = Workbench::new(Editor::new(project).unwrap());
    b.objects = 16;
    b.gpu = "headless".into();
    if let Some(id) = b.editor.document.project().elements.keys().next().copied() {
        b.editor.selected = [id].into();
    }
    b.reports.directory = std::env::temp_dir()
        .join("open-bim-chrome-release")
        .to_string_lossy()
        .into();
    b.drawings.export_dir = std::env::temp_dir()
        .join("open-bim-chrome-export")
        .to_string_lossy()
        .into();
    b
}

fn frame(
    ctx: &egui::Context,
    b: &mut Workbench,
    size: [f32; 2],
    events: Vec<egui::Event>,
) -> (egui::FullOutput, Vec<Request>) {
    // Each context gets its own clock so hover delays behave like a fresh window.
    let started = ctx.data_mut(|d| {
        *d.get_temp_mut_or_insert_with(egui::Id::new("test clock"), std::time::Instant::now)
    });
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size[0], size[1]),
        )),
        events,
        time: Some(started.elapsed().as_secs_f64()),
        ..Default::default()
    };
    let mut requests = Vec::new();
    let mut output = ctx.run_ui(input, |ui| {
        requests = b.panels(ui);
        egui::CentralPanel::default().show(ui, |ui| {
            if !b.central(ui) {
                ui.label("3D-вид (GPU)");
            }
        });
    });
    output.textures_delta.clear();
    (output, requests)
}

/// (role, name, enabled, center) for every node.
fn nodes(output: &egui::FullOutput) -> Vec<(Role, String, bool, egui::Pos2)> {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .filter_map(|(_, n)| {
            let r = n.bounds()?;
            let name = n.label().or(n.value()).unwrap_or("").to_string();
            let center = egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32);
            Some((n.role(), name, !n.is_disabled(), center))
        })
        .collect()
}

fn click(at: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    vec![egui::Event::PointerMoved(at), button(true), button(false)]
}

fn state(b: &Workbench) -> String {
    format!(
        "{:?} {} {} {} {} {} {:?} {:?} {:?} {} {} {:?} {} {} {} {}",
        b.mode,
        b.palette.open,
        b.help,
        b.editor.events.len(),
        b.editor.active,
        b.job.is_some(),
        b.side_panel,
        b.ribbon_tab,
        b.engine.tool,
        b.light_theme,
        b.workplane_picks.is_some(),
        b.tools.message,
        b.reports.message.is_some(),
        b.switches.ortho,
        b.section_enabled,
        b.hidden.len(),
    )
}

#[test]
fn every_ribbon_button_runs_its_registry_command_by_mouse() {
    let mut rows = Vec::new();
    for tab in RibbonTab::ALL {
        for (group, items) in chrome::groups(tab) {
            for it in items {
                let ctx = egui::Context::default();
                ctx.enable_accesskit();
                theme::apply(&ctx);
                let mut b = bench();
                if matches!(
                    it.cmd,
                    Some(open_bim_desktop::ui::commands::Cmd::UpdateDrawing)
                        | Some(open_bim_desktop::ui::commands::Cmd::ExportPdf)
                        | Some(open_bim_desktop::ui::commands::Cmd::ExportDxf)
                ) {
                    b.drawings.create_part(&mut b.editor).unwrap();
                }
                b.ribbon_tab = tab;
                frame(&ctx, &mut b, [1280., 800.], vec![]);
                let out = frame(&ctx, &mut b, [1280., 800.], vec![]).0;
                // Ribbon buttons live in the top band; the side strip reuses some names.
                let found = nodes(&out)
                    .into_iter()
                    .find(|(role, name, _, c)| {
                        *role == Role::Button && name == it.label && (60. ..120.).contains(&c.y)
                    })
                    .unwrap_or_else(|| panic!("{tab:?}/{group}: no button {}", it.label));
                let before = state(&b);
                let (_, requests) = frame(&ctx, &mut b, [1280., 800.], click(found.3));
                let (_, later) = frame(&ctx, &mut b, [1280., 800.], vec![]);
                let effect = !requests.is_empty() || !later.is_empty() || state(&b) != before;
                match it.cmd {
                    Some(cmd) => {
                        assert!(found.2, "{}: disabled", it.label);
                        assert!(
                            effect,
                            "{tab:?}/{}: click had no effect at {:?}: {before}",
                            it.label, found.3
                        );
                        rows.push(
                            json!({"tab": tab.title(), "group": group, "button": it.label,
                            "command": open_bim_desktop::ui::commands::spec(cmd).label}),
                        );
                    }
                    None => {
                        assert!(!found.2, "{}: future tool must be disabled", it.label);
                        assert!(!effect, "{}: future tool must do nothing", it.label);
                    }
                }
                if b.job.is_some() {
                    b.cancel_job();
                }
            }
        }
    }
    assert!(rows.len() >= 45, "ribbon buttons: {}", rows.len());
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui/ribbon-chrome");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("ribbon-commands.json"),
        serde_json::to_string_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
fn unavailable_concrete_and_rebar_explain_on_hover() {
    for tab in [RibbonTab::Concrete, RibbonTab::Rebar] {
        assert!(!tab.available());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::apply(&ctx);
        let mut b = bench();
        frame(&ctx, &mut b, [1280., 800.], vec![]);
        let out2 = frame(&ctx, &mut b, [1280., 800.], vec![]).0;
        let tab_node = nodes(&out2)
            .into_iter()
            .find(|(_, name, _, _)| name == tab.title())
            .expect("tab");
        // Clicking an unavailable tab still shows its (disabled) tools.
        frame(&ctx, &mut b, [1280., 800.], click(tab_node.3));
        assert_eq!(b.ribbon_tab, tab);
        let out = frame(&ctx, &mut b, [1280., 800.], vec![]).0;
        let tool = nodes(&out)
            .into_iter()
            .rfind(|(role, _, enabled, c)| {
                *role == Role::Button && !enabled && (60. ..120.).contains(&c.y)
            })
            .expect("disabled tool");
        let mut seen = false;
        // egui hides tooltips when a click happened less than 0.1 s before the
        // pointer moved; a user pauses after picking the tab.
        std::thread::sleep(std::time::Duration::from_millis(200));
        frame(
            &ctx,
            &mut b,
            [1280., 800.],
            vec![egui::Event::PointerMoved(tool.3)],
        );
        for _ in 0..60 {
            let out = frame(&ctx, &mut b, [1280., 800.], vec![]).0;
            if nodes(&out)
                .iter()
                .any(|(_, n, _, _)| n == chrome::UNAVAILABLE)
            {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(seen, "{tab:?}: tooltip «{}» not shown", chrome::UNAVAILABLE);
    }
}

#[test]
fn quick_launch_runs_the_first_match_on_enter() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut b = bench();
    frame(&ctx, &mut b, [1280., 800.], vec![]);
    let out = frame(&ctx, &mut b, [1280., 800.], vec![]).0;
    let field = nodes(&out)
        .into_iter()
        .find(|(role, name, _, _)| *role == Role::TextInput && name.contains("Быстрый запуск"))
        .expect("quick launch field");
    frame(&ctx, &mut b, [1280., 800.], click(field.3));
    frame(
        &ctx,
        &mut b,
        [1280., 800.],
        vec![egui::Event::Text("балка".into())],
    );
    assert_eq!(b.quick_launch, "балка");
    let out = frame(&ctx, &mut b, [1280., 800.], vec![]).0;
    assert!(
        nodes(&out).iter().any(|(_, n, _, c)| {
            *n == open_bim_desktop::ui::commands::caption(
                open_bim_desktop::ui::commands::Cmd::ToolBeam,
            ) && c.x > field.3.x - 120.
                && c.y > field.3.y
        }),
        "results list is shown under the field: {:?}",
        nodes(&out)
            .iter()
            .filter(|n| n.3.x > 900.)
            .collect::<Vec<_>>()
    );
    let enter = |pressed| egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(&ctx, &mut b, [1280., 800.], vec![enter(true), enter(false)]);
    assert_eq!(b.engine.tool, Some(open_bim_desktop::tools::ToolKind::Beam));
    assert!(b.quick_launch.is_empty());
    assert!(b.prompt().contains("начальную точку"), "{}", b.prompt());
}

#[test]
fn light_theme_contrast_and_focus_ring() {
    let v = theme::light_visuals();
    let mut rows = Vec::new();
    for (name, fg, bg) in theme::text_pairs(&v) {
        let c = theme::contrast(fg, bg);
        rows.push(json!({"pair": name, "contrast": (c * 100.).round() / 100.}));
        assert!(c >= 4.5, "{name}: {c:.2}");
    }
    assert!(theme::contrast(theme::LIGHT_FOCUS, v.panel_fill) >= 3.0);
    assert!(theme::contrast(theme::LIGHT_FOCUS, theme::RIBBON_BG) >= 3.0);
    assert!(v.widgets.active.bg_stroke.width >= 2.0);
    assert!(!v.dark_mode);
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui/ribbon-chrome");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("contrast-light.json"),
        serde_json::to_string_pretty(&rows).unwrap(),
    )
    .unwrap();
    // The theme switch is a registry command and flips the visuals.
    let ctx = egui::Context::default();
    let mut b = bench();
    frame(&ctx, &mut b, [1280., 800.], vec![]);
    assert!(!ctx.global_style().visuals.dark_mode);
    b.execute(open_bim_desktop::ui::commands::Cmd::ToggleTheme);
    frame(&ctx, &mut b, [1280., 800.], vec![]);
    assert!(ctx.global_style().visuals.dark_mode);
}

#[test]
fn selection_switches_pick_parts_assemblies_or_nothing() {
    let mut b = bench();
    b.editor.selected.clear();
    b.execute(open_bim_desktop::ui::commands::Cmd::AutoAssemblies);
    let project = b.editor.document.project();
    let assembly = project
        .assemblies
        .values()
        .find(|a| a.members.len() > 1)
        .expect("welded assembly")
        .clone();
    let part = *assembly.members.iter().next().unwrap();
    b.editor.selected.clear();
    b.select_picked(Some(part), Click::Replace);
    assert_eq!(b.editor.selected, [part].into());
    b.select.assemblies = true;
    b.select_picked(Some(part), Click::Replace);
    assert_eq!(b.editor.selected, assembly.members);
    b.select.parts = false;
    b.select.assemblies = false;
    b.editor.selected.clear();
    b.select_picked(Some(part), Click::Replace);
    assert!(b.editor.selected.is_empty());
}

#[test]
fn model_search_selects_matching_parts() {
    let mut b = bench();
    b.editor.selected.clear();
    let first = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap();
    let (id, name) = (first.id, first.name.clone());
    b.search = name.to_uppercase();
    assert!(b.search_model() >= 1);
    assert!(
        b.editor.selected.contains(&id),
        "case-insensitive name match"
    );
    b.search = format!("#{}", id.0);
    b.search_model();
    assert!(b.editor.selected.contains(&id), "ID match");
    b.search = "нет такого объекта".into();
    assert_eq!(b.search_model(), 0);
    assert!(b.editor.selected.is_empty());
}
