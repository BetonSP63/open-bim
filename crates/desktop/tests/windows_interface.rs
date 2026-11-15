#![cfg(target_os = "windows")]
//! Exercise actual Control events, which expose Command/Control collisions
//! that a registry comparison of the modifier structs cannot detect.
mod headless;
use eframe::egui::{self, Key, Modifiers};
use headless::HeadlessFrame;
use open_bim_desktop::{
    interaction::tools::default_project,
    tools::Editor,
    ui::{
        Workbench,
        commands::{self, Cmd},
        platform,
    },
};

fn control(shift: bool) -> Modifiers {
    Modifiers {
        ctrl: true,
        command: true,
        shift,
        ..Default::default()
    }
}

fn key(key: Key, modifiers: Modifiers) -> Vec<egui::Event> {
    vec![
        egui::Event::ModifiersChanged(modifiers),
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        },
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers,
        },
    ]
}

#[test]
fn physical_windows_shortcuts_run_one_intended_command() {
    for (key_code, shift, expected) in [
        (Key::F, false, Cmd::FocusSearch),
        (Key::F, true, Cmd::ComponentCatalog),
        (Key::I, false, Cmd::ImportIfc),
        (Key::I, true, Cmd::PanelViews),
        (Key::P, false, Cmd::Palette),
        (Key::Num3, true, Cmd::PlaneOr3d),
        (Key::P, true, Cmd::ExportPdf),
        (Key::K, false, Cmd::NumberAndCheck),
        (Key::K, true, Cmd::ToggleContextual),
        (Key::L, false, Cmd::EditProperty),
        (Key::L, true, Cmd::DocumentManager),
        (Key::C, false, Cmd::CopyToClipboard),
        (Key::D, true, Cmd::CopyByPicks),
        (Key::V, false, Cmd::Paste),
        (Key::S, false, Cmd::SaveProject),
        (Key::S, true, Cmd::SaveProjectAs),
        (Key::Z, false, Cmd::Undo),
        (Key::Y, false, Cmd::Redo),
        (Key::Z, true, Cmd::Redo),
    ] {
        let ctx = egui::Context::default();
        let mut commands = vec![];
        let _ = ctx.run_headless(
            egui::RawInput {
                events: key(key_code, control(shift)),
                ..Default::default()
            },
            |ui| {
                commands = commands::pressed(ui.ctx());
            },
        );
        assert_eq!(commands, vec![expected], "Ctrl+{shift:?}+{key_code:?}");
    }
    assert_eq!(commands::shortcut_text(Cmd::Redo), "Ctrl+Y");
    assert_eq!(commands::shortcut_text(Cmd::SaveProjectAs), "Ctrl+Shift+S");
    for command in commands::ALL {
        assert!(!commands::shortcut_text(command.cmd).contains(['⌘', '⇧', '⌥', '↩']));
    }
}

#[test]
fn text_field_keeps_select_all_clipboard_and_undo_keys() {
    let ctx = egui::Context::default();
    let mut text = "Текст свойства".to_owned();
    let id = egui::Id::new("Windows property text");
    let _ = ctx.run_headless(Default::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).id(id))
            .request_focus();
    });
    for (key_code, expected) in [
        (Key::A, Cmd::SelectAll),
        (Key::C, Cmd::CopyToClipboard),
        (Key::V, Cmd::Paste),
        (Key::Z, Cmd::Undo),
        (Key::Y, Cmd::Redo),
    ] {
        let _ = ctx.run_headless(
            egui::RawInput {
                events: key(key_code, control(false)),
                ..Default::default()
            },
            |ui| {
                assert!(ui.ctx().egui_wants_keyboard_input());
                assert!(
                    !commands::pressed(ui.ctx()).contains(&expected),
                    "text edit lost {key_code:?}"
                );
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
            },
        );
    }
}

#[test]
fn native_clipboard_events_reach_model_commands_but_other_text_does_not() {
    let ctx = egui::Context::default();
    for (event, expected) in [
        (egui::Event::Copy, vec![Cmd::CopyToClipboard]),
        (
            egui::Event::Paste(commands::MODEL_CLIPBOARD_MARKER.into()),
            vec![Cmd::Paste],
        ),
        (
            egui::Event::Paste("Текст из другого приложения".into()),
            vec![],
        ),
    ] {
        let _ = ctx.run_headless(
            egui::RawInput {
                events: vec![event],
                ..Default::default()
            },
            |ui| {
                assert_eq!(commands::pressed(ui.ctx()), expected);
            },
        );
    }
}

