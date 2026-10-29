//! Grid properties as sections, the same in the pane of a selected grid
//! and in the window of a new one: coordinates, labels, line extensions,
//! origin, magnetic grid planes, line colour, label size and colour.
use crate::interaction::tools::GridSpec;
use eframe::egui;
use open_bim_model::GridSettings;

fn text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(super::property_pane::field_width(ui)))
        .labelled_by(l.id);
    ui.end_row();
}

fn colour(ui: &mut egui::Ui, label: &str, rgb: &mut [u8; 3]) {
    let l = ui.label(label);
    ui.horizontal(|ui| {
        ui.label(format!("{} {} {}", rgb[0], rgb[1], rgb[2]));
        ui.color_edit_button_srgb(rgb).labelled_by(l.id);
    });
    ui.end_row();
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    super::property_pane::section(title)
        .default_open(true)
        .show(ui, |ui| {
            super::property_pane::rows(ui, ("grid section", title), body);
        });
}

/// All grid sections; edits go to `spec` and `settings`.
pub fn sections(ui: &mut egui::Ui, spec: &mut GridSpec, settings: &mut GridSettings) {
    section(ui, "Координаты", |ui| {
        text(ui, "X", &mut spec.x);
        text(ui, "Y", &mut spec.y);
        text(ui, "Z", &mut spec.z);
    });
    ui.small("«0 4*6000» — первая линия в начале и четыре шага по 6000 мм; Z — отметки уровней");
    section(ui, "Подписи", |ui| {
        text(ui, "X", &mut spec.labels_x);
        text(ui, "Y", &mut spec.labels_y);
        text(ui, "Z", &mut spec.labels_z);
    });
    super::property_pane::section("Выступающие части линий")
        .default_open(true)
        .show(ui, |ui| {
            let [left, right, bottom, top] = &mut spec.extension;
            egui::Grid::new("grid extensions")
                .num_columns(3)
                .show(ui, |ui| {
                    ui.label("");
                    ui.label("Слева/снизу, мм");
                    ui.label("Справа/сверху, мм");
                    ui.end_row();
                    for (axis, low, high) in [("X", left, right), ("Y", bottom, top)] {
                        let l = ui.label(axis);
                        ui.add(egui::TextEdit::singleline(low).desired_width(80.))
                            .labelled_by(l.id);
                        ui.add(egui::TextEdit::singleline(high).desired_width(80.))
                            .labelled_by(l.id);
                        ui.end_row();
                    }
                });
        });
    section(ui, "Начало координат", |ui| {
        let [x, y, z] = &mut spec.origin;
        text(ui, "X, мм", x);
        text(ui, "Y, мм", y);
        text(ui, "Z, мм", z);
    });
    section(
        ui,
        "Магнитная плоскость сетки",
        |ui| {
            let l = ui.label("Магнитные");
            egui::ComboBox::new("grid magnetic", "")
                .selected_text(if settings.magnetic { "Да" } else { "Нет" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut settings.magnetic, true, "Да");
                    ui.selectable_value(&mut settings.magnetic, false, "Нет");
                })
                .response
                .labelled_by(l.id)
                .on_hover_text(
                    "Концы балок и колонн на линиях сетки следуют за линиями при изменении сетки",
                );
            ui.end_row();
        },
    );
    section(ui, "Цвет сетки", |ui| {
        colour(ui, "Цвет", &mut settings.color)
    });
    section(
        ui,
        "Размер и цвет шрифта подписи",
        |ui| {
            let l = ui.label("Размер шрифта");
            ui.add(egui::DragValue::new(&mut settings.label_size).range(6.0..=48.0))
                .labelled_by(l.id);
            ui.end_row();
            colour(ui, "Цвет текста", &mut settings.label_color);
        },
    );
}
