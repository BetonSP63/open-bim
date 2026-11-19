//! «Армирование» tab: commands, the bar settings and schedule window.
//! Bar sets are made by `interaction::rebar`.
use super::commands::Cmd;
use super::{Action, Mode, Request, Workbench};
use crate::interaction::rebar::{self as rb, RebarSettings};
use crate::interaction::trim::{TrimPicker, TrimTool};
use eframe::egui;
use open_bim_model::ObjectId;
use open_bim_model::rebar::{HookKind, Stress};

#[derive(Default)]
pub struct RebarUi {
    pub settings: RebarSettings,
    /// Bars drawn over the model («Видимость»).
    pub show: bool,
    pub open: bool,
    /// Shape type being edited. Recorded into the model by «Записать тип».
    pub shape_draft: Option<open_bim_model::concrete::BarShapeType>,
    /// Slab type being edited.
    pub slab_draft: Option<open_bim_model::concrete::SlabRebarType>,
    /// Cage type being edited.
    pub cage_draft: Option<open_bim_model::concrete::CageType>,
    /// 0–3 member face, 4 column top, 5 slab edge.
    pub starter_where: u8,
    pub starter_edge: u32,
    /// Г-образный выпуск; otherwise a straight bar.
    pub starter_elbow: bool,
}

/// Generator of a bar set in a host part.
type Generator = fn(
    &open_bim_model::Project,
    ObjectId,
    &RebarSettings,
) -> Result<open_bim_model::RebarSet, String>;

fn generator(tool: TrimTool) -> Option<Generator> {
    Some(match tool {
        TrimTool::RebarLongitudinal => rb::longitudinal,
        TrimTool::RebarStirrup => rb::stirrups,
        TrimTool::RebarMesh => rb::mesh,
        _ => return None,
    })
}

impl Workbench {
    pub(super) fn rebar_command(&mut self, cmd: Cmd) -> Option<Request> {
        let tool = match cmd {
            Cmd::RebarVisibility => {
                self.rebar.show = !self.rebar.show;
                self.message(if self.rebar.show {
                    "Арматура показана"
                } else {
                    "Арматура скрыта"
                });
                return None;
            }
            Cmd::RebarMore => {
                self.rebar.open = !self.rebar.open;
                self.message("Свойства арматуры и ведомость");
                return None;
            }
            Cmd::RebarLongitudinal => TrimTool::RebarLongitudinal,
            Cmd::RebarStirrup => TrimTool::RebarStirrup,
            Cmd::RebarFace => TrimTool::RebarMesh,
            Cmd::RebarGuide => TrimTool::RebarGuide,
            Cmd::RebarGroup => TrimTool::RebarGroup,
            _ => TrimTool::RebarSingle,
        };
        self.rebar.show = true;
        // Bars in the selected concrete parts at once.
        if let Some(make) = generator(tool) {
            let project = self.editor.document.project();
            let hosts: Vec<ObjectId> = self
                .editor
                .selected
                .iter()
                .copied()
                .filter(|id| {
                    project
                        .elements
                        .get(id)
                        .is_some_and(|e| crate::interaction::concrete::part_type(e).is_some())
                })
                .collect();
            if !hosts.is_empty() {
                let mut staged = project.clone();
                let mut all = Vec::new();
                for host in hosts {
                    let r = make(&staged, host, &self.rebar.settings)
                        .and_then(|set| rb::add(&staged, set));
                    match r {
                        Ok(commands) => {
                            if let Err(e) = open_bim_commands::apply_staged(&mut staged, &commands)
                            {
                                self.message(e.to_string());
                                return None;
                            }
                            all.extend(commands);
                        }
                        Err(e) => {
                            self.message(e);
                            return None;
                        }
                    }
                }
                // One SetConcrete with the final state is enough.
                let last = all.pop()?;
                return Some(Request::Action(Action::Commit(
                    super::commands::caption(cmd),
                    vec![last],
                )));
            }
        }
        self.cancel_point_input();
        self.mode = Mode::Model;
        self.trim = TrimPicker::start(tool);
        self.message(self.prompt());
        None
    }

    /// A finished pick sequence of a rebar tool.
    pub(super) fn finish_rebar(
        &self,
        tool: TrimTool,
        parts: &[ObjectId],
        points: &[open_bim_renderer::DVec3],
    ) -> Option<Result<Vec<open_bim_commands::Command>, String>> {
        let project = self.editor.document.project();
        let s = &self.rebar.settings;
        let set = match (tool, parts, points) {
            (t, [host], []) if generator(t).is_some() => generator(t)?(project, *host, s),
            (TrimTool::RebarSingle, [], [a, b]) => rb::single(*a, *b, s),
            (TrimTool::RebarGroup, [], [a, b, c]) => rb::group(*a, *b, *c, s),
            (TrimTool::RebarGuide, [], points) => rb::guide(points, s),
            _ => return None,
        };
        Some(set.and_then(|set| rb::add(project, set)))
    }

