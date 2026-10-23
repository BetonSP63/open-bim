//! Headless runs of the real application shell (`ui::Workbench`): accessible
//! names (what VoiceOver reads through AccessKit), layout bounds at
//! 1280×800 and 1920×1080, contrast, command registry and job scenarios.
use eframe::egui::{self, accesskit::Role};
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{Mode, Workbench, commands, theme};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn out_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui")
}

fn bench() -> Workbench {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    let mut b = Workbench::new(Editor::new(project).unwrap());
    b.objects = 16;
    b.gpu = "headless".into();
    b
}

fn run(
    ctx: &egui::Context,
    b: &mut Workbench,
    size: [f32; 2],
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size[0], size[1]),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        let _ = b.panels(ui);
        egui::CentralPanel::default().show(ui, |ui| {
            if !b.central(ui) {
                ui.label("3D-вид (GPU)");
            }
        });
    });
    // No renderer here: font atlas uploads are intentionally discarded.
    output.textures_delta.clear();
    output
}

struct Tree {
    nodes: BTreeMap<u64, egui::accesskit::Node>,
}
impl Tree {
    fn name(&self, node: &egui::accesskit::Node) -> String {
        if let Some(l) = node.label() {
            return l.to_string();
        }
        node.labelled_by()
            .iter()
            .filter_map(|id| self.nodes.get(&id.0))
            .filter_map(|n| n.value().or(n.label()).map(str::to_string))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn tree(output: &egui::FullOutput) -> Tree {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled");
    Tree {
        nodes: update
            .nodes
            .iter()
            .map(|(id, n)| (id.0, n.clone()))
            .collect(),
    }
}

const INTERACTIVE: [Role; 8] = [
    Role::Button,
    Role::CheckBox,
    Role::TextInput,
    Role::MultilineTextInput,
    Role::ComboBox,
    Role::Slider,
    Role::RadioButton,
    Role::Link,
];

#[test]
fn every_control_has_an_accessible_name_and_fits_both_window_sizes() {
    let mut summary = serde_json::Map::new();
    for size in [[1280., 800.], [1920., 1080.]] {
        for mode in [Mode::Model, Mode::Drawings, Mode::Reports] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            theme::apply(&ctx);
            let mut b = bench();
            b.mode = mode;
            if let Some(id) = b.editor.document.project().elements.keys().next().copied() {
                b.editor.selected = [id].into();
            }
            if mode == Mode::Drawings {
                b.drawings.create_part(&mut b.editor).unwrap();
            }
            if mode == Mode::Reports {
                b.reports.refresh(&mut b.editor).unwrap();
            }
            let mut output = run(&ctx, &mut b, size, vec![]);
            for _ in 0..3 {
                output = run(&ctx, &mut b, size, vec![]);
            }
            let t = tree(&output);
            let mut unnamed = Vec::new();
            let mut clipped = Vec::new();
            let mut controls = Vec::new();
            for node in t.nodes.values() {
                if !INTERACTIVE.contains(&node.role()) {
                    continue;
                }
                let name = t.name(node);
                if name.trim().is_empty() {
                    unnamed.push(format!("{:?} {:?}", node.role(), node.bounds()));
                }
                if let Some(r) = node.bounds() {
                    // Horizontal clipping: controls must start and end inside the window.
                    if r.x0 < -0.5 || r.x1 > size[0] as f64 + 0.5 {
                        clipped.push(format!("{name} {r:?}"));
                    }
                    controls.push(json!({"role": format!("{:?}", node.role()), "name": name,
                        "bounds": [r.x0, r.y0, r.x1, r.y1]}));
                }
            }
            let key = format!("{:?}-{}x{}", mode, size[0], size[1]);
            assert!(unnamed.is_empty(), "{key}: unnamed controls {unnamed:?}");
            assert!(clipped.is_empty(), "{key}: clipped controls {clipped:?}");
            assert!(controls.len() > 10, "{key}");
            summary.insert(key, json!({"controls": controls.len(), "items": controls}));
        }
    }
    std::fs::write(
        out_dir().join("accessibility-tree.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
}

#[test]
fn text_contrast_is_at_least_wcag_aa() {
    let v = theme::visuals();
    let mut rows = Vec::new();
    for (name, fg, bg) in theme::text_pairs(&v) {
        let c = theme::contrast(fg, bg);
        rows.push(json!({"pair": name, "contrast": (c * 100.).round() / 100.}));
        assert!(c >= 4.5, "{name}: {c:.2}");
    }
    // Focus ring must stand out from the button and panel backgrounds.
    assert!(theme::contrast(theme::FOCUS, v.panel_fill) >= 3.0);
    assert!(v.widgets.active.bg_stroke.width >= 2.0);
    std::fs::write(
        out_dir().join("contrast.json"),
        serde_json::to_string_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
fn commands_are_named_unique_searchable_and_not_stubs() {
    let mut labels = BTreeSet::new();
    let mut shortcuts = BTreeSet::new();
    for spec in commands::ALL {
        assert!(
            !spec.label.is_empty()
                && spec
                    .label
                    .chars()
                    .any(|c| ('а'..='я').contains(&c.to_ascii_lowercase())
                        || ('А'..='Я').contains(&c))
        );
        assert!(labels.insert(spec.label), "duplicate label {}", spec.label);
        if let Some(s) = spec.shortcut {
            assert!(
                shortcuts.insert(format!("{:?}", s)),
                "duplicate shortcut {}",
                spec.label
            );
        }
    }
    let count = commands::ALL
        .iter()
        .filter(|s| s.shortcut.is_some())
        .count();
    assert!(count >= 30, "shortcuts: {count}");
    assert!(commands::search("балка").contains(&commands::Cmd::ToolBeam));
    assert!(commands::search("pdf").contains(&commands::Cmd::ExportPdf));
    assert!(commands::search("ЧЕРТЁЖ ДЕТ").contains(&commands::Cmd::PartDrawing));
    assert!(commands::search("⌘K").contains(&commands::Cmd::NumberAndCheck));
    // Every command has an observable effect (request, state change or message).
    for spec in commands::ALL {
        let mut b = bench();
        if let Some(id) = b.editor.document.project().elements.keys().next().copied() {
            b.editor.selected = [id].into();
        }
        if matches!(
            spec.cmd,
            commands::Cmd::UpdateDrawing | commands::Cmd::ExportPdf | commands::Cmd::ExportDxf
        ) {
            b.drawings.create_part(&mut b.editor).unwrap();
            b.drawings.export_dir = std::env::temp_dir()
                .join("open-bim-ui-cmd")
                .to_string_lossy()
                .into();
        }
        b.reports.directory = std::env::temp_dir()
            .join("open-bim-ui-release")
            .to_string_lossy()
            .into();
        if spec.cmd == commands::Cmd::ModeModel {
            b.mode = Mode::Reports;
        }
        let before = (
            b.mode,
            b.palette.open,
            b.help,
            b.editor.events.len(),
            b.editor.active,
            b.job.is_some(),
            b.tools.focus_property,
        );
        let request = b.execute(spec.cmd);
        let after = (
            b.mode,
            b.palette.open,
            b.help,
            b.editor.events.len(),
            b.editor.active,
            b.job.is_some(),
            b.tools.focus_property,
        );
        let effect = request.is_some()
            || before != after
            || b.tools.message.is_some()
            || b.reports.message.is_some();
        assert!(effect, "{} has no effect", spec.label);
        if b.job.is_some() {
            b.cancel_job();
        }
    }
}

#[test]
fn file_error_and_cancelled_open_leave_the_model_unchanged() {
    let mut b = bench();
    let before = open_bim_commands::project_hash(b.editor.document.project()).unwrap();
    // Missing and corrupt files: error message, model unchanged.
    let dir = std::env::temp_dir().join(format!("open-bim-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let corrupt = dir.join("corrupt.obim");
    std::fs::write(
        &corrupt,
        b"{\"magic\":\"open-bim-project\",\"format_version\":1",
    )
    .unwrap();
    for path in [dir.join("missing.obim"), corrupt] {
        b.start_open(path);
        let started = std::time::Instant::now();
        while b.job.is_some() && started.elapsed().as_secs() < 60 {
            let _ = b.poll_job();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let message = b.tools.message.clone().unwrap_or_default();
        assert!(message.starts_with("Ошибка файла"), "{message}");
        assert!(
            !message.contains("прервана"),
            "real cause is shown: {message}"
        );
        assert_eq!(
            open_bim_commands::project_hash(b.editor.document.project()).unwrap(),
            before
        );
    }
    // Long open (artificial delay) cancelled: UI stays usable, model unchanged.
    b.load_delay_ms = 3000;
    b.start_open(std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/drawings/control.obim"
    )));
    assert!(b.busy());
    assert!(
        b.execute(commands::Cmd::NewProject).is_none(),
        "editing blocked while busy"
    );
    b.execute(commands::Cmd::CancelTool);
    assert!(!b.busy());
    assert_eq!(
        open_bim_commands::project_hash(b.editor.document.project()).unwrap(),
        before
    );
    // Release cancelled at the first checkpoint leaves no output folder.
    let release = dir.join("release");
    b.reports.directory = release.to_string_lossy().into();
    b.start_release();
    b.cancel_job();
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!release.exists() || std::fs::read_dir(&release).unwrap().next().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_caption_glyph_exists_in_the_proportional_fonts() {
    use skrifa::MetadataProvider;
    let fonts = theme::fonts();
    let family = &fonts.families[&egui::FontFamily::Proportional];
    let faces: Vec<skrifa::FontRef> = family
        .iter()
        .filter_map(|name| fonts.font_data.get(name))
        .filter_map(|data| skrifa::FontRef::new(&data.font).ok())
        .collect();
    assert!(faces.len() >= 2);
    let covered = |c: char| c.is_whitespace() || faces.iter().any(|f| f.charmap().map(c).is_some());
    let mut texts: Vec<String> = commands::ALL
        .iter()
        .map(|s| commands::caption(s.cmd))
        .collect();
    texts.push("↑/↓ — выбор, Enter — выполнить, Esc — закрыть".into());
    texts.push("☰ Файл".into());
    for t in texts {
        let missing: String = t.chars().filter(|c| !covered(*c)).collect();
        assert!(missing.is_empty(), "{t}: {missing}");
    }
}