#[test]
fn save_as_preserves_original_and_retains_destination_after_failure() {
    let dir = std::env::temp_dir().join(format!(
        "open-bim-windows-save-as-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let original = dir.join("Исходный проект.obim");
    let mut bench = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
    bench.set_project_path(original.clone());
    bench.save_project().unwrap();
    let original_bytes = std::fs::read(&original).unwrap();
    bench
        .editor
        .commit(
            "Изменить имя",
            &[open_bim_commands::Command::RenameProject {
                name: "Сохранённая копия".into(),
            }],
        )
        .unwrap();
    let copy = dir.join("Копия проекта.OBIM");
    bench.save_project_as(copy.clone()).unwrap();
    assert_eq!(bench.project_path(), copy);
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
    assert_eq!(
        open_bim_persistence::load(&copy).unwrap().name,
        "Сохранённая копия"
    );
    assert!(!bench.modified());
    assert!(
        bench
            .save_project_as(original.join("невозможный.obim"))
            .is_err()
    );
    assert_eq!(bench.project_path(), copy);
    assert_eq!(bench.tools.path, copy.to_string_lossy());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn document_caption_is_short_and_preserves_cyrillic() {
    assert_eq!(
        platform::document_title(r"C:\Длинная папка\Проект цеха.obim", true),
        "* Проект цеха.obim — Open BIM"
    );
    assert_eq!(
        platform::document_title(r"C:\Длинная папка\Проект цеха.obim", false),
        "Проект цеха.obim — Open BIM"
    );
    assert_eq!(
        platform::obim_path(std::path::Path::new("Новая модель")),
        std::path::Path::new("Новая модель.obim")
    );
}

#[test]
fn backspace_only_retracts_picks_and_does_not_delete_windows_selection() {
    let mut bench =
        Workbench::new(Editor::new(open_bim_model::example_steel_frame().unwrap()).unwrap());
    let id = *bench
        .editor
        .document
        .project()
        .elements
        .keys()
        .next()
        .unwrap();
    bench.editor.selected.insert(id);
    assert!(bench.execute(Cmd::UndoPick).is_none());
    assert!(bench.editor.document.project().elements.contains_key(&id));
    assert!(bench.execute(Cmd::DeleteSelected).is_some() || bench.delete_confirm.is_some());
}

#[test]
fn scaled_windows_layout_keeps_space_for_the_model() {
    for size in [[1280., 800.], [1024., 640.], [853., 533.], [800., 480.]] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        open_bim_desktop::ui::theme::apply(&ctx);
        ctx.set_fonts(open_bim_desktop::ui::theme::fonts_with_system());
        let mut bench = Workbench::new(Editor::new(default_project().unwrap()).unwrap());
        let mut viewport = egui::Rect::NOTHING;
        for _ in 0..20 {
            let output = ctx.run_headless(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0], size[1]),
                    )),
                    ..Default::default()
                },
                |ui| {
                    bench.panels(ui);
                    egui::CentralPanel::default().show(ui, |ui| {
                        viewport = ui.max_rect();
                    });
                },
            );
            if let Some(update) = &output.platform_output.accesskit_update {
                for (_, node) in &update.nodes {
                    if node.role() == egui::accesskit::Role::TextInput
                        && let Some(bounds) = node.bounds()
                        && bounds.y0 >= viewport.min.y as f64
                        && bounds.y1 <= viewport.max.y as f64
                    {
                        assert!(
                            bounds.x0 >= viewport.max.x as f64 && bounds.x1 <= size[0] as f64,
                            "property field was cropped at {size:?}: {bounds:?}, viewport={viewport:?}"
                        );
                    }
                }
            }
        }
        assert!(
            viewport.min.y < 190.,
            "ribbon grew vertically at {size:?}: {viewport:?}"
        );
        assert!(
            viewport.height() > 150. && viewport.width() > 300.,
            "model squeezed out at {size:?}: {viewport:?}"
        );
    }
}
