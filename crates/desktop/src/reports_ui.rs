//! Steel release lists in the GUI: assemblies by shop welds, part/assembly/
//! fastener lists regenerated from the model, CSV export and release set.
use crate::tools::Editor;
use crate::ui::commands::{Cmd, caption};
use eframe::egui;
use open_bim_reports::{Reports, csv, release};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Parts,
    Assemblies,
    Fasteners,
    /// «Обработка поверхности»: area per treatment.
    Coatings,
}

pub struct ReportsUi {
    pub reports: Option<Reports>,
    pub tab: Tab,
    pub directory: String,
    pub message: Option<String>,
}

impl Default for ReportsUi {
    fn default() -> Self {
        Self {
            reports: None,
            tab: Tab::Parts,
            directory: "release".into(),
            message: None,
        }
    }
}

impl ReportsUi {
    pub fn stale(&self, editor: &Editor) -> bool {
        self.reports
            .as_ref()
            .is_some_and(|r| r.revision != editor.document.project().revision)
    }

    pub fn refresh(&mut self, editor: &mut Editor) -> Result<(), String> {
        let r = open_bim_reports::generate(editor.document.project())?;
        self.message = Some(format!(
            "Ведомости по ревизии {}: деталей {}, масса {:.3} кг",
            r.revision,
            r.parts.iter().map(|p| p.quantity).sum::<usize>(),
            r.total_part_mass_kg
        ));
        self.reports = Some(r);
        editor.record("Ведомости сформированы");
        Ok(())
    }

    pub fn auto_assemblies(&mut self, editor: &mut Editor) -> Result<usize, String> {
        let commands = open_bim_reports::auto_assemblies(editor.document.project())?;
        if commands.is_empty() {
            return Ok(0);
        }
        editor.commit_data("Сборки по заводским швам", &commands)?;
        self.refresh(editor)?;
        Ok(commands.len())
    }

    pub fn export_csv(&mut self, editor: &mut Editor) -> Result<Vec<PathBuf>, String> {
        self.refresh(editor)?;
        let r = self.reports.as_ref().ok_or("Нет ведомостей")?;
        let dir = PathBuf::from(&self.directory);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut files = Vec::new();
        for (name, text) in [
            ("ведомость-деталей.csv", csv::parts(r)),
            ("ведомость-сборок.csv", csv::assemblies(r)),
            ("крепёж-и-сварка.csv", csv::fasteners(r)),
            ("покрытия.csv", csv::coatings(r)),
        ] {
            if name == "покрытия.csv" && r.coatings.is_empty() {
                continue;
            }
            let path = dir.join(name);
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
            files.push(path);
        }
        editor.record(&format!("Экспорт CSV {}", dir.display()));
        Ok(files)
    }

    /// Create missing drawings, refresh outdated ones, export PDFs and lists.
    pub fn release(&mut self, editor: &mut Editor) -> Result<usize, String> {
        let commands = release::drawing_commands(editor.document.project())?;
        if !commands.is_empty() {
            editor.commit_data("Комплект чертежей", &commands)?;
        }
        let summary = release::export(editor.document.project(), &PathBuf::from(&self.directory))?;
        editor.record(&format!(
            "Выпуск комплекта: {} чертежей, {} файлов",
            summary.drawings.len(),
            summary.files.len()
        ));
        self.refresh(editor)?;
        Ok(summary.drawings.len())
    }

