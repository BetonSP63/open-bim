//! «Выбор профиля»: the profile catalogue in its own window. A tree on the
//! left (profiles of the project, then the built-in ГОСТ catalogue by type and
//! standard), a search field above it, and on the right a drawing of the
//! section with its dimensions and properties. «ОК» takes the profile and
//! closes the window, «Применить» takes it and keeps the window open.
//!
//! Profile fields (property pane, contextual toolbar, part tools) show the
//! current name as a button that opens this window.
use super::catalog_picker::{Pick, profile_by_name};
use eframe::egui;
use open_bim_model::library::{self, Entry, Family};
use open_bim_model::sections::SectionKind;
use open_bim_model::{ObjectId, Profile, ProfileShape, Project, SectionDefinition};

/// The row highlighted in the tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Selected {
    Project(ObjectId),
    Catalog(String),
    /// A user section (project definition or template) at its default
    /// sizes, by profile name.
    Section(String),
}

/// The user section whose default profile is named `name`.
fn section_definition(project: &Project, name: &str) -> Option<SectionDefinition> {
    project.sections.all().into_iter().find(|d| {
        d.defaults()
            .and_then(|v| d.profile_name(&v))
            .is_ok_and(|n| n == name)
    })
}

/// The user section profile named `name`, under the project's next free id.
fn section_profile(project: &Project, name: &str) -> Option<Profile> {
    let d = section_definition(project, name)?;
    d.profile(ObjectId(project.next_id), &d.defaults().ok()?)
        .ok()
}

/// What the buttons of the window asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Take the profile and close the window («ОК», double click).
    Ok,
    /// Take the profile and keep the window open.
    Apply,
    Cancel,
}

/// Types shown as the first level of the catalogue tree.
pub const GROUPS: [(&str, &[Family]); 5] = [
    (
        "Двутавры",
        &[
            Family::IBeamNormal57837,
            Family::IBeamWide57837,
            Family::IBeamColumn57837,
            Family::IBeamExtraNormal57837,
            Family::IBeamExtraColumn57837,
            Family::IBeamPile57837,
            Family::IBeamNormal,
            Family::IBeamWide,
            Family::IBeamColumn,
            Family::IBeamSloped,
            Family::IBeamNormalSto,
            Family::IBeamWideSto,
            Family::IBeamColumnSto,
        ],
    ),
    (
        "Тавры",
        &[Family::TeeNormal, Family::TeeWide, Family::TeeColumn],
    ),
    ("Швеллеры", &[Family::Channel, Family::ChannelP]),
    ("Уголки", &[Family::Angle, Family::AngleUnequal]),
    (
        "Трубы",
        &[
            Family::Pipe,
            Family::PipeWaterGas,
            Family::SquareTube,
            Family::RectTube,
        ],
    ),
];

/// State of one profile window; one per field, kept in egui memory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileDialog {
    pub open: bool,
    pub query: String,
    pub selected: Option<Selected>,
}

impl ProfileDialog {
    /// Opens the window on the field's current profile.
    pub fn open_on(&mut self, project: &Project, current: Option<ObjectId>) {
        self.open = true;
        self.query.clear();
        self.selected = current
            .filter(|id| project.profiles.contains_key(id))
            .map(Selected::Project);
    }

    /// The profile the highlighted row stands for: the project's copy, or a
    /// catalogue record to add under the project's next free id.
    pub fn pick(&self, project: &Project) -> Option<Pick<Profile>> {
        match self.selected.as_ref()? {
            Selected::Project(id) => project
                .profiles
                .contains_key(id)
                .then_some(Pick::Project(*id)),
            Selected::Catalog(name) => profile_by_name(project, name),
            Selected::Section(name) => match project.profiles.values().find(|p| p.name == *name) {
                Some(p) => Some(Pick::Project(p.id)),
                None => section_profile(project, name).map(Pick::New),
            },
        }
    }