    pub(super) fn rebar_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.rebar.open {
            return;
        }
        let mut open = true;
        let mut export = false;
        let mut export_bvbs = false;
        {
            let selected: Vec<ObjectId> = self.editor.selected.iter().copied().collect();
            let project = self.editor.document.project();
            let rebar = &mut self.rebar;
            egui::Window::new("Армирование")
            .id(egui::Id::new("rebar"))
            .open(&mut open)
            .default_width(920.)
            .show(ctx, |ui| {
                let s = &mut rebar.settings;
                ui.strong("Свойства следующих стержней");
                egui::Grid::new("rebar settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Диаметр, мм");
                        egui::ComboBox::from_id_salt("rebar d")
                            .selected_text(open_bim_reports::fmt(s.diameter))
                            .show_ui(ui, |ui| {
                                for d in rb::DIAMETERS {
                                    ui.selectable_value(
                                        &mut s.diameter,
                                        d,
                                        open_bim_reports::fmt(d),
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label("Класс");
                        egui::ComboBox::from_id_salt("rebar grade")
                            .selected_text(s.grade.as_str())
                            .show_ui(ui, |ui| {
                                for g in rb::GRADES {
                                    ui.selectable_value(&mut s.grade, g.into(), g);
                                }
                            });
                        ui.end_row();
                        ui.label("Защитный слой, мм");
                        ui.add(egui::DragValue::new(&mut s.cover).range(0.0..=500.));
                        ui.end_row();
                        ui.label("Шаг, мм");
                        ui.add(egui::DragValue::new(&mut s.spacing).range(10.0..=10_000.));
                        ui.end_row();
                        ui.label("Продольных на грани");
                        ui.add(egui::DragValue::new(&mut s.per_face).range(2..=50));
                        ui.end_row();
                        for (label, id, hook) in [
                            ("Концы стержней", "rebar hook", &mut s.hook),
                            ("Концы хомутов", "rebar stirrup hook", &mut s.stirrup_hook),
                        ] {
                            ui.label(label);
                            egui::ComboBox::from_id_salt(id)
                                .selected_text(hook.label())
                                .show_ui(ui, |ui| {
                                    for k in HookKind::ALL {
                                        ui.selectable_value(hook, k, k.label());
                                    }
                                });
                            ui.end_row();
                        }
                        let min = s.min_bend();
                        ui.label("Диаметр оправки, мм");
                        ui.horizontal(|ui| {
                            let mut own = s.bend_diameter.is_some();
                            ui.checkbox(&mut own, "задать")
                                .on_hover_text("Без флажка: наименьший по СП 63 для класса и диаметра");
                            match (own, s.bend_diameter) {
                                (true, None) => s.bend_diameter = Some(min),
                                (false, Some(_)) => s.bend_diameter = None,
                                _ => {}
                            }
                            if let Some(d) = &mut s.bend_diameter {
                                ui.add(egui::DragValue::new(d).range(s.diameter..=10_000.));
                            } else {
                                ui.label(open_bim_reports::fmt(min));
                            }
                        });
                        ui.end_row();
                        ui.label("Длина поставки, мм");
                        ui.horizontal(|ui| {
                            let mut lap = s.stock_length.is_some();
                            ui.checkbox(&mut lap, "стыковать внахлёст")
                                .on_hover_text("Более длинные стержни делятся с нахлёстом по СП 63");
                            match (lap, s.stock_length) {
                                (true, None) => s.stock_length = Some(11_700.),
                                (false, Some(_)) => s.stock_length = None,
                                _ => {}
                            }
                            if let Some(l) = &mut s.stock_length {
                                ui.add(egui::DragValue::new(l).range(500.0..=100_000.));
                            }
                        });
                        ui.end_row();
                    });
                // Anchorage in the concrete of the selected part (В25 when
                // none is selected or its class is unknown).
                let host = self
                    .editor
                    .selected
                    .iter()
                    .find(|id| project.elements.contains_key(id))
                    .copied();
                let rbt = host.map_or(1.05, |h| open_bim_model::rebar::host_rbt(project, h));
                let up = |v: f64| (v / 10.).ceil() * 10.;
                if let (Some(t), Some(c)) = (
                    s.anchorage(rbt, Stress::Tension),
                    s.anchorage(rbt, Stress::Compression),
                ) {
                    ui.label(format!(
                        "По СП 63 при Rbt = {rbt} МПа: анкеровка {} мм (сжатие {} мм), нахлёст {} мм (сжатие {} мм)",
                        up(t.anchorage),
                        up(c.anchorage),
                        up(t.lap),
                        up(c.lap)
                    ))
                    .on_hover_text(
                        "l0,an = Rs·ds/(4·η1·η2·Rbt); lan ≥ 0,3·l0,an, 15ds, 200 мм; \
                         ll = 1,2·l0,an ≥ 20ds, 250 мм (не более 50% стыков в сечении)",
                    );
                }
                ui.separator();
                let details = open_bim_model::rebar::detail_schedule(project);
                let steel = open_bim_model::rebar::steel_schedule(project);
                ui.strong(format!("Наборы: {}", project.concrete.rebars.len()));
                let mut remove = None;
                egui::ScrollArea::vertical()
                    .id_salt("rebar sets")
                    .max_height(160.)
                    .show(ui, |ui| {
                        egui::Grid::new("rebar sets grid")
                            .num_columns(7)
                            .striped(true)
                            .show(ui, |ui| {
                                for h in ["№", "Поз.", "Вид", "Деталь", "Ø, класс", "Стержней", ""]
                                {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for r in &project.concrete.rebars {
                                    ui.label(r.number.to_string());
                                    ui.label(position_text(&details, r.number));
                                    ui.label(r.kind.label());
                                    ui.label(r.host.map_or("—".into(), |h| {
                                        project
                                            .elements
                                            .get(&h)
                                            .map_or(format!("#{} (удалена)", h.0), |e| {
                                                format!("{} #{}", e.name, h.0)
                                            })
                                    }));
                                    ui.label(format!(
                                        "Ø{} {}",
                                        open_bim_reports::fmt(r.diameter),
                                        r.grade
                                    ));
                                    ui.label(r.bars.len().to_string());
                                    if ui.small_button("Удалить").clicked() {
                                        remove = Some(r.number);
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(n) = remove
                    && let Ok(commands) = rb::remove(project, n)
                {
                    requests.push(Request::Action(Action::Commit(
                        "Удалить арматуру".into(),
                        commands,
                    )));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Экспорт CSV ведомостей")
                        .on_hover_text("Папка release: ведомость деталей и ведомость расхода стали")
                        .clicked()
                    {
                        export = true;
                    }
                    if ui
                        .button("Экспорт BVBS")
                        .on_hover_text(
                            "Папка release: арматура.bvbs, одна строка на позицию \
                             (BVBS-Richtlinie 3.1, BF2D)",
                        )
                        .clicked()
                    {
                        export_bvbs = true;
                    }
                });
                schedule_tables(ui, &details, &steel);
                let _ = s;
                kj_panel(ui, rebar, project, &selected, requests);
            });
        }
        self.rebar.open = open;
        if export {
            let project = self.editor.document.project();
            match open_bim_reports::csv::write_rebar(project, std::path::Path::new("release")) {
                Ok(files) if files.is_empty() => self.message("В модели нет арматуры"),
                Ok(files) => self.message(format!(
                    "Ведомости арматуры: {}",
                    files
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                Err(e) => self.message(e),
            }
        }
        if export_bvbs {
            let project = self.editor.document.project();
            match open_bim_reports::csv::write_bvbs(project, std::path::Path::new("release")) {
                Ok(files) if files.is_empty() => self.message("В модели нет арматуры"),
                Ok(files) => self.message(format!(
                    "BVBS: {}",
                    files
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                Err(e) => self.message(e),
            }
        }
    }
}

fn position_text(rows: &[open_bim_model::rebar::DetailRow], set: u32) -> String {
    let mut marks: Vec<u32> = rows
        .iter()
        .filter(|r| r.sets.contains(&set))
        .map(|r| r.mark)
        .collect();
    marks.sort_unstable();
    marks.dedup();
    if marks.is_empty() {
        "—".into()
    } else {
        marks
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn schedule_tables(
    ui: &mut egui::Ui,
    details: &[open_bim_model::rebar::DetailRow],
    steel: &open_bim_model::rebar::SteelSchedule,
) {
    use open_bim_model::rebar::column_letters;
    egui::CollapsingHeader::new("Ведомость деталей")
        .id_salt("rebar detail schedule")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                "Позиция — одинаковые стержни (форма, диаметр, класс по ГОСТ 34028, \
                 длины участков, крюки). Эскиз строится по осевой линии формы.",
            );
            if details.is_empty() {
                ui.label("Стержней нет");
                return;
            }
            let letters = column_letters(details);
            egui::ScrollArea::horizontal()
                .id_salt("rebar detail scroll")
                .show(ui, |ui| {
                    egui::Grid::new("rebar detail grid")
                        .striped(true)
                        .num_columns(9 + letters.len())
                        .show(ui, |ui| {
                            for h in ["Поз.", "Эскиз", "Форма", "Ø", "Класс"] {
                                ui.strong(h);
                            }
                            for letter in &letters {
                                ui.strong(letter);
                            }
                            for h in ["Длина, мм", "Кол.", "Масса ед., кг", "Масса, кг"]
                            {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for row in details {
                                ui.label(row.mark.to_string());
                                paint_sketch(ui, &row.sketch);
                                ui.label(&row.code).on_hover_text(&row.standard);
                                ui.label(open_bim_reports::fmt(row.diameter));
                                ui.label(&row.grade).on_hover_text(&row.standard);
                                for letter in &letters {
                                    let lens = row.lengths_of(letter);
                                    ui.label(if lens.is_empty() {
                                        String::new()
                                    } else {
                                        lens.iter()
                                            .map(|v| open_bim_reports::fmt(*v))
                                            .collect::<Vec<_>>()
                                            .join(" / ")
                                    });
                                }
                                ui.label(open_bim_reports::fmt(row.length_mm));
                                ui.label(row.count.to_string());
                                ui.label(format!("{:.3}", row.unit_mass_kg));
                                ui.label(format!("{:.3}", row.total_mass_kg));
                                ui.end_row();
                            }
                            ui.strong("Итого");
                            for _ in 0..5 + letters.len() {
                                ui.label("");
                            }
                            ui.strong(details.iter().map(|r| r.count).sum::<usize>().to_string());
                            ui.label("");
                            ui.strong(format!(
                                "{:.3}",
                                details.iter().map(|r| r.total_mass_kg).sum::<f64>()
                            ));
                            ui.end_row();
                        });
                });
        });
    egui::CollapsingHeader::new("Ведомость расхода стали")
        .id_salt("rebar steel schedule")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("По элементу, классу стали и диаметру. Итог — по классу и диаметру.");
            if steel.by_class.is_empty() {
                ui.label("Стержней нет");
                return;
            }
            if steel.by_element.iter().any(|r| r.element != "—") {
                steel_grid(ui, "rebar steel elements", &steel.by_element, true);
                ui.add_space(6.);
                ui.strong("Всего по классу и диаметру");
            }
            steel_grid(ui, "rebar steel class", &steel.by_class, false);
            ui.strong(format!(
                "Итого: {} стержней, {:.3} м, {:.3} кг",
                steel.total_bars, steel.total_length_m, steel.total_kg
            ));
        });
}

fn steel_grid(
    ui: &mut egui::Ui,
    id: &str,
    rows: &[open_bim_model::rebar::SteelRow],
    element: bool,
) {
    egui::Grid::new(id)
        .striped(true)
        .num_columns(if element { 6 } else { 5 })
        .show(ui, |ui| {
            if element {
                ui.strong("Элемент");
            }
            for h in ["Класс", "Ø, мм", "Стержней", "Длина, м", "Масса, кг"]
            {
                ui.strong(h);
            }
            ui.end_row();
            for r in rows {
                if element {
                    ui.label(&r.element);
                }
                ui.label(&r.grade);
                ui.label(open_bim_reports::fmt(r.diameter));
                ui.label(r.bars.to_string());
                ui.label(format!("{:.3}", r.length_m));
                ui.label(format!("{:.3}", r.mass_kg));
                ui.end_row();
            }
        });
}

fn paint_sketch(ui: &mut egui::Ui, sketch: &open_bim_model::rebar::ShapeSketch) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(168., 96.), egui::Sense::hover());
    let visuals = ui.visuals().clone();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2., visuals.extreme_bg_color);
    painter.rect_stroke(
        rect,
        2.,
        egui::Stroke::new(1., visuals.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    if sketch.points.len() < 2 {
        return;
    }
    let [x0, y0, x1, y1] = sketch.bounds;
    let (w, h) = ((x1 - x0).max(1.), (y1 - y0).max(1.));
    let pad = 8.;
    let scale = ((rect.width() - 2. * pad) / w as f32).min((rect.height() - 2. * pad) / h as f32);
    let map = |p: [f64; 2]| {
        egui::pos2(
            rect.left() + pad + (p[0] - x0) as f32 * scale,
            rect.bottom() - pad - (p[1] - y0) as f32 * scale,
        )
    };
    let stroke = egui::Stroke::new(1.6, visuals.strong_text_color());
    for pair in sketch.points.windows(2) {
        painter.line_segment([map(pair[0]), map(pair[1])], stroke);
    }
    let font = egui::FontId::proportional(10.);
    for mark in &sketch.marks {
        painter.text(
            map(mark.at),
            egui::Align2::CENTER_CENTER,
            &mark.text,
            font.clone(),
            visuals.text_color(),
        );
    }
}

fn cage_panel(
    ui: &mut egui::Ui,
    rebar: &mut RebarUi,
    project: &open_bim_model::Project,
    selected: &[ObjectId],
    requests: &mut Vec<Request>,
) {
    use open_bim_model::concrete::CageType;
    egui::CollapsingHeader::new("Каркасы балок и колонн")
        .id_salt("kj cage")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Тип задаёт продольные стержни и хомуты. Защитный слой берётся с балки или колонны. \
                 Хомут хранит ссылки на продольные стержни и повторяет их размер.",
            );
            if rebar.cage_draft.is_none() && ui.button("Новый тип каркаса").clicked() {
                rebar.cage_draft = Some(CageType::new(
                    format!("cage-{}", project.concrete.cage_types.len() + 1),
                    "Каркас",
                ));
            }
            let mut edit = None;
            for ty in &project.concrete.cage_types {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{}{}",
                        ty.name,
                        if ty.default_for_new {
                            " — для новых балок и колонн"
                        } else {
                            ""
                        }
                    ));
                    if ui.small_button("Изменить").clicked() {
                        edit = Some(ty.clone());
                    }
                });
            }
            if let Some(ty) = edit {
                rebar.cage_draft = Some(ty);
            }
            if let Some(draft) = &mut rebar.cage_draft {
                ui.horizontal(|ui| {
                    ui.label("Имя");
                    ui.text_edit_singleline(&mut draft.name);
                });
                ui.checkbox(&mut draft.default_for_new, "Для новых балок и колонн");
                ui.horizontal(|ui| {
                    ui.label("Продольные Ø");
                    ui.add(egui::DragValue::new(&mut draft.long_diameter).range(4.0..=40.));
                    ui.label("на грани");
                    ui.add(egui::DragValue::new(&mut draft.per_face).range(2..=50));
                });
                ui.horizontal(|ui| {
                    ui.label("Хомут Ø");
                    ui.add(egui::DragValue::new(&mut draft.stirrup_diameter).range(4.0..=40.));
                    ui.label("шаг");
                    ui.add(
                        egui::DragValue::new(&mut draft.stirrup_spacing)
                            .range(draft.stirrup_diameter..=10_000.),
                    );
                });
                let mut dense = draft.end_spacing.is_some();
                ui.checkbox(&mut dense, "Учащение у торцов");
                if dense {
                    let min_step = draft.stirrup_diameter;
                    let mut end_spacing = draft.end_spacing.unwrap_or(100.);
                    let mut end_length = draft.end_length.unwrap_or(400.);
                    ui.horizontal(|ui| {
                        ui.label("шаг у торца");
                        ui.add(egui::DragValue::new(&mut end_spacing).range(min_step..=10_000.));
                        ui.label("длина зоны");
                        ui.add(egui::DragValue::new(&mut end_length).range(1.0..=10_000.));
                    });
                    draft.end_spacing = Some(end_spacing);
                    draft.end_length = Some(end_length);
                } else {
                    draft.end_spacing = None;
                    draft.end_length = None;
                }
                let shapes = stirrup_shapes(project);
                let label = shapes
                    .iter()
                    .find(|(id, _)| id == &draft.stirrup_shape)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| draft.stirrup_shape.clone());
                ui.horizontal(|ui| {
                    ui.label("Форма хомута");
                    egui::ComboBox::from_id_salt("kj stirrup shape")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (id, name) in &shapes {
                                ui.selectable_value(&mut draft.stirrup_shape, id.clone(), name);
                            }
                        });
                });
                if ui.button("Записать тип каркаса").clicked()
                    && let Ok(commands) = rb::save_cage_type(project, draft.clone())
                {
                    requests.push(Request::Action(Action::Commit(
                        "Тип каркаса".into(),
                        commands,
                    )));
                }
            }
            if ui.button("Каркас в выбранных балках и колоннах").clicked() {
                let id = rebar
                    .cage_draft
                    .as_ref()
                    .map(|t| t.id.clone())
                    .or_else(|| {
                        project
                            .concrete
                            .cage_types
                            .iter()
                            .find(|t| t.default_for_new)
                            .map(|t| t.id.clone())
                    })
                    .or_else(|| project.concrete.cage_types.first().map(|t| t.id.clone()));
                if let Some(id) = id
                    && let Ok(commands) = rb::place_cage(project, selected, &id)
                {
                    requests.push(Request::Action(Action::Commit("Каркас".into(), commands)));
                }
            }
        });
}

