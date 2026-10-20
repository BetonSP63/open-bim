//! Issue, change and release of the sets from the drawings pane (ГОСТ Р
//! 21.101-2026, sections 7, 8): «Выдать комплект», «Записать изменение»
//! with the permission, date, content and reason, «Выпустить пакет ПДЭ».
use super::DrawingEditor;
use crate::tools::Editor;
use eframe::egui;
use open_bim_reports::gost_release::{self, ChangeRequest};

/// Fields of the change form.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeForm {
    pub mark: String,
    pub permission: String,
    pub date: String,
    pub description: String,
    pub reason: Option<u8>,
    pub replace: bool,
    pub package_dir: String,
}

impl Default for ChangeForm {
    fn default() -> Self {
        Self {
            mark: "КМ".into(),
            permission: String::new(),
            date: today(),
            description: String::new(),
            reason: None,
            replace: false,
            package_dir: "package".into(),
        }
    }
}

/// Today as «ДД.ММ.ГГ» (UTC civil date from the system clock).
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (y, m, d) = civil(secs.div_euclid(86_400));
    open_bim_model::format_date(y as i32, m, d)
}

/// Days since 1970-01-01 to (year, month, day) — Howard Hinnant's algorithm.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

impl DrawingEditor {
    fn request(&self, editor: &Editor) -> ChangeRequest {
        let author = editor.document.project().document.developer.name.clone();
        ChangeRequest {
            mark: self.change.mark.clone(),
            permission: self.change.permission.clone(),
            date: self.change.date.clone(),
            description: self.change.description.clone(),
            reason: self.change.reason,
            author,
            replace: self.change.replace,
        }
    }

    /// Issue every sheet of the chosen set.
    pub fn issue_set(&mut self, editor: &mut Editor) -> Result<(), String> {
        let mark = self.change.mark.clone();
        let commands = gost_release::issue_set(editor.document.project(), &mark)?;
        editor.commit_data(&format!("Выдать комплект {mark}"), &commands)?;
        self.message = Some(format!("Комплект {mark} выдан: листов {}", commands.len()));
        Ok(())
    }

    /// Record the next change of the chosen set and reissue its sheets.
    pub fn record_change(&mut self, editor: &mut Editor) -> Result<(), String> {
        let request = self.request(editor);
        let command = gost_release::record_change(editor.document.project(), &request)?;
        let open_bim_commands::Command::RecordChange { change, .. } = &command else {
            unreachable!("record_change returns RecordChange")
        };
        let text = format!(
            "Изм. {} комплекта {}: листов {}",
            change.number,
            change.mark,
            change.sheets.len()
        );
        editor.commit_data(&format!("Изменение {}", request.permission), &[command])?;
        self.message = Some(text);
        Ok(())
    }

    /// Annul the current sheet under the permission of the form.
    pub fn annul_current(&mut self, editor: &mut Editor) -> Result<(), String> {
        let id = self.current.ok_or("Выберите лист")?;
        let request = self.request(editor);
        let commands = gost_release::annul_sheet(editor.document.project(), id, &request)?;
        editor.commit_data("Аннулировать лист", &commands)?;
        self.current = None;
        self.message = Some("Лист аннулирован: номер сохранён в ведомости".into());
        Ok(())
    }

    /// Write the package of electronic documents with certifying sheets.
    pub fn export_package(&mut self, editor: &Editor) -> Result<(), String> {
        let dir = std::path::PathBuf::from(self.change.package_dir.trim());
        let stamp = format!("{} {}", self.change.date.trim(), clock());
        let package = gost_release::export_package(editor.document.project(), &dir, &stamp)?;
        self.message = Some(format!(
            "Пакет {}: файлов {}, УЛ {}",
            dir.display(),
            package.files.len(),
            package.certifying_sheets.len()
        ));
        Ok(())
    }

    pub(super) fn changes_section(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        ui.strong("Изменения и выпуск (ГОСТ Р 21.101, разд. 7, 8)");
        egui::Grid::new("change form")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Комплект");
                ui.horizontal(|ui| {
                    for m in open_bim_model::SET_MARKS {
                        ui.radio_value(&mut self.change.mark, m.to_string(), m);
                    }
                });
                ui.end_row();
                for (caption, value) in [
                    ("Разрешение", &mut self.change.permission),
                    ("Дата ДД.ММ.ГГ", &mut self.change.date),
                ] {
                    let l = ui.label(caption);
                    ui.add(egui::TextEdit::singleline(value).desired_width(140.))
                        .labelled_by(l.id);
                    ui.end_row();
                }
                let l = ui.label("Содержание");
                ui.add(
                    egui::TextEdit::multiline(&mut self.change.description)
                        .desired_rows(2)
                        .desired_width(240.),
                )
                .labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Код причины");
                let current = self
                    .change
                    .reason
                    .and_then(|r| open_bim_model::REASONS.iter().find(|x| x.0 == r))
                    .map_or("—".to_string(), |r| format!("{} — {}", r.0, r.1));
                egui::ComboBox::from_id_salt("change reason")
                    .selected_text(current)
                    .width(240.)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.change.reason, None, "—");
                        for (code, text) in open_bim_model::REASONS {
                            ui.selectable_value(
                                &mut self.change.reason,
                                Some(code),
                                format!("{code} — {text}"),
                            );
                        }
                    })
                    .response
                    .labelled_by(l.id);
                ui.end_row();
            });
        ui.checkbox(
            &mut self.change.replace,
            "Заменить листы («Зам.») без отметки участков",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Выдать комплект").clicked() {
                let r = self.issue_set(editor);
                super::panel_report(self, r);
            }
            if ui
                .button("Записать изменение")
                .on_hover_text("Изменённые после выдачи листы: участки, графы 14–19, ведомость")
                .clicked()
            {
                let r = self.record_change(editor);
                super::panel_report(self, r);
            }
            if ui.button("Аннулировать лист").clicked() {
                let r = self.annul_current(editor);
                super::panel_report(self, r);
            }
        });
        ui.horizontal(|ui| {
            let l = ui.label("Папка пакета");
            ui.add(egui::TextEdit::singleline(&mut self.change.package_dir).desired_width(140.))
                .labelled_by(l.id);
            if ui
                .button("Выпустить пакет ПДЭ")
                .on_hover_text("PDF комплектов, обложки, титульные листы, УЛ с SHA-256, info.xml")
                .clicked()
            {
                let r = self.export_package(editor);
                super::panel_report(self, r);
            }
        });
    }
}

/// Current UTC time «ЧЧ:ММ».
fn clock() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{:02}:{:02}", secs / 3600 % 24, secs / 60 % 60)
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_746), (2026, 10, 20));
    }
}