    /// Applies a button: returns the profile to use (if any) and closes
    /// the window on «ОК» and «Отмена».
    pub fn finish(&mut self, project: &Project, outcome: Outcome) -> Option<Pick<Profile>> {
        let pick = match outcome {
            Outcome::Ok | Outcome::Apply => self.pick(project),
            Outcome::Cancel => None,
        };
        if outcome != Outcome::Apply {
            self.open = false;
        }
        pick
    }

    /// Draws the window when open; returns the profile chosen by «ОК» or
    /// «Применить».
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        salt: &str,
        project: &Project,
    ) -> Option<Pick<Profile>> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut outcome = None;
        egui::Window::new("Выбор профиля")
            .id(egui::Id::new((salt, "profile dialog")))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                outcome = self.body(ui, salt, project);
            });
        if !open {
            outcome = Some(Outcome::Cancel);
        }
        outcome.and_then(|o| self.finish(project, o))
    }

    fn body(&mut self, ui: &mut egui::Ui, salt: &str, project: &Project) -> Option<Outcome> {
        let mut outcome = None;
        ui.horizontal(|ui| {
            let l = ui.label("Поиск");
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("30Б1, ш1, труба 219")
                    .desired_width(240.),
            )
            .labelled_by(l.id);
            if !self.query.is_empty() && ui.button("Сбросить").clicked() {
                self.query.clear();
            }
        });
        ui.separator();
        // Both columns scroll inside a fixed height, so the buttons below
        // stay in place whatever the tree or the drawing need.
        let height = 400.;
        let size = egui::vec2(700., height);
        let row = egui::Layout::left_to_right(egui::Align::Min);
        ui.allocate_ui_with_layout(size, row, |ui| {
            // The vertical separator spans the row's height: keep it fixed,
            // or an auto-sized window grows by it every frame.
            ui.set_height(height);
            ui.vertical(|ui| {
                ui.set_width(300.);
                egui::ScrollArea::vertical()
                    .id_salt((salt, "profile tree"))
                    .max_height(height)
                    .auto_shrink([false, true])
                    .min_scrolled_height(height)
                    .show(ui, |ui| {
                        if self.tree(ui, salt, project) {
                            outcome = Some(Outcome::Ok);
                        }
                    });
            });
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt((salt, "profile details"))
                    .max_height(height)
                    .auto_shrink([false, true])
                    .min_scrolled_height(height)
                    .show(ui, |ui| self.details(ui, project));
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let ready = self.pick(project).is_some();
            if ui.add_enabled(ready, egui::Button::new("ОК")).clicked() {
                outcome = Some(Outcome::Ok);
            }
            if ui
                .add_enabled(ready, egui::Button::new("Применить"))
                .clicked()
            {
                outcome = Some(Outcome::Apply);
            }
            if ui.button("Отмена").clicked() {
                outcome = Some(Outcome::Cancel);
            }
            ui.separator();
            if ui.button("Редактор сечений…").clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new(super::section_editor::OPEN_REQUEST), true)
                });
            }
        });
        outcome
    }

    /// The tree; true when a row was double-clicked.
    fn tree(&mut self, ui: &mut egui::Ui, salt: &str, project: &Project) -> bool {
        let searching = !self.query.trim().is_empty();
        let mut chosen = false;
        let own: Vec<&Profile> = project
            .profiles
            .values()
            .filter(|p| matches(&p.name, &self.query))
            .collect();
        egui::CollapsingHeader::new(format!("Профили проекта ({})", own.len()))
            .id_salt((salt, "project profiles"))
            .default_open(true)
            .open(searching.then_some(true))
            .show(ui, |ui| {
                for p in own {
                    let sel = Selected::Project(p.id);
                    let r = ui.selectable_label(self.selected.as_ref() == Some(&sel), &p.name);
                    if r.clicked() {
                        self.selected = Some(sel);
                    }
                    chosen |= r.double_clicked();
                }
            });
        let found = library::search(&self.query);
        for (group, families) in GROUPS {
            let count = found
                .iter()
                .filter(|e| families.contains(&e.family))
                .count();
            if count == 0 {
                continue;
            }
            egui::CollapsingHeader::new(format!("{group} ({count})"))
                .id_salt((salt, group))
                .open(searching.then_some(true))
                .show(ui, |ui| {
                    for family in families.iter().copied() {
                        let rows: Vec<&Entry> =
                            found.iter().filter(|e| e.family == family).collect();
                        if rows.is_empty() {
                            continue;
                        }
                        egui::CollapsingHeader::new(family.label())
                            .id_salt((salt, family.label()))
                            .open(searching.then_some(true))
                            .show(ui, |ui| {
                                for e in rows {
                                    let sel = Selected::Catalog(e.name.clone());
                                    let r = ui.selectable_label(
                                        self.selected.as_ref() == Some(&sel),
                                        &e.name,
                                    );
                                    if r.clicked() {
                                        self.selected = Some(sel);
                                    }
                                    chosen |= r.double_clicked();
                                }
                            });
                    }
                });
        }
        // User sections: welded and contour, at their default sizes; other
        // sizes are made in «Редактор сечений».
        let sections = project.sections.all();
        for kind in [SectionKind::Welded, SectionKind::Contour] {
            let rows: Vec<String> = sections
                .iter()
                .filter(|d| d.kind() == kind)
                .filter_map(|d| d.profile_name(&d.defaults().ok()?).ok())
                .filter(|n| matches(n, &self.query))
                .collect();
            if rows.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!("{} ({})", kind.label(), rows.len()))
                .id_salt((salt, kind.label()))
                .open(searching.then_some(true))
                .show(ui, |ui| {
                    for name in rows {
                        let sel = Selected::Section(name.clone());
                        let r = ui.selectable_label(self.selected.as_ref() == Some(&sel), &name);
                        if r.clicked() {
                            self.selected = Some(sel);
                        }
                        chosen |= r.double_clicked();
                    }
                });
        }
        chosen
    }

    /// Drawing and properties of the highlighted profile.
    fn details(&self, ui: &mut egui::Ui, project: &Project) {
        let Some(info) = self.info(project) else {
            ui.weak("Выберите профиль в списке слева");
            return;
        };
        ui.heading(&info.name);
        if let Some(s) = &info.standard {
            ui.label(s);
        }
        preview(ui, &info.shape);
        egui::Grid::new("profile dialog properties")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (label, value) in properties(&info) {
                    ui.label(label);
                    ui.label(value);
                    ui.end_row();
                }
            });
    }

    fn info(&self, project: &Project) -> Option<Info> {
        match self.selected.as_ref()? {
            Selected::Project(id) => {
                let p = project.profiles.get(id)?;
                Some(Info {
                    name: p.name.clone(),
                    shape: p.shape.clone(),
                    standard: p.standard.clone(),
                    area_mm2: p.area_mm2().ok(),
                    radius: library::section(&p.name).map(|e| e.radius),
                })
            }
            Selected::Catalog(name) => {
                let e = library::section(name)?;
                Some(Info {
                    name: e.name.clone(),
                    shape: e.shape.clone(),
                    standard: Some(e.standard.into()),
                    area_mm2: Some(e.area_mm2),
                    radius: Some(e.radius),
                })
            }
            Selected::Section(name) => {
                let p = section_profile(project, name)?;
                Some(Info {
                    name: p.name.clone(),
                    area_mm2: p.area_mm2().ok(),
                    shape: p.shape,
                    standard: Some(p.source),
                    radius: None,
                })
            }
        }
    }
}

