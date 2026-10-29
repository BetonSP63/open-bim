//! «Семейства» and «Редактор семейств» driven by pointer events: place a
//! ladder by two points, change its instance values, edit the family and
//! store it back in the project.
use eframe::egui::{self, accesskit::Role};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_renderer::DVec3;

fn frame(ctx: &egui::Context, b: &mut Workbench, events: Vec<egui::Event>) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600., 1000.),
        )),
        events,
        ..Default::default()
    };
    let mut requests = Vec::new();
    let mut output = ctx.run_ui(input, |ui| {
        requests = b.panels(ui);
        egui::CentralPanel::default().show(ui, |ui| {
            b.central(ui);
        });
    });
    for r in requests {
        if let Request::Action(Action::Commit(label, commands)) = r {
            b.editor.commit(&label, &commands).unwrap();
        }
    }
    output.textures_delta.clear();
    output
}

fn button(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update.nodes.iter().find_map(|(_, n)| {
        let r = n.bounds()?;
        (n.role() == Role::Button && n.label() == Some(label))
            .then(|| egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32))
    })
}

fn press(ctx: &egui::Context, b: &mut Workbench, label: &str) {
    let out = frame(ctx, b, vec![]);
    let at = button(&out, label).unwrap_or_else(|| panic!("no button «{label}»"));
    let ev = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        ctx,
        b,
        vec![egui::Event::PointerMoved(at), ev(true), ev(false)],
    );
    frame(ctx, b, vec![]);
}

#[test]
fn place_change_and_edit_a_family() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut b = Workbench::new(Editor::new(open_bim_model::Project::default()).unwrap());
    b.gpu = "headless".into();
    b.execute(Cmd::Families);
    frame(&ctx, &mut b, vec![]);
    let ladder = b
        .families
        .current
        .take()
        .map(|_| {
            open_bim_model::sample_families()
                .into_iter()
                .find(|f| f.name == "Стремянка")
                .unwrap()
        })
        .unwrap();
    b.families.current = Some((ladder, "Ш600".into()));
    press(&ctx, &mut b, "Разместить по двум точкам");
    assert!(b.pick(DVec3::new(0., 0., 0.)).is_none());
    match b.pick(DVec3::new(0., 0., 3000.)) {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap()
        }
        _ => panic!("no placement: {:?}", b.tools.message),
    };
    let p = b.editor.document.project();
    let instance = p.family_instances.values().next().unwrap().clone();
    assert_eq!(instance.children.len(), 2 + 10 + 1);
    // Select a rung: the window shows the object and «Применить» rebuilds it.
    b.editor.selected = [instance.children["body-1-0"]].into();
    frame(&ctx, &mut b, vec![]);
    press(&ctx, &mut b, "Применить");
    // Edit the family from the catalogue and store it in the project.
    press(&ctx, &mut b, "Редактировать");
    assert!(b.family_editor.open);
    b.family_editor.family.parameters[3].value = "800".into();
    press(&ctx, &mut b, "Проверить");
    press(&ctx, &mut b, "Сохранить в проект");
    let p = b.editor.document.project();
    assert_eq!(p.families["Стремянка"].parameters[3].value, "800");
    assert_eq!(p.family_instances.len(), 1);
    // A new family opens a blank one in the editor.
    press(&ctx, &mut b, "Новое семейство");
    assert!(b.family_editor.family.name.starts_with("Семейство"));
}
