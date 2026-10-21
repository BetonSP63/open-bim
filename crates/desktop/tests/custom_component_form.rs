//! The custom component form: values on the preview sample, a headless run
//! of the form, and a saved user component keeping its formulas.
use eframe::egui;
use open_bim_desktop::ui::component_editor::{UserComponent, base_template};
use open_bim_desktop::ui::components::catalog;
use open_bim_desktop::ui::custom_form::{definition_form, sample_values};
use open_bim_model::TemplateKind;

fn custom() -> open_bim_model::ConnectionTemplate {
    let c = catalog()
        .into_iter()
        .find(|c| matches!(c.kind, TemplateKind::Custom { .. }))
        .expect("the catalogue has a custom component");
    base_template(&c)
}

#[test]
fn sample_values_follow_the_formulas_and_report_errors() {
    let mut t = custom();
    let (values, each) = sample_values(&t, definition_of(&t));
    // e + d = 45 + 20 on the I 300×150 sample.
    assert_eq!(values["свес"], 65.);
    assert_eq!(values["ширина"], 280.);
    assert!(each.iter().all(Result::is_ok));
    if let TemplateKind::Custom { definition } = &mut t.kind {
        definition.variables[1].formula = "b + 2 * нет".into();
    }
    let (_, each) = sample_values(&t, definition_of(&t));
    assert!(each[1].as_ref().unwrap_err().contains("«нет»"));
}

fn definition_of(t: &open_bim_model::ConnectionTemplate) -> &open_bim_model::CustomDefinition {
    match &t.kind {
        TemplateKind::Custom { definition } => definition,
        _ => panic!(),
    }
}

#[test]
fn the_form_runs_headless_and_a_saved_component_keeps_its_formulas() {
    let mut t = custom();
    let ctx = egui::Context::default();
    let mut changed = true;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        changed = definition_form(ui, &mut t);
    });
    output.textures_delta.clear();
    assert!(!changed, "nothing typed, nothing changed");
    let saved = UserComponent {
        version: 1,
        description: "Пластина по формулам".into(),
        template: t.clone(),
    };
    let json = serde_json::to_string(&saved).unwrap();
    let back: UserComponent = serde_json::from_str(&json).unwrap();
    assert_eq!(back, saved);
    assert!(matches!(back.component().kind, TemplateKind::Custom { .. }));
}