/// What the right side shows about a profile.
pub struct Info {
    pub name: String,
    pub shape: ProfileShape,
    pub standard: Option<String>,
    pub area_mm2: Option<f64>,
    pub radius: Option<f64>,
}

fn matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    query
        .split_whitespace()
        .all(|w| name.contains(&w.to_lowercase()))
}

/// «12,5» in millimetres, decimal comma, no trailing zeros.
fn mm(v: f64) -> String {
    super::property_pane::number_text(v).replace('.', ",")
}

/// Rows of the properties table: dimensions, area and mass per metre.
pub fn properties(info: &Info) -> Vec<(&'static str, String)> {
    let mut rows: Vec<(&'static str, String)> = match &info.shape {
        ProfileShape::Polygon { .. } => {
            let [h, b, ..] = info.shape.sizes();
            vec![("h, мм", mm(h)), ("b, мм", mm(b))]
        }
        ProfileShape::Welded { plates, welds } => {
            let [h, b, ..] = info.shape.sizes();
            let mut rows = vec![("h, мм", mm(h)), ("b, мм", mm(b))];
            rows.push(("Листов", plates.len().to_string()));
            rows.push(("Поясных швов", welds.len().to_string()));
            rows
        }
        _ => Vec::new(),
    };
    rows.extend(match info.shape {
        ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } => vec![],
        ProfileShape::ISection {
            height,
            width,
            web,
            flange,
        }
        | ProfileShape::Channel {
            height,
            width,
            web,
            flange,
        } => vec![
            ("h, мм", mm(height)),
            ("b, мм", mm(width)),
            ("s, мм", mm(web)),
            ("t, мм", mm(flange)),
        ],
        ProfileShape::Angle {
            height,
            width,
            thickness,
        } => vec![
            ("B, мм", mm(height)),
            ("b, мм", mm(width)),
            ("t, мм", mm(thickness)),
        ],
        ProfileShape::CircularHollow {
            diameter,
            thickness,
        } => vec![("D, мм", mm(diameter)), ("t, мм", mm(thickness))],
        ProfileShape::RectangularHollow {
            height,
            width,
            thickness,
        } => vec![
            ("H, мм", mm(height)),
            ("B, мм", mm(width)),
            ("t, мм", mm(thickness)),
        ],
        ProfileShape::Rectangle { height, width } => {
            vec![("h, мм", mm(height)), ("b, мм", mm(width))]
        }
    });
    if let Some(r) = info.radius.filter(|r| *r > 0.) {
        rows.push(("R, мм", mm(r)));
    }
    if let Some(a) = info.area_mm2 {
        rows.push(("A, см²", format!("{:.2}", a / 100.).replace('.', ",")));
        rows.push((
            "Масса, кг/м",
            format!("{:.2}", a * 7850.0 / 1e6).replace('.', ","),
        ));
    }
    rows
}