fn stirrup_shapes(project: &open_bim_model::Project) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = open_bim_model::rebar::builtin_shapes()
        .into_iter()
        .filter(|s| s.id == "gost.stirrup" || s.id == "gost.straight")
        .map(|s| (s.id, format!("{} ({})", s.name, s.code)))
        .collect();
    for s in &project.concrete.shape_types {
        if let Some(row) = rows.iter_mut().find(|n| n.0 == s.id) {
            row.1 = format!("{} — изменён", s.name);
        } else {
            rows.push((s.id.clone(), s.name.clone()));
        }
    }
    rows
}

fn starter_panel(
    ui: &mut egui::Ui,
    rebar: &mut RebarUi,
    project: &open_bim_model::Project,
    selected: &[ObjectId],
    requests: &mut Vec<Request>,
) {
    use open_bim_model::concrete::StarterPath;
    egui::CollapsingHeader::new("Выпуски")
        .id_salt("kj starter")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Прямые или Г-образные стержни вдоль грани или с верха колонны. \
                 Длина участка — нахлёстка ll по СП 63. Шаг, диаметр и класс — из свойств выше.",
            );
            ui.horizontal(|ui| {
                ui.label("Форма");
                ui.selectable_value(&mut rebar.starter_elbow, false, "Прямой");
                ui.selectable_value(&mut rebar.starter_elbow, true, "Г-образный");
            });
            const WHERE: [(&str, u8); 6] = [
                ("Нижняя грань", 0),
                ("Правая грань", 1),
                ("Верхняя грань", 2),
                ("Левая грань", 3),
                ("С верха колонны", 4),
                ("Кромка плиты", 5),
            ];
            let label = WHERE
                .iter()
                .find(|(_, n)| *n == rebar.starter_where)
                .map(|(text, _)| *text)
                .unwrap_or("Верхняя грань");
            ui.horizontal(|ui| {
                ui.label("Путь");
                egui::ComboBox::from_id_salt("kj starter path")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (text, n) in WHERE {
                            ui.selectable_value(&mut rebar.starter_where, n, text);
                        }
                    });
            });
            if rebar.starter_where == 5 {
                ui.horizontal(|ui| {
                    ui.label("Номер кромки");
                    ui.add(egui::DragValue::new(&mut rebar.starter_edge).range(0..=100));
                });
            }
            if ui.button("Поставить выпуски").clicked() {
                let path = match rebar.starter_where {
                    4 => StarterPath::ColumnTop,
                    5 => StarterPath::SlabEdge {
                        index: rebar.starter_edge,
                    },
                    n => StarterPath::Face {
                        index: u32::from(n),
                    },
                };
                let shape = if rebar.starter_elbow {
                    "gost.l"
                } else {
                    "gost.straight"
                };
                if let Ok(commands) =
                    rb::place_starters(project, selected, &path, shape, &rebar.settings)
                {
                    requests.push(Request::Action(Action::Commit("Выпуски".into(), commands)));
                }
            }
        });
}

