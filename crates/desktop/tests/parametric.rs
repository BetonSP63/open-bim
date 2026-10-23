//! «Параметрическое моделирование»: part dimensions bound to model
//! variables follow every change of the variables in one undo step.
use open_bim_desktop::interaction::parametric::{apply, bind, current, unbind};
use open_bim_desktop::tools::Editor;
use open_bim_model::{ElementShape, ParamField, Parametric, parametric::Variable};

#[test]
fn bound_parts_follow_variables() {
    let mut e = Editor::new(open_bim_model::example_steel_frame().unwrap()).unwrap();
    let p = e.document.project();
    let member = *p
        .elements
        .iter()
        .find(|(_, el)| matches!(el.shape, ElementShape::Member { .. }))
        .unwrap()
        .0;
    let mut vars = p.parametric.clone();
    vars.variables = vec![
        Variable {
            name: "Пролёт".into(),
            formula: "6000".into(),
        },
        Variable {
            name: "Высота".into(),
            formula: "Пролёт / 2".into(),
        },
    ];
    let commands = apply(p, vars).unwrap();
    e.commit("Параметры", &commands).unwrap();
    let commands = bind(
        e.document.project(),
        &[member].into(),
        ParamField::EndZ,
        "Высота + 200",
    )
    .unwrap();
    e.commit("Связать", &commands).unwrap();
    assert_eq!(
        current(e.document.project(), member, ParamField::EndZ),
        Some(3200.)
    );
    // A new span moves the part.
    let mut p2: Parametric = e.document.project().parametric.clone();
    p2.variables[0].formula = "8000".into();
    let commands = apply(e.document.project(), p2).unwrap();
    assert_eq!(commands.len(), 2);
    e.commit("Параметры", &commands).unwrap();
    assert_eq!(
        current(e.document.project(), member, ParamField::EndZ),
        Some(4200.)
    );
    // Undo restores both the variable and the part.
    e.history(false).unwrap();
    assert_eq!(
        current(e.document.project(), member, ParamField::EndZ),
        Some(3200.)
    );
    assert_eq!(e.document.project().parametric.variables[0].formula, "6000");
    // Bad formula is refused; unbinding keeps the part where it is.
    let mut bad = e.document.project().parametric.clone();
    bad.variables[1].formula = "Нет + 1".into();
    assert!(apply(e.document.project(), bad).is_err());
    let commands = unbind(e.document.project(), &[member].into()).unwrap();
    e.commit("Снять", &commands).unwrap();
    assert!(e.document.project().parametric.bindings.is_empty());
    let json = serde_json::to_string(e.document.project()).unwrap();
    let back: open_bim_model::Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.parametric, e.document.project().parametric);
}