    /// Panel controls; actions are returned as commands so long ones run as jobs.
    pub fn show_panel(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> Option<Cmd> {
        ui.heading("Ведомости");
        let mut cmd = None;
        for c in [Cmd::AutoAssemblies, Cmd::RefreshReports] {
            if ui.button(caption(c)).clicked() {
                cmd = Some(c);
            }
        }
        if self.stale(editor) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Ведомости устарели: модель изменена",
            );
        }
        ui.separator();
        ui.horizontal(|ui| {
            let label = ui.label("Папка выпуска");
            ui.text_edit_singleline(&mut self.directory)
                .labelled_by(label.id);
        });
        for c in [Cmd::ExportCsv, Cmd::ReleaseSet] {
            if ui
                .button(caption(c))
                .on_hover_text("Чертежи всех позиций, общий вид, PDF и ведомости")
                .clicked()
            {
                cmd = Some(c);
            }
        }
        if let Some(m) = &self.message {
            ui.label(m);
        }
        cmd
    }

    pub fn show_table(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Parts, "Детали");
            ui.selectable_value(&mut self.tab, Tab::Assemblies, "Сборки");
            ui.selectable_value(&mut self.tab, Tab::Fasteners, "Крепёж и сварка");
            ui.selectable_value(&mut self.tab, Tab::Coatings, "Покрытия");
        });
        let Some(r) = &self.reports else {
            ui.centered_and_justified(|ui| {
                ui.label("Нажмите «Сформировать ведомости» справа");
            });
            return;
        };
        egui::ScrollArea::both().show(ui, |ui| {
            let (header, rows): (Vec<&str>, Vec<Vec<String>>) = match self.tab {
                Tab::Parts => (
                    vec![
                        "Позиция",
                        "Наименование",
                        "Сечение",
                        "Материал",
                        "Кол-во",
                        "Длина, мм",
                        "Масса ед., кг",
                        "Масса общ., кг",
                    ],
                    r.parts
                        .iter()
                        .map(|p| {
                            vec![
                                p.position.clone(),
                                p.name.clone(),
                                p.section.clone(),
                                p.material.clone(),
                                p.quantity.to_string(),
                                open_bim_reports::fmt(p.length_mm),
                                format!("{:.3}", p.unit_mass_kg),
                                format!("{:.3}", p.total_mass_kg),
                            ]
                        })
                        .chain(std::iter::once(vec![
                            "Итого".into(),
                            String::new(),
                            String::new(),
                            String::new(),
                            r.parts
                                .iter()
                                .map(|p| p.quantity)
                                .sum::<usize>()
                                .to_string(),
                            String::new(),
                            String::new(),
                            format!("{:.3}", r.total_part_mass_kg),
                        ]))
                        .collect(),
                ),
                Tab::Assemblies => (
                    vec![
                        "Марка",
                        "Наименование",
                        "Главная деталь",
                        "Кол-во",
                        "Деталей",
                        "Масса ед., кг",
                        "Масса общ., кг",
                    ],
                    r.assemblies
                        .iter()
                        .map(|a| {
                            vec![
                                a.position.clone(),
                                a.name.clone(),
                                a.main_part.clone(),
                                a.quantity.to_string(),
                                a.parts_per_assembly.to_string(),
                                format!("{:.3}", a.unit_mass_kg),
                                format!("{:.3}", a.total_mass_kg),
                            ]
                        })
                        .chain(std::iter::once(vec![
                            "Итого".into(),
                            String::new(),
                            String::new(),
                            r.assemblies
                                .iter()
                                .map(|a| a.quantity)
                                .sum::<usize>()
                                .to_string(),
                            String::new(),
                            String::new(),
                            format!("{:.3}", r.total_assembly_mass_kg),
                        ]))
                        .collect(),
                ),
                Tab::Fasteners => (
                    vec!["Тип", "Наименование", "Кол-во", "Длина швов, мм"],
                    r.fasteners
                        .iter()
                        .map(|f| {
                            vec![
                                f.kind.clone(),
                                f.description.clone(),
                                f.quantity.to_string(),
                                open_bim_reports::fmt(f.length_mm),
                            ]
                        })
                        .collect(),
                ),
                Tab::Coatings => (
                    vec!["Обработка поверхности", "Кол-во деталей", "Площадь, м²"],
                    r.coatings
                        .iter()
                        .map(|c| {
                            vec![
                                c.treatment.clone(),
                                c.quantity.to_string(),
                                format!("{:.3}", c.area_m2),
                            ]
                        })
                        .collect(),
                ),
            };
            egui::Grid::new("report grid")
                .striped(true)
                .num_columns(header.len())
                .show(ui, |ui| {
                    for h in &header {
                        ui.strong(*h);
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.label(cell);
                        }
                        ui.end_row();
                    }
                });
            if !r.unassembled.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Детали вне сборок: {}", r.unassembled.len()),
                );
            }
        });
    }
}
