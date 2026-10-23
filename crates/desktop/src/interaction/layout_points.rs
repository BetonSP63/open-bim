//! «Диспетчер разбивок»: setting-out points of the model for the site —
//! construction points and the plan intersections of grid axes at a chosen
//! level — with coordinates from the project base point and CSV export.
use crate::tools::Editor;
use eframe::egui;
use open_bim_model::{ConstructionKind, ObjectId, Project};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutPoint {
    pub name: String,
    /// Model coordinates minus the project origin, mm.
    pub at: [f64; 3],
    /// The construction point, if the row is one.
    pub id: Option<ObjectId>,
}

/// Plan intersection of two axes (their lines, extended), if not parallel.
fn cross(a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> Option<[f64; 2]> {
    let (p, r) = ([a.0[0], a.0[1]], [a.1[0] - a.0[0], a.1[1] - a.0[1]]);
    let (q, s) = ([b.0[0], b.0[1]], [b.1[0] - b.0[0], b.1[1] - b.0[1]]);
    let den = r[0] * s[1] - r[1] * s[0];
    let scale = (r[0].hypot(r[1]) * s[0].hypot(s[1])).max(1e-9);
    if den.abs() < 1e-6 * scale {
        return None;
    }
    let t = ((q[0] - p[0]) * s[1] - (q[1] - p[1]) * s[0]) / den;
    Some([p[0] + t * r[0], p[1] + t * r[1]])
}

/// Setting-out points: construction points «Т1…» in id order, then grid
/// nodes «A/1» at `level` (its elevation; model zero without levels).
pub fn points(project: &Project, level: Option<ObjectId>) -> Vec<LayoutPoint> {
    let o = project.origin.0;
    let rel = |p: [f64; 3]| [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
    let mut out: Vec<LayoutPoint> = project
        .construction
        .values()
        .filter_map(|c| match c.kind {
            ConstructionKind::Point { at } => Some((c.id, at)),
            _ => None,
        })
        .enumerate()
        .map(|(i, (id, at))| LayoutPoint {
            name: format!("Т{}", i + 1),
            at: rel(at.0),
            id: Some(id),
        })
        .collect();
    let z = level
        .and_then(|l| project.levels.get(&l))
        .map_or(0., |l| l.elevation_mm);
    let axes: Vec<_> = project.axes.values().collect();
    for (i, a) in axes.iter().enumerate() {
        for b in &axes[i + 1..] {
            if let Some([x, y]) = cross((a.start.0, a.end.0), (b.start.0, b.end.0)) {
                // Letter axes first: «Б/2», not «2/Б».
                let (first, second) = if b.name.starts_with(|c: char| c.is_alphabetic())
                    && !a.name.starts_with(|c: char| c.is_alphabetic())
                {
                    (&b.name, &a.name)
                } else {
                    (&a.name, &b.name)
                };
                out.push(LayoutPoint {
                    name: format!("{first}/{second}"),
                    at: rel([x, y, z]),
                    id: None,
                });
            }
        }
    }
    out
}

/// The points as CSV (semicolon, decimal comma, UTF-8 with BOM).
pub fn csv(points: &[LayoutPoint]) -> String {
    let mut out = String::from("\u{feff}Точка;X, мм;Y, мм;Z, мм\r\n");
    for p in points {
        let cells: Vec<String> = std::iter::once(open_bim_reports::csv::field(&p.name))
            .chain(p.at.iter().map(|v| format!("{v:.1}").replace('.', ",")))
            .collect();
        out.push_str(&cells.join(";"));
        out.push_str("\r\n");
    }
    out
}

/// «разбивка.csv» next to the model file (the working folder when unsaved).
pub fn csv_path(project_path: &str) -> PathBuf {
    match Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join("разбивка.csv"),
        _ => PathBuf::from("разбивка.csv"),
    }
}

#[derive(Default)]
pub struct LayoutManager {
    pub open: bool,
    pub level: Option<ObjectId>,
    pub message: Option<String>,
}

impl LayoutManager {
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor, project_path: &str) {
        if !self.open {
            return;
        }
        let project = editor.document.project();
        if self.level.is_none_or(|l| !project.levels.contains_key(&l)) {
            self.level = project
                .levels
                .values()
                .min_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm))
                .map(|l| l.id);
        }
        let rows = points(project, self.level);
        let level_name = |id: Option<ObjectId>| {
            id.and_then(|l| project.levels.get(&l))
                .map_or("0,000".to_string(), |l| l.name.clone())
        };
        let mut level = self.level;
        let (mut export, mut select) = (false, None);
        let mut open = true;
        egui::Window::new("Диспетчер разбивок")
            .id(egui::Id::new("layout manager"))
            .open(&mut open)
            .collapsible(false)
            .default_width(460.)
            .show(ctx, |ui| {
                if !project.levels.is_empty() {
                    egui::ComboBox::new("layout level", "Уровень узлов сетки")
                        .selected_text(level_name(level))
                        .show_ui(ui, |ui| {
                            for l in project.levels.values() {
                                ui.selectable_value(&mut level, Some(l.id), &l.name);
                            }
                        });
                }
                ui.small("Координаты от базовой точки проекта, мм");
                if rows.is_empty() {
                    ui.weak("Точек нет: постройте сетку или точки («Точка»)");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.)
                    .show(ui, |ui| {
                        egui::Grid::new("layout grid")
                            .num_columns(4)
                            .striped(true)
                            .show(ui, |ui| {
                                for h in ["Точка", "X", "Y", "Z"] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for (i, p) in rows.iter().enumerate() {
                                    if ui.selectable_label(false, &p.name).clicked() {
                                        select = Some(i);
                                    }
                                    for v in p.at {
                                        ui.label(open_bim_reports::fmt(v));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                export = ui
                    .add_enabled(!rows.is_empty(), egui::Button::new("Экспорт в CSV"))
                    .clicked();
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        self.level = level;
        if let Some(p) = select.map(|i| &rows[i]) {
            match p.id {
                Some(id) => {
                    editor.selected = [id].into();
                    self.message = Some(format!("Выбрана точка {}", p.name));
                }
                None => self.message = Some(format!("Узел сетки {}", p.name)),
            }
        }
        if export {
            let path = csv_path(project_path);
            self.message = Some(match std::fs::write(&path, csv(&rows)) {
                Ok(()) => format!("Записано точек: {} — {}", rows.len(), path.display()),
                Err(e) => format!("{}: {e}", path.display()),
            });
        }
    }
}