/// «КЖ»: shape types, the slab type and the model check. Kept in this window
/// so it does not touch the property pane.
fn kj_panel(
    ui: &mut egui::Ui,
    rebar: &mut RebarUi,
    project: &open_bim_model::Project,
    selected: &[ObjectId],
    requests: &mut Vec<Request>,
) {
    use open_bim_model::concrete::{
        BarAlign, BarShapeType, MeshLayer, ShapeSegment, SlabRebarType,
    };
    use open_bim_model::rebar::{self as shapes, HookKind};
    ui.separator();
    egui::CollapsingHeader::new("Типы форм")
        .id_salt("kj shapes")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                "Осевая линия, параметры A, B, C…, крюки и диаметр оправки. \
                 Стержни и наборы ссылаются на тип и перестраиваются вместе с ним.",
            );
            if ui.button("Новый тип").clicked() {
                rebar.shape_draft = Some(BarShapeType {
                    id: format!("form-{}", project.concrete.shape_types.len() + 1),
                    name: "Своя форма".into(),
                    code: "С".into(),
                    segments: vec![ShapeSegment {
                        letter: "A".into(),
                        length: 1000.,
                        bend_deg: 0.,
                    }],
                    hooks: [open_bim_model::Hook::default(); 2],
                    mandrel: None,
                });
            }
            let mut rows: Vec<(String, String)> = shapes::builtin_shapes()
                .into_iter()
                .map(|s| (s.id.clone(), format!("{} ({})", s.name, s.code)))
                .collect();
            for s in &project.concrete.shape_types {
                if let Some(row) = rows.iter_mut().find(|n| n.0 == s.id) {
                    row.1 = format!("{} — изменён", s.name);
                } else {
                    rows.push((s.id.clone(), s.name.clone()));
                }
            }
            let mut edit = None;
            for (id, label) in &rows {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.small_button("Изменить").clicked() {
                        edit = Some(id.clone());
                    }
                });
            }
            if let Some(id) = edit {
                rebar.shape_draft = shapes::resolve_shape(project, &id);
            }
            let Some(draft) = &mut rebar.shape_draft else {
                return;
            };
            ui.separator();
            ui.label("Редактор типа формы");
            ui.horizontal(|ui| {
                ui.label("Имя");
                ui.text_edit_singleline(&mut draft.name);
            });
            ui.horizontal(|ui| {
                ui.label("Обозначение");
                ui.text_edit_singleline(&mut draft.code);
            });
            let mut mandrel = draft.mandrel.unwrap_or(0.);
            ui.horizontal(|ui| {
                ui.label("Диаметр оправки, мм");
                ui.add(egui::DragValue::new(&mut mandrel).range(0.0..=10_000.));
                ui.label("0 — наименьший по СП 63");
            });
            draft.mandrel = (mandrel >= 1.).then_some(mandrel);
            for (i, label) in ["Крюк начала", "Крюк конца"].into_iter().enumerate()
            {
                ui.horizontal(|ui| {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(label)
                        .selected_text(draft.hooks[i].kind.label())
                        .show_ui(ui, |ui| {
                            for k in HookKind::ALL {
                                ui.selectable_value(&mut draft.hooks[i].kind, k, k.label());
                            }
                        });
                });
            }
            let mut remove = None;
            for (i, seg) in draft.segments.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label("Участок");
                    ui.add(egui::TextEdit::singleline(&mut seg.letter).desired_width(28.));
                    ui.label("длина, мм");
                    ui.add(egui::DragValue::new(&mut seg.length).range(1.0..=100_000.));
                    ui.label("загиб, °");
                    ui.add(egui::DragValue::new(&mut seg.bend_deg).range(-180.0..=180.));
                    if ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove
                && draft.segments.len() > 1
            {
                draft.segments.remove(i);
            }
            if ui.button("Добавить участок").clicked() {
                let n = draft.segments.len();
                let letter = (b'A' + (n as u8)).min(b'Z') as char;
                draft.segments.push(ShapeSegment {
                    letter: letter.to_string(),
                    length: 200.,
                    bend_deg: 0.,
                });
                if n > 0 {
                    draft.segments[n - 1].bend_deg = 90.;
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Записать тип").clicked()
                    && let Ok(commands) = rb::save_shape(project, draft.clone())
                {
                    requests.push(Request::Action(Action::Commit(
                        "Тип формы".into(),
                        commands,
                    )));
                }
                if ui.button("Стержень в центр плиты").clicked()
                    && let Ok(commands) = rb::place_shape(project, selected, draft.clone())
                {
                    requests.push(Request::Action(Action::Commit(
                        "Стержень по типу формы".into(),
                        commands,
                    )));
                }
            });
        });

    egui::CollapsingHeader::new("Раскладка в плите")
        .id_salt("kj slab")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                "Тип плиты задаёт верхнюю и нижнюю сетки. Защитный слой хранится на плите. \
                 П-стержни и лягушки ставятся, когда есть обе сетки.",
            );
            if rebar.slab_draft.is_none() && ui.button("Новый тип плиты").clicked() {
                let mut ty = SlabRebarType::new(
                    format!("slab-{}", project.concrete.slab_types.len() + 1),
                    "Перекрытие, сетки",
                );
                ty.bottom = Some(MeshLayer::new(12., 200., 0.));
                ty.top = Some(MeshLayer::new(12., 200., 0.));
                rebar.slab_draft = Some(ty);
            }
            let mut edit = None;
            for ty in &project.concrete.slab_types {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{}{}",
                        ty.name,
                        if ty.default_for_new {
                            " — для новых плит"
                        } else {
                            ""
                        }
                    ));
                    if ui.small_button("Изменить").clicked() {
                        edit = Some(ty.clone());
                    }
                });
            }
            if let Some(ty) = edit {
                rebar.slab_draft = Some(ty);
            }
            if let Some(draft) = &mut rebar.slab_draft {
                ui.horizontal(|ui| {
                    ui.label("Имя");
                    ui.text_edit_singleline(&mut draft.name);
                });
                ui.checkbox(
                    &mut draft.default_for_new,
                    "Для новых плит и фундаментных плит",
                );
                ui.checkbox(&mut draft.cut_openings, "Резать стержни в проёмах");
                ui.horizontal(|ui| {
                    ui.label("Привязка шага");
                    egui::ComboBox::from_id_salt("kj align")
                        .selected_text(if draft.align == BarAlign::Edge {
                            "От края"
                        } else {
                            "От оси"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.align, BarAlign::Edge, "От края");
                            ui.selectable_value(&mut draft.align, BarAlign::Axis, "От оси");
                        });
                });
                for (title, layer) in [
                    ("Нижняя сетка", &mut draft.bottom),
                    ("Верхняя сетка", &mut draft.top),
                ] {
                    let mut on = layer.is_some();
                    ui.checkbox(&mut on, title);
                    if on && layer.is_none() {
                        *layer = Some(MeshLayer::new(12., 200., 0.));
                    }
                    if !on {
                        *layer = None;
                    }
                    if let Some(layer) = layer {
                        ui.horizontal(|ui| {
                            ui.label("Ø");
                            ui.add(egui::DragValue::new(&mut layer.diameter).range(4.0..=40.));
                            ui.label("шаг");
                            ui.add(
                                egui::DragValue::new(&mut layer.spacing)
                                    .range(layer.diameter..=10_000.),
                            );
                            ui.label("направление °");
                            ui.add(
                                egui::DragValue::new(&mut layer.direction_deg).range(-180.0..=180.),
                            );
                        });
                        let mut cross = layer.cross_spacing.unwrap_or(0.);
                        ui.horizontal(|ui| {
                            ui.label("Шаг поперёк, 0 — одно направление");
                            ui.add(egui::DragValue::new(&mut cross).range(0.0..=10_000.));
                        });
                        layer.cross_spacing = (cross >= layer.diameter).then_some(cross);
                    }
                }
                ui.checkbox(&mut draft.edge_bars, "П-образные по контуру и проёмам");
                ui.horizontal(|ui| {
                    ui.label("Ø П");
                    ui.add(egui::DragValue::new(&mut draft.edge_diameter).range(4.0..=40.));
                    ui.label("шаг");
                    ui.add(
                        egui::DragValue::new(&mut draft.edge_spacing)
                            .range(draft.edge_diameter..=10_000.),
                    );
                });
                ui.checkbox(&mut draft.frogs, "Лягушки между сетками");
                ui.horizontal(|ui| {
                    ui.label("Ø лягушки");
                    ui.add(egui::DragValue::new(&mut draft.frog_diameter).range(4.0..=40.));
                    ui.label("шаг в плане");
                    ui.add(
                        egui::DragValue::new(&mut draft.frog_spacing)
                            .range(draft.frog_diameter..=10_000.),
                    );
                });
                if ui.button("Записать тип плиты").clicked()
                    && let Ok(commands) = rb::save_slab_type(project, draft.clone())
                {
                    requests.push(Request::Action(Action::Commit(
                        "Тип плиты".into(),
                        commands,
                    )));
                }
            }
            if ui.button("Разложить в выбранных плитах").clicked() {
                let id = rebar
                    .slab_draft
                    .as_ref()
                    .map(|t| t.id.clone())
                    .or_else(|| {
                        project
                            .concrete
                            .slab_types
                            .iter()
                            .find(|t| t.default_for_new)
                            .map(|t| t.id.clone())
                    })
                    .or_else(|| project.concrete.slab_types.first().map(|t| t.id.clone()));
                if let Some(id) = id
                    && let Ok(commands) = rb::reinforce(project, selected, &id)
                {
                    requests.push(Request::Action(Action::Commit(
                        "Раскладка".into(),
                        commands,
                    )));
                }
            }
            if let Some(zone) = project
                .concrete
                .zones
                .iter()
                .find(|z| selected.contains(&z.host))
            {
                ui.label(format!("Марка зоны: {}", zone.mark));
                let mut off = zone.edge_off.clone();
                let n = project
                    .elements
                    .get(&zone.host)
                    .and_then(|e| match &e.shape {
                        open_bim_model::ElementShape::Plate { contour, .. } => Some(contour.len()),
                        _ => None,
                    })
                    .unwrap_or(0);
                let mut changed = false;
                for i in 0..n {
                    let mut on = !off.contains(&(i as u32));
                    if ui
                        .checkbox(&mut on, format!("Обрамление кромки {}", i + 1))
                        .changed()
                    {
                        changed = true;
                        if on {
                            off.retain(|e| *e != i as u32);
                        } else {
                            off.push(i as u32);
                        }
                    }
                }
                if changed && let Ok(commands) = rb::frame_edges(project, zone.number, off) {
                    requests.push(Request::Action(Action::Commit(
                        "Обрамление кромки".into(),
                        commands,
                    )));
                }
            }
        });

    cage_panel(ui, rebar, project, selected, requests);
    starter_panel(ui, rebar, project, selected, requests);

    let notes = shapes::model_checks(project);
    if !notes.is_empty() {
        ui.separator();
        ui.strong("Проверка армирования");
        for note in notes {
            ui.label(note);
        }
    }
}