/// The section to scale with its overall height and width dimensioned.
fn preview(ui: &mut egui::Ui, shape: &ProfileShape) {
    if let ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } = shape {
        super::section_editor::paint_section(ui, shape, egui::vec2(260., 210.));
        return;
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(260., 210.), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2., visuals.extreme_bg_color);
    let (h, w) = match *shape {
        ProfileShape::ISection { height, width, .. }
        | ProfileShape::Channel { height, width, .. }
        | ProfileShape::Angle { height, width, .. }
        | ProfileShape::RectangularHollow { height, width, .. }
        | ProfileShape::Rectangle { height, width } => (height, width),
        ProfileShape::CircularHollow { diameter, .. } => (diameter, diameter),
        ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } => unreachable!(),
    };
    if !(h > 0. && w > 0.) {
        return;
    }
    // Room for the dimension lines on the left and below.
    let area = egui::Rect::from_min_max(
        rect.min + egui::vec2(46., 12.),
        rect.max - egui::vec2(12., 40.),
    );
    let scale = (area.width() as f64 / w).min(area.height() as f64 / h) as f32;
    let centre = area.center();
    // Section axes: y up, origin at the centre of the bounding box (an angle
    // is drawn from its heel, so shift it back to its box centre).
    let (dx, dy) = match shape {
        ProfileShape::Angle { .. } => (-w / 2., -h / 2.),
        ProfileShape::Channel { .. } => (-w / 2., 0.),
        _ => (0., 0.),
    };
    let to_screen = |p: [f64; 2]| {
        centre + egui::vec2(((p[0] + dx) as f32) * scale, -((p[1] + dy) as f32) * scale)
    };
    let fill = visuals.selection.bg_fill.gamma_multiply(0.5);
    let stroke = egui::Stroke::new(1.5, visuals.strong_text_color());
    match *shape {
        ProfileShape::CircularHollow {
            diameter,
            thickness,
        } => {
            let r = diameter as f32 / 2. * scale;
            painter.circle(centre, r, fill, stroke);
            painter.circle(
                centre,
                r - thickness as f32 * scale,
                visuals.extreme_bg_color,
                stroke,
            );
        }
        ProfileShape::RectangularHollow { thickness, .. } => {
            let outer = egui::Rect::from_center_size(
                centre,
                egui::vec2(w as f32 * scale, h as f32 * scale),
            );
            let inner = outer.shrink(thickness as f32 * scale);
            painter.rect(outer, 0., fill, stroke, egui::StrokeKind::Middle);
            painter.rect(
                inner,
                0.,
                visuals.extreme_bg_color,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        _ => {
            if let Ok(contour) = shape.solid_contour() {
                let points: Vec<egui::Pos2> = contour.into_iter().map(to_screen).collect();
                // egui fills only convex shapes; the outline is enough here.
                painter.add(egui::Shape::closed_line(points, stroke));
            }
        }
    }
    // Overall dimensions.
    let half = egui::vec2(w as f32 * scale / 2., h as f32 * scale / 2.);
    let top = centre.y - half.y;
    let bottom = centre.y + half.y;
    let left = centre.x - half.x;
    let right = centre.x + half.x;
    let dim = egui::Stroke::new(1., visuals.text_color());
    let font = egui::FontId::proportional(12.);
    let x = left - 16.;
    painter.line_segment([egui::pos2(x, top), egui::pos2(x, bottom)], dim);
    for y in [top, bottom] {
        painter.line_segment([egui::pos2(x - 4., y), egui::pos2(left - 2., y)], dim);
    }
    painter.text(
        egui::pos2(x - 4., centre.y),
        egui::Align2::RIGHT_CENTER,
        mm(h),
        font.clone(),
        visuals.text_color(),
    );
    let y = bottom + 14.;
    painter.line_segment([egui::pos2(left, y), egui::pos2(right, y)], dim);
    for x in [left, right] {
        painter.line_segment([egui::pos2(x, bottom + 2.), egui::pos2(x, y + 4.)], dim);
    }
    painter.text(
        egui::pos2(centre.x, y + 3.),
        egui::Align2::CENTER_TOP,
        mm(w),
        font,
        visuals.text_color(),
    );
}

/// A profile field: the current name on a button that opens «Выбор профиля».
/// Returns the profile chosen with «ОК» or «Применить».
pub fn profile_field(
    ui: &mut egui::Ui,
    salt: &str,
    project: &Project,
    current: Option<ObjectId>,
    shown: &str,
    accessible: &str,
    width: f32,
) -> Option<Pick<Profile>> {
    let id = egui::Id::new((salt, "profile dialog state"));
    let mut dialog: ProfileDialog = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    let accessible = accessible.to_string();
    let response = ui
        .add(egui::Button::new(format!("{shown}  …")).min_size(egui::vec2(width, 0.)))
        .on_hover_text("Выбор профиля из каталога");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, accessible.clone())
    });
    if response.clicked() {
        dialog.open_on(project, current);
    }
    let picked = dialog.show(ui.ctx(), salt, project);
    ui.data_mut(|d| d.insert_temp(id, dialog));
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_family_has_a_type_group() {
        for family in Family::ALL {
            let n = GROUPS.iter().filter(|(_, f)| f.contains(&family)).count();
            assert_eq!(n, 1, "{family:?}");
        }
    }

    #[test]
    fn user_sections_are_picked_as_new_profiles_with_a_preview() {
        let project = Project::default();
        let dialog = ProfileDialog {
            open: true,
            query: String::new(),
            selected: Some(Selected::Section("Двутавр сварной 600×250×10×16".into())),
        };
        let Some(Pick::New(p)) = dialog.pick(&project) else {
            panic!("no profile");
        };
        assert!(matches!(p.shape, ProfileShape::Welded { .. }));
        assert_eq!(p.id, ObjectId(project.next_id));
        let info = dialog.info(&project).unwrap();
        let rows = properties(&info);
        assert!(
            rows.iter().any(|(k, v)| *k == "Листов" && v == "3"),
            "{rows:?}"
        );
    }
}
