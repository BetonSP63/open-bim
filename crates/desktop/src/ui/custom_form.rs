//! Form of a custom component in the component editor: variables with
//! formulas, plates, bolt groups. Values are shown for the sample I 300×150
//! the preview is built on, so a wrong formula shows next to it.
use eframe::egui;
use open_bim_model::{
    BUILTIN_NAMES, ConnectionTemplate, CustomBolts, CustomDefinition, CustomPlate, PlatePlane,
    TemplateKind, Variable, WeldTo, formula,
};
use std::collections::BTreeMap;

/// Built-in values on the preview sample: I 300×150×8×12 beam on an
/// I 300×150 column.
pub fn sample_builtins(t: &ConnectionTemplate) -> BTreeMap<String, f64> {
    BUILTIN_NAMES
        .into_iter()
        .zip([
            300.,
            150.,
            8.,
            12.,
            2850.,
            300.,
            150.,
            t.thickness,
            t.bolt_diameter,
            t.edge_distance,
        ])
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

/// Values of the variables on the sample, in order; a variable whose
/// formula fails gets its error instead and is left out for the rest.
pub fn sample_values(
    t: &ConnectionTemplate,
    def: &CustomDefinition,
) -> (BTreeMap<String, f64>, Vec<Result<f64, String>>) {
    let mut values = sample_builtins(t);
    let mut each = Vec::new();
    for v in &def.variables {
        let r = formula::eval(&v.formula, &values);
        if let Ok(x) = r {
            values.insert(v.name.clone(), x);
        }
        each.push(r);
    }
    (values, each)
}

const PLANES: [(PlatePlane, &str, &str); 3] = [
    (
        PlatePlane::End,
        "Торец",
        "Поперёк стержня; контур в (x, y), толщина наружу от торца",
    ),
    (
        PlatePlane::Web,
        "Вдоль стенки",
        "Параллельно стенке; контур в (z, y), толщина в сторону +x",
    ),
    (
        PlatePlane::Flange,
        "Вдоль полки",
        "Параллельно полкам; контур в (x, z), толщина в сторону +y",
    ),
];

fn formula_edit(
    ui: &mut egui::Ui,
    text: &mut String,
    values: &BTreeMap<String, f64>,
    width: f32,
) -> bool {
    let r = ui.add(egui::TextEdit::singleline(text).desired_width(width));
    match formula::eval(text, values) {
        Ok(v) => {
            ui.weak(format!("= {}", open_bim_connections::norms::mm(v)))
                .on_hover_text("Значение на образце I 300×150");
        }
        Err(e) => {
            ui.colored_label(ui.visuals().error_fg_color, "⚠")
                .on_hover_text(e);
        }
    }
    r.changed()
}

/// The form; true when the definition changed.
pub fn definition_form(ui: &mut egui::Ui, template: &mut ConnectionTemplate) -> bool {
    let probe = template.clone();
    let TemplateKind::Custom { definition: def } = &mut template.kind else {
        return false;
    };
    let mut changed = false;
    ui.add_space(6.);
    ui.heading("Переменные");
    ui.small(format!(
        "Формулы: + − * / ^, скобки, min, max, abs, sqrt, round, ceil, floor, if(условие, да, нет). Готовые имена: {} — сечение и длина основной детали, hs, bs — сечение опоры, t, d, e — толщина, болт и отступ узла.",
        ["h", "b", "tw", "tf", "L"].join(", ")
    ));
    let (values, each) = sample_values(&probe, def);
    let mut remove = None;
    let mut up = None;
    egui::Grid::new("custom-variables")
        .num_columns(5)
        .spacing([6., 4.])
        .show(ui, |ui| {
            ui.strong("Имя");
            ui.strong("Подпись");
            ui.strong("Формула");
            ui.strong("Значение");
            ui.label("");
            ui.end_row();
            for (i, v) in def.variables.iter_mut().enumerate() {
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut v.name).desired_width(70.))
                    .on_hover_text("Имя для формул: буквы, цифры и _")
                    .changed();
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut v.label).desired_width(120.))
                    .changed();
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut v.formula).desired_width(130.))
                    .changed();
                match &each[i] {
                    Ok(x) => {
                        ui.label(open_bim_connections::norms::mm(*x));
                    }
                    Err(e) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                }
                ui.horizontal(|ui| {
                    if i > 0 && ui.small_button("↑").on_hover_text("Выше").clicked() {
                        up = Some(i);
                    }
                    if ui
                        .small_button("✕")
                        .on_hover_text("Удалить переменную")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
                ui.end_row();
            }
        });
    if let Some(i) = up {
        def.variables.swap(i - 1, i);
        changed = true;
    }
    if let Some(i) = remove {
        def.variables.remove(i);
        changed = true;
    }
    if ui.button("＋ Переменная").clicked() {
        let n = (1..)
            .map(|i| format!("p{i}"))
            .find(|n| !def.variables.iter().any(|v| &v.name == n))
            .unwrap_or_default();
        def.variables.push(Variable {
            name: n,
            label: String::new(),
            formula: "0".into(),
        });
        changed = true;
    }

    ui.add_space(6.);
    ui.heading("Пластины");
    ui.small("Координаты от конца основной детали: x поперёк сечения, y вверх по сечению, z от торца внутрь детали.");
    let mut remove_plate = None;
    for (i, p) in def.plates.iter_mut().enumerate() {
        egui::CollapsingHeader::new(format!("{}. {}", i + 1, p.name))
            .id_salt(("custom-plate", i))
            .default_open(i == 0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let l = ui.label("Имя");
                    changed |= ui
                        .text_edit_singleline(&mut p.name)
                        .labelled_by(l.id)
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Плоскость");
                    let current = PLANES.iter().find(|x| x.0 == p.plane).expect("all planes");
                    egui::ComboBox::new(("custom-plane", i), "")
                        .selected_text(current.1)
                        .show_ui(ui, |ui| {
                            for (plane, name, hint) in PLANES {
                                if ui
                                    .selectable_label(p.plane == plane, name)
                                    .on_hover_text(hint)
                                    .clicked()
                                {
                                    p.plane = plane;
                                    changed = true;
                                }
                            }
                        });
                    ui.weak("ⓘ").on_hover_text(current.2);
                });
                ui.horizontal(|ui| {
                    ui.label(match p.plane {
                        PlatePlane::End => "z плоскости",
                        PlatePlane::Web => "x плоскости",
                        PlatePlane::Flange => "y плоскости",
                    });
                    changed |= formula_edit(ui, &mut p.offset, &values, 110.);
                });
                ui.horizontal(|ui| {
                    ui.label("Толщина");
                    changed |= formula_edit(ui, &mut p.thickness, &values, 110.);
                });
                ui.horizontal(|ui| {
                    ui.label("Шов");
                    for (to, name) in [
                        (None, "нет"),
                        (Some(WeldTo::Main), "к основной детали"),
                        (Some(WeldTo::Support), "к опоре"),
                    ] {
                        if ui.radio(p.weld == to, name).clicked() {
                            p.weld = to;
                            changed = true;
                        }
                    }
                });
                ui.small("Шов идёт по первой стороне контура (от точки 1 к точке 2).");
                let (a, b) = match p.plane {
                    PlatePlane::End => ("x", "y"),
                    PlatePlane::Web => ("z", "y"),
                    PlatePlane::Flange => ("x", "z"),
                };
                let mut remove_point = None;
                egui::Grid::new(("custom-contour", i))
                    .num_columns(3)
                    .show(ui, |ui| {
                        for (k, [u, v]) in p.contour.iter_mut().enumerate() {
                            ui.label(format!("Точка {}", k + 1));
                            ui.horizontal(|ui| {
                                ui.label(a);
                                changed |= formula_edit(ui, u, &values, 100.);
                                ui.label(b);
                                changed |= formula_edit(ui, v, &values, 100.);
                            });
                            if ui
                                .small_button("✕")
                                .on_hover_text("Удалить точку")
                                .clicked()
                            {
                                remove_point = Some(k);
                            }
                            ui.end_row();
                        }
                    });
                if let Some(k) = remove_point.filter(|_| p.contour.len() > 3) {
                    p.contour.remove(k);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    if ui.small_button("＋ Точка").clicked() {
                        let last = p.contour.last().cloned().unwrap_or_default();
                        p.contour.push(last);
                        changed = true;
                    }
                    if ui.small_button("Удалить пластину").clicked() {
                        remove_plate = Some(i);
                    }
                });
            });
    }
    if let Some(i) = remove_plate.filter(|_| def.plates.len() > 1) {
        def.plates.remove(i);
        def.bolts.retain(|g| g.plate != i);
        for g in &mut def.bolts {
            if g.plate > i {
                g.plate -= 1;
            }
        }
        changed = true;
    }
    if ui.button("＋ Пластина").clicked() {
        let s = |v: &str| v.to_string();
        def.plates.push(CustomPlate {
            name: format!("Пластина {}", def.plates.len() + 1),
            plane: PlatePlane::Web,
            offset: s("tw / 2"),
            contour: vec![
                [s("0"), s("-100")],
                [s("100"), s("-100")],
                [s("100"), s("100")],
                [s("0"), s("100")],
            ],
            thickness: s("t"),
            weld: None,
        });
        changed = true;
    }

    ui.add_space(6.);
    ui.heading("Группы болтов");
    let names: Vec<String> = def.plates.iter().map(|p| p.name.clone()).collect();
    let mut remove_group = None;
    for (j, g) in def.bolts.iter_mut().enumerate() {
        egui::CollapsingHeader::new(format!("Болты {}", j + 1))
            .id_salt(("custom-bolts", j))
            .default_open(j == 0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Через пластину");
                    egui::ComboBox::new(("custom-bolt-plate", j), "")
                        .selected_text(names.get(g.plate).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                if ui.selectable_label(g.plate == i, n).clicked() {
                                    g.plate = i;
                                    changed = true;
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Первый болт: u");
                    changed |= formula_edit(ui, &mut g.x, &values, 100.);
                    ui.label("v");
                    changed |= formula_edit(ui, &mut g.y, &values, 100.);
                });
                ui.horizontal(|ui| {
                    ui.label("Болтов по u");
                    changed |= ui
                        .add(egui::DragValue::new(&mut g.count_x).range(1..=8))
                        .changed();
                    ui.label("шаг");
                    changed |= formula_edit(ui, &mut g.spacing_x, &values, 90.);
                });
                ui.horizontal(|ui| {
                    ui.label("Болтов по v");
                    changed |= ui
                        .add(egui::DragValue::new(&mut g.count_y).range(1..=8))
                        .changed();
                    ui.label("шаг");
                    changed |= formula_edit(ui, &mut g.spacing_y, &values, 90.);
                });
                changed |= ui
                    .checkbox(&mut g.through_main, "Сверлить и основную деталь")
                    .changed();
                if ui.small_button("Удалить группу").clicked() {
                    remove_group = Some(j);
                }
            });
    }
    if let Some(j) = remove_group {
        def.bolts.remove(j);
        changed = true;
    }
    if ui.button("＋ Группа болтов").clicked() {
        let s = |v: &str| v.to_string();
        def.bolts.push(CustomBolts {
            plate: 0,
            x: s("0"),
            y: s("0"),
            count_x: 1,
            count_y: 2,
            spacing_x: s("3 * d"),
            spacing_y: s("3 * d"),
            through_main: false,
        });
        changed = true;
    }
    if let Err(e) = def.validate() {
        ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {e}"));
    }
    changed
}
