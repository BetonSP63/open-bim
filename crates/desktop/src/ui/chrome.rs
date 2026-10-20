//! Tekla-style window chrome: title row with quick access and quick launch,
//! side pane strip with panels (the ribbon is in `ribbon.rs`), two bottom
//! toolbars (work plane, model search, snap and selection switches) and the
//! blue status bar with the current prompt.
use super::icons::{self, Icon};
use super::{Cmd, FocusArea, Mode, Request, Workbench, commands, theme};
use crate::interaction::snap::SnapKind;
use crate::tools::ui::{Action, ToolsPanel, select};
use eframe::egui;
use open_bim_model::ObjectId;
use std::collections::BTreeSet;
use std::sync::Arc;

pub use super::ribbon::{Item, RibbonTab, UNAVAILABLE, groups};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanel {
    Properties,
    Edit,
    Detailing,
    Connections,
    Clash,
    Tree,
    Views,
}

impl SidePanel {
    pub fn title(self) -> &'static str {
        match self {
            Self::Properties => "Свойства",
            Self::Edit => "Правка выбранных",
            Self::Detailing => "Болты, отверстия и швы",
            Self::Connections => "Узлы",
            Self::Clash => "Нумерация и коллизии",
            Self::Tree => "Дерево модели",
            Self::Views => "Виды модели",
        }
    }
}

pub(super) fn tooltip(cmd: Cmd) -> String {
    let shortcut = commands::shortcut_text(cmd);
    let label = commands::spec(cmd).label;
    if shortcut.is_empty() {
        label.to_string()
    } else {
        format!("{label} · {shortcut}")
    }
}

impl Workbench {
    pub(super) fn run(&mut self, cmd: Cmd, requests: &mut Vec<Request>) {
        if let Some(r) = self.execute(cmd) {
            requests.push(r);
        }
    }

    /// Whether viewport clicks go to a pick command.
    pub fn picking(&self) -> bool {
        self.engine.active()
            || self.workplane_picks.is_some()
            || self.detail.picks_points()
            || self.edit_pick.op.is_some()
            || self.measure.is_some()
            || matches!(
                self.trim.need(),
                Some(
                    crate::interaction::trim::Need::Point | crate::interaction::trim::Need::Polygon
                )
            )
    }

    pub fn prompt(&self) -> String {
        if self.workplane_picks.is_some() {
            self.workplane_prompt()
        } else if let Some(p) = self.trim.prompt() {
            p
        } else if let Some(p) = self.detail.prompt() {
            p
        } else if let Some(p) = self.edit_pick.prompt() {
            p
        } else if let Some(m) = &self.measure {
            if m.is_empty() {
                "Укажите первую точку измерения".into()
            } else {
                "Укажите вторую точку измерения".into()
            }
        } else {
            self.engine.prompt()
        }
    }

    /// Viewport click selection honouring the selection switches: the part,
    /// its whole assembly, or nothing; `mode` is replace / Shift add / Ctrl toggle.
    pub fn select_picked(
        &mut self,
        id: Option<ObjectId>,
        mode: crate::interaction::selection::Click,
    ) {
        use crate::interaction::selection::{Click, apply, expand_assemblies};
        let project = self.editor.document.project();
        let mut ids: BTreeSet<ObjectId> = id
            .filter(|id| project.elements.contains_key(id))
            .filter(|_| self.select.parts || self.select.assemblies)
            .into_iter()
            .collect();
        if self.select.assemblies {
            expand_assemblies(project, &mut ids);
        }
        if ids.is_empty() && mode != Click::Replace {
            return;
        }
        apply(&mut self.editor.selected, &ids, mode);
    }

    /// Give keyboard focus to a field when its area was chosen by F6/Ctrl+F.
    fn take_focus(&mut self, ui: &egui::Ui, area: FocusArea, id: egui::Id) {
        if self.focus_target == Some(area) {
            self.focus_target = None;
            ui.memory_mut(|m| m.request_focus(id));
        }
    }

    /// Alt+arrow: move the keyboard cursor to the next snap point (while a
    /// command picks points) or the next part (otherwise) in that direction.
    pub fn keyboard_step(
        &mut self,
        p: &open_bim_renderer::overlay::Projector,
        arrow: crate::interaction::keyboard_cursor::Arrow,
    ) {
        use crate::interaction::keyboard_cursor::{object_points, snap_points, step};
        let project = self.editor.document.project();
        if self.picking() {
            let points = snap_points(project);
            let from = self
                .kb_cursor
                .or(self.engine.picks.last().copied())
                .unwrap_or(self.engine.workplane.origin);
            match step(p, &points, from, arrow) {
                Some(i) => {
                    let q = points[i];
                    self.kb_cursor = Some(q);
                    self.message(format!(
                        "Курсор: {:.0}, {:.0}, {:.0} — Пробел указывает точку",
                        q.x, q.y, q.z
                    ));
                }
                None => self.message("В этом направлении нет точек привязки"),
            }
        } else {
            let objects = object_points(project);
            let from = self.kb_cursor.unwrap_or_default();
            let points: Vec<_> = objects.iter().map(|o| o.1).collect();
            // The first press lands on the first part; later ones step.
            let found = if self.kb_object.is_none() {
                (!objects.is_empty()).then_some(0)
            } else {
                step(p, &points, from, arrow)
            };
            match found {
                Some(i) => {
                    let (id, q) = objects[i];
                    self.kb_cursor = Some(q);
                    self.kb_object = Some(id);
                    let name = &project.elements[&id].name;
                    self.message(format!(
                        "Объект: {name} #{} — Пробел выбирает, Alt+Пробел добавляет/убирает",
                        id.0
                    ));
                }
                None => self.message("В этом направлении нет объектов"),
            }
        }
    }

    /// Space: pick the keyboard cursor point, or select the object under it.
    pub fn keyboard_space(
        &mut self,
        mode: crate::interaction::selection::Click,
    ) -> Option<Request> {
        if self.picking() {
            let point = self.kb_cursor?;
            return self.pick(point);
        }
        let id = self.kb_object?;
        self.select_picked(Some(id), mode);
        self.message(format!("Выбрано объектов: {}", self.editor.selected.len()));
        None
    }

    /// Plate contour preview for the overlay: the picked points plus the
    /// cursor (or the rectangle corners) and whether the cursor point would be
    /// accepted.
    pub fn plate_preview(
        &self,
        cursor: Option<open_bim_renderer::DVec3>,
    ) -> Option<(Vec<open_bim_renderer::DVec3>, bool)> {
        use crate::interaction::polygon::{self, PlateMode};
        self.engine.tool.filter(|k| k.is_plate())?;
        let wp = &self.engine.workplane;
        let local = |p: open_bim_renderer::DVec3| {
            let l = wp.to_local(p);
            [l.x, l.y]
        };
        let picks = &self.engine.picks;
        let level = picks.first().map_or(0., |p| wp.to_local(*p).z);
        let world = |c: [f64; 2]| wp.to_world(open_bim_renderer::DVec3::new(c[0], c[1], level));
        match (self.editor.draft.plate_mode, picks.as_slice(), cursor) {
            (PlateMode::Polygon, _, _) => {
                let mut points = picks.clone();
                let valid = match cursor {
                    Some(c) => {
                        let ok = polygon::check_next(
                            &picks.iter().map(|p| local(*p)).collect::<Vec<_>>(),
                            local(c),
                        )
                        .is_ok();
                        points.push(c);
                        ok
                    }
                    None => self.engine.error.is_none(),
                };
                (!points.is_empty()).then_some((points, valid))
            }
            (PlateMode::Rectangle, [a], Some(c)) => match polygon::rectangle(local(*a), local(c)) {
                Ok(r) => Some((r.into_iter().map(world).collect(), true)),
                Err(_) => Some((vec![*a, c], false)),
            },
            (PlateMode::RotatedRectangle, [a, b], Some(c)) => {
                match polygon::rotated_rectangle(local(*a), local(*b), local(c)) {
                    Ok(r) => Some((r.into_iter().map(world).collect(), true)),
                    Err(_) => Some((vec![*a, *b, c], false)),
                }
            }
            (PlateMode::RotatedRectangle, [a], Some(c)) => Some((vec![*a, c], true)),
            _ => None,
        }
    }

    /// Viewport click: the part under the cursor, otherwise the grid line
    /// near it (when the «Выбор осей» switch is on).
    pub fn select_clicked(
        &mut self,
        part: Option<ObjectId>,
        axis: Option<ObjectId>,
        mode: crate::interaction::selection::Click,
    ) {
        let project = self.editor.document.project();
        let allowed = |id: &ObjectId| {
            (self.select.grids && project.axes.contains_key(id))
                || (self.select.levels && project.levels.contains_key(id))
                || project.construction.contains_key(id)
                || project.dimensions.contains_key(id)
        };
        match (part, axis.filter(allowed)) {
            (None, Some(axis)) => {
                let name = match (project.axes.get(&axis), project.levels.get(&axis)) {
                    (Some(a), _) => format!("Выбрана ось {}", a.name),
                    (_, Some(l)) => format!("Выбран уровень {}", l.name),
                    _ if project.dimensions.contains_key(&axis) => "Выбран размер".into(),
                    _ => "Выбран вспомогательный объект (Delete — удалить)".into(),
                };
                crate::interaction::selection::apply(
                    &mut self.editor.selected,
                    &BTreeSet::from([axis]),
                    mode,
                );
                self.message(name);
            }
            _ => self.select_picked(part, mode),
        }
    }

    /// Area selection (left→right window, right→left crossing).
    pub fn select_area(
        &mut self,
        projector: &open_bim_renderer::overlay::Projector,
        from: [f64; 2],
        to: [f64; 2],
        mode: crate::interaction::selection::Click,
    ) {
        let ids = crate::interaction::selection::area(
            self.editor.document.project(),
            projector,
            &self.hidden,
            from,
            to,
            self.select,
        );
        crate::interaction::selection::apply(&mut self.editor.selected, &ids, mode);
        self.message(format!("Выбрано объектов: {}", self.editor.selected.len()));
    }

    /// Select parts whose name, profile, material or ID contains the text.
    pub fn search_model(&mut self) -> usize {
        let query = self.search.trim().to_lowercase();
        if query.is_empty() {
            return 0;
        }
        let project = self.editor.document.project();
        let found: std::collections::BTreeSet<ObjectId> = project
            .elements
            .values()
            .filter(|e| {
                let profile = match &e.shape {
                    open_bim_model::ElementShape::Member { profile, .. } => project
                        .profiles
                        .get(profile)
                        .map(|p| p.name.as_str())
                        .unwrap_or(""),
                    open_bim_model::ElementShape::Plate { .. } => "пластина",
                };
                let material = project
                    .materials
                    .get(&e.material)
                    .map(|m| m.name.as_str())
                    .unwrap_or("");
                format!("{} {} {} #{}", e.name, profile, material, e.id.0)
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|e| e.id)
            .collect();
        let n = found.len();
        self.editor.selected = found;
        self.message(format!("Найдено и выбрано: {n}"));
        n
    }

    pub(super) fn chrome(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        if self.theme_applied != Some(self.light_theme) {
            theme::set_theme(ui.ctx(), self.light_theme);
            self.theme_applied = Some(self.light_theme);
        }
        self.title_row(ui, requests);
        self.ribbon(ui, requests);
        self.status_bar(ui);
        self.selection_toolbar(ui, requests);
        self.snap_toolbar(ui, requests);
        self.side(ui, requests);
        self.contextual_toolbar(ui.ctx(), requests);
    }

    fn title_row(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::top("title").show(ui, |ui| {
            ui.horizontal(|ui| {
                let (badge, _) = ui.allocate_exact_size(egui::vec2(22., 22.), egui::Sense::hover());
                ui.painter().rect_filled(badge, 3., theme::STATUS_BG);
                ui.painter().text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    "OB",
                    egui::FontId::proportional(11.),
                    egui::Color32::WHITE,
                );
                for (cmd, icon) in [
                    (Cmd::SaveProject, Icon::Save),
                    (Cmd::Undo, Icon::Undo),
                    (Cmd::Redo, Icon::Redo),
                ] {
                    let enabled = match cmd {
                        Cmd::Undo => self.editor.document.history_depth().0 > 0,
                        Cmd::Redo => self.editor.document.history_depth().1 > 0,
                        _ => true,
                    } && !self.busy();
                    if icons::button(
                        ui,
                        icon,
                        16.,
                        None,
                        commands::spec(cmd).label,
                        &tooltip(cmd),
                        enabled,
                        false,
                    )
                    .clicked()
                    {
                        self.run(cmd, requests);
                    }
                }
                ui.separator();
                ui.label(format!(
                    "Open BIM — {} ({})",
                    self.editor.document.project().name,
                    self.tools.path
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(
                        ui,
                        Icon::Help,
                        18.,
                        None,
                        "Горячие клавиши",
                        &tooltip(Cmd::Help),
                        true,
                        self.help,
                    )
                    .clicked()
                    {
                        self.run(Cmd::Help, requests);
                    }
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.quick_launch)
                            .id(egui::Id::new("quick launch"))
                            .hint_text("Быстрый запуск")
                            .desired_width(200.),
                    );
                    edit.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            true,
                            "Быстрый запуск",
                        )
                    });
                    let results = if self.quick_launch.trim().is_empty() {
                        Vec::new()
                    } else {
                        commands::search(&self.quick_launch)
                    };
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(cmd) = results.first().copied() {
                            self.quick_launch.clear();
                            self.run(cmd, requests);
                        }
                    } else if edit.has_focus() && !results.is_empty() {
                        let mut chosen = None;
                        egui::Area::new(egui::Id::new("quick launch results"))
                            .fixed_pos(edit.rect.left_bottom() + egui::vec2(0., 2.))
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    for cmd in results.iter().take(8) {
                                        if ui
                                            .selectable_label(false, commands::caption(*cmd))
                                            .clicked()
                                        {
                                            chosen = Some(*cmd);
                                        }
                                    }
                                });
                            });
                        if let Some(cmd) = chosen {
                            self.quick_launch.clear();
                            self.run(cmd, requests);
                        }
                    }
                });
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .frame(egui::Frame::side_top_panel(ui.style()).fill(theme::STATUS_BG))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let white = |t: String| egui::RichText::new(t).color(theme::STATUS_TEXT);
                    let text =
                        if let Some(error) = self.engine.error.clone().filter(|_| self.picking()) {
                            // A refused pick says why; the next step follows on hover.
                            format!("⚠ {error}")
                        } else if self.picking() {
                            self.prompt()
                        } else {
                            self.tools.message.clone().unwrap_or_else(|| self.prompt())
                        };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(white(format!(
                            "{} объектов выбрано",
                            self.editor.selected.len()
                        )));
                        ui.label(white(format!(
                            "{} · {} объектов · рев. {}",
                            self.gpu,
                            self.objects,
                            self.editor.document.project().revision
                        )));
                        if let Some(kind) = self.last_snap.filter(|_| self.picking()) {
                            ui.label(white(kind.label().to_string()));
                        }
                        // The prompt takes the remaining width and is cut, never
                        // overlapping the counters on the right.
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let prompt = self.prompt();
                            ui.add(egui::Label::new(white(text.clone())).truncate())
                                .on_hover_text(format!("{text}\n{prompt}"));
                        });
                    });
                });
            });
    }

    fn selection_toolbar(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::bottom("selection toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Выбор:");
                let f = &mut self.select;
                for (icon, name, tip, flag) in [
                    (
                        Icon::SelectParts,
                        "Выбор деталей",
                        "Выбирать детали",
                        &mut f.parts,
                    ),
                    (
                        Icon::SelectAssemblies,
                        "Выбор сборок",
                        "Выбирать сборку целиком (Alt+щелчок — сборка один раз)",
                        &mut f.assemblies,
                    ),
                    (
                        Icon::SelectBolts,
                        "Выбор болтов",
                        "Выбирать болтовые группы",
                        &mut f.bolts,
                    ),
                    (
                        Icon::SelectWelds,
                        "Выбор швов",
                        "Выбирать сварные швы",
                        &mut f.welds,
                    ),
                    (
                        Icon::SelectGrid,
                        "Выбор осей",
                        "Выбирать линии сетки",
                        &mut f.grids,
                    ),
                    (
                        Icon::SelectLevels,
                        "Выбор уровней",
                        "Выбирать уровни (отметки)",
                        &mut f.levels,
                    ),
                ] {
                    if icons::button(ui, icon, 18., None, name, tip, true, *flag).clicked() {
                        *flag = !*flag;
                    }
                }
                let dm = self.direct_modification;
                if icons::button(
                    ui,
                    Icon::Move,
                    18.,
                    None,
                    "Прямое изменение",
                    &tooltip(Cmd::ToggleDirectModification),
                    true,
                    dm,
                )
                .clicked()
                {
                    self.run(Cmd::ToggleDirectModification, requests);
                }
                ui.checkbox(&mut self.extend_selection, "Добавлять к выбору");
                ui.separator();
                for cmd in [Cmd::Undo, Cmd::Redo] {
                    let enabled = match cmd {
                        Cmd::Undo => self.editor.document.history_depth().0 > 0,
                        _ => self.editor.document.history_depth().1 > 0,
                    };
                    if ui
                        .add_enabled(enabled, egui::Button::new(commands::caption(cmd)))
                        .clicked()
                    {
                        self.run(cmd, requests);
                    }
                }
                ui.separator();
                for mode in [Mode::Model, Mode::Drawings, Mode::Reports] {
                    let (label, cmd) = match mode {
                        Mode::Model => ("3D модель", Cmd::ModeModel),
                        Mode::Drawings => ("Чертежи", Cmd::ModeDrawings),
                        Mode::Reports => ("Ведомости", Cmd::ModeReports),
                    };
                    if ui
                        .selectable_label(
                            self.mode == mode,
                            format!("{label} · {}", commands::shortcut_text(cmd)),
                        )
                        .clicked()
                    {
                        self.mode = mode;
                    }
                }
            });
        });
    }

    fn snap_toolbar(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::bottom("snap toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                self.take_focus(ui, FocusArea::QuickLaunch, egui::Id::new("quick launch"));
                self.take_focus(ui, FocusArea::Search, egui::Id::new("model search"));
                let custom = self.engine.workplane != crate::interaction::Workplane::default()
                    && self.engine.workplane.axes[2] != open_bim_renderer::DVec3::Z;
                let mut choice = custom;
                let coordinate = egui::ComboBox::new("coordinate system", "Система координат")
                    .selected_text(if custom {
                        "Рабочая плоскость"
                    } else {
                        "Начало координат модели"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut choice, false, "Начало координат модели");
                        ui.selectable_value(&mut choice, true, "Рабочая плоскость по 3 точкам…");
                    });
                if self.focus_target == Some(FocusArea::Toolbar) {
                    self.focus_target = None;
                    coordinate.response.request_focus();
                }
                if choice != custom {
                    self.run(
                        if choice {
                            Cmd::WorkPlaneThreePoints
                        } else {
                            Cmd::WorkPlaneReset
                        },
                        requests,
                    );
                }
                ui.separator();
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .id(egui::Id::new("model search"))
                        .hint_text("Поиск в модели")
                        .desired_width(150.),
                );
                edit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Поиск в модели")
                });
                let go = icons::button(
                    ui,
                    Icon::Search,
                    16.,
                    None,
                    "Найти в модели",
                    "Выбрать найденные объекты",
                    true,
                    false,
                );
                if go.clicked()
                    || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.search_model();
                }
                ui.separator();
                ui.label("Привязки:");
                let sw = &mut self.switches;
                for (icon, name, flag) in [
                    (
                        Icon::SnapReference,
                        SnapKind::Reference.label(),
                        &mut sw.reference,
                    ),
                    (Icon::SnapNearest, "Геометрия", &mut sw.geometry),
                    (Icon::SnapEnd, SnapKind::EndPoint.label(), &mut sw.end),
                    (Icon::SnapMid, SnapKind::MidPoint.label(), &mut sw.mid),
                    (
                        Icon::SnapIntersection,
                        SnapKind::Intersection.label(),
                        &mut sw.intersection,
                    ),
                    (
                        Icon::SnapPerpendicular,
                        SnapKind::Perpendicular.label(),
                        &mut sw.perpendicular,
                    ),
                    (
                        Icon::SnapNearest,
                        SnapKind::Nearest.label(),
                        &mut sw.nearest,
                    ),
                ] {
                    if icons::button(
                        ui,
                        icon,
                        16.,
                        None,
                        name,
                        &format!("Привязка: {}", name.to_lowercase()),
                        true,
                        *flag,
                    )
                    .clicked()
                    {
                        *flag = !*flag;
                    }
                }
                let ortho = self.switches.ortho;
                if icons::button(
                    ui,
                    Icon::Ortho,
                    16.,
                    None,
                    "Ортогонально",
                    &tooltip(Cmd::ToggleOrtho),
                    true,
                    ortho,
                )
                .clicked()
                {
                    self.run(Cmd::ToggleOrtho, requests);
                }
                ui.add(
                    egui::DragValue::new(&mut self.snap_tolerance_px)
                        .range(4.0..=40.0)
                        .suffix(" px")
                        .prefix("Допуск "),
                );
                ui.separator();
                let z = ui.label("Высота плоскости, мм");
                ui.add(egui::TextEdit::singleline(&mut self.tools.workplane).desired_width(60.))
                    .labelled_by(z.id);
            });
        });
    }

    fn side(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::right("side strip")
            .exact_size(40.)
            .resizable(false)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for (panel, icon, cmd) in [
                        (
                            SidePanel::Properties,
                            Icon::Properties,
                            Cmd::PanelProperties,
                        ),
                        (SidePanel::Edit, Icon::Move, Cmd::PanelEdit),
                        (SidePanel::Detailing, Icon::Bolt, Cmd::PanelDetailing),
                        (
                            SidePanel::Connections,
                            Icon::Component,
                            Cmd::PanelConnections,
                        ),
                        (SidePanel::Clash, Icon::Clash, Cmd::PanelClash),
                        (SidePanel::Tree, Icon::Grid, Cmd::PanelModelTree),
                        (SidePanel::Views, Icon::View3d, Cmd::PanelViews),
                    ] {
                        let open = self.mode == Mode::Model && self.side_panel == Some(panel);
                        let response = icons::button(
                            ui,
                            icon,
                            20.,
                            None,
                            panel.title(),
                            &tooltip(cmd),
                            true,
                            open,
                        );
                        // F6: an open pane takes the focus on its first field
                        // (below); a closed one leaves it on its strip button.
                        if panel == SidePanel::Properties
                            && self.focus_target == Some(FocusArea::SidePane)
                            && !(self.mode == Mode::Model && self.side_panel.is_some())
                        {
                            self.focus_target = None;
                            response.request_focus();
                        }
                        if response.clicked() {
                            self.run(cmd, requests);
                        }
                    }
                    ui.separator();
                    for (mode, icon, cmd) in [
                        (Mode::Drawings, Icon::Drawing, Cmd::ModeDrawings),
                        (Mode::Reports, Icon::Report, Cmd::ModeReports),
                    ] {
                        if icons::button(
                            ui,
                            icon,
                            20.,
                            None,
                            commands::spec(cmd).label,
                            &tooltip(cmd),
                            true,
                            self.mode == mode,
                        )
                        .clicked()
                        {
                            self.mode = if self.mode == mode { Mode::Model } else { mode };
                        }
                    }
                });
            });
        let open = match self.mode {
            Mode::Model => self.side_panel.is_some(),
            _ => true,
        };
        if !open {
            return;
        }
        egui::Panel::right("model tools")
            .default_size(330.0)
            .min_size(280.)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let has_selection = self.selection_common().is_some();
                    let has_axes = {
                        let project = self.editor.document.project();
                        self.editor
                            .selected
                            .iter()
                            .any(|id| project.axes.contains_key(id))
                    };
                    let has_levels = {
                        let project = self.editor.document.project();
                        self.editor
                            .selected
                            .iter()
                            .any(|id| project.levels.contains_key(id))
                    };
                    ui.add_enabled_ui(!self.busy(), |ui| match self.mode {
                        Mode::Drawings => self.drawings.show(ui, &mut self.editor),
                        Mode::Reports => {
                            if let Some(cmd) = self.reports.show_panel(ui, &mut self.editor) {
                                self.run(cmd, requests);
                            }
                        }
                        Mode::Model => match self.side_panel {
                            Some(SidePanel::Properties) if self.tool_pane_active() => {
                                self.tool_pane(ui)
                            }
                            Some(SidePanel::Tree) => self.tree_panel(ui),
                            Some(SidePanel::Views) => self.views_panel(ui, requests),
                            Some(SidePanel::Properties) if has_selection => {
                                self.selection_pane(ui, requests)
                            }
                            Some(SidePanel::Properties) if has_axes => self.axis_pane(ui, requests),
                            Some(SidePanel::Properties) if has_levels => {
                                self.level_pane(ui, requests)
                            }
                            Some(panel) => {
                                if panel == SidePanel::Properties {
                                    self.object_type(ui, requests);
                                }
                                self.tools.panel = match panel {
                                    SidePanel::Edit => ToolsPanel::Edit,
                                    SidePanel::Detailing => ToolsPanel::Detailing,
                                    SidePanel::Connections => ToolsPanel::Connections,
                                    SidePanel::Clash => ToolsPanel::Clash,
                                    _ => ToolsPanel::Properties,
                                };
                                if let Some(a) = self.tools.show(ui, &mut self.editor) {
                                    match a {
                                        Action::Open => self
                                            .start_open(std::path::PathBuf::from(&self.tools.path)),
                                        other => requests.push(Request::Action(other)),
                                    }
                                }
                            }
                            None => {}
                        },
                    });
                });
                if self.focus_target == Some(FocusArea::SidePane) {
                    self.focus_target = None;
                    let (layer, area) = (ui.layer_id(), ui.max_rect());
                    let first = ui.ctx().viewport(|v| {
                        v.this_pass
                            .widgets
                            .get_layer(layer)
                            .find(|w| {
                                w.enabled && w.sense.is_focusable() && area.contains_rect(w.rect)
                            })
                            .map(|w| w.id)
                    });
                    if let Some(id) = first {
                        ui.memory_mut(|m| m.request_focus(id));
                    }
                }
            });
    }

    /// Property pane of selected grid lines: name (ГОСТ letters), end points,
    /// the whole grid dialog and delete.
    fn axis_pane(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        let project = self.editor.document.project();
        let axes: Vec<&open_bim_model::GridAxis> = self
            .editor
            .selected
            .iter()
            .filter_map(|id| project.axes.get(id))
            .collect();
        ui.heading(format!("Свойства: ось ({})", axes.len()));
        if axes.len() == 1 {
            let axis = axes[0].clone();
            if self.axis_buffer.as_ref().is_none_or(|b| b.id != axis.id) {
                self.axis_buffer = Some(axis.clone());
            }
            let buffer = self.axis_buffer.as_mut().expect("filled above");
            egui::Grid::new("axis fields")
                .num_columns(4)
                .show(ui, |ui| {
                    let l = ui.label("Обозначение");
                    ui.text_edit_singleline(&mut buffer.name).labelled_by(l.id);
                    ui.end_row();
                    for (label, p) in [
                        ("Начало, мм", &mut buffer.start),
                        ("Конец, мм", &mut buffer.end),
                    ] {
                        ui.label(label);
                        for (k, axis_name) in ["X", "Y", "Z"].iter().enumerate() {
                            ui.add(
                                egui::DragValue::new(&mut p.0[k])
                                    .speed(10.)
                                    .prefix(format!("{axis_name} ")),
                            );
                        }
                        ui.end_row();
                    }
                });
            if ui.button("Изменить").clicked() {
                let edited = buffer.clone();
                match crate::interaction::grid_edit::validate_axis_name(&edited.name) {
                    Ok(()) if edited != axis => requests.push(Request::Action(Action::Commit(
                        format!("Изменить ось {}", edited.name),
                        vec![open_bim_commands::Command::ReplaceAxis { axis: edited }],
                    ))),
                    Ok(()) => self.tools.message = Some("Нет изменений".into()),
                    Err(e) => self.tools.message = Some(e),
                }
            }
        } else {
            ui.label(
                axes.iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        ui.horizontal(|ui| {
            if ui.button(commands::caption(Cmd::GridDialog)).clicked() {
                self.run(Cmd::GridDialog, requests);
            }
            if ui.button(commands::caption(Cmd::DeleteSelected)).clicked() {
                self.run(Cmd::DeleteSelected, requests);
            }
        });
        if let Some(m) = &self.tools.message {
            ui.label(m.clone());
        }
    }

    fn tool_pane_active(&self) -> bool {
        use crate::interaction::trim::TrimTool;
        self.detail.tool.is_some()
            || matches!(self.trim.tool, Some(TrimTool::Gusset | TrimTool::Stiffener))
    }

    /// Properties of the running bolt, weld, gusset or stiffener command
    /// (Tekla opens the command's dialog the same way) with its steps.
    fn tool_pane(&mut self, ui: &mut egui::Ui) {
        use crate::interaction::tools::DetailTool;
        use crate::interaction::trim::TrimTool;
        let (title, steps): (&str, &[&str]) = match (self.detail.tool, self.trim.tool) {
            (Some(DetailTool::Bolt), _) => (
                "Свойства болтов",
                &[
                    "1. Щёлкните основную деталь, затем присоединяемые",
                    "2. Enter или средняя кнопка — детали выбраны",
                    "3. Укажите начало группы (первый болт)",
                    "4. Укажите направление ряда X — болты видны заранее",
                ],
            ),
            (Some(DetailTool::Weld), _) => (
                "Свойства шва",
                &[
                    "1. Щёлкните деталь, к которой приваривают",
                    "2. Щёлкните привариваемую деталь — шов создан",
                    "Заводской шов объединяет детали в сборку",
                ],
            ),
            (_, Some(TrimTool::Gusset)) => (
                "Косынка",
                &[
                    "Пластина в углу между двумя деталями: края на их гранях,",
                    "катеты вдоль деталей, угол срезан фаской.",
                    "1. Щёлкните первую деталь угла, 2. вторую",
                ],
            ),
            _ => (
                "Ребро жёсткости",
                &[
                    "Пластина поперёк двутавра между полками,",
                    "со срезами углов у стенки.",
                    "1. Щёлкните балку, 2. укажите место со стороны стенки",
                ],
            ),
        };
        ui.heading(title);
        for s in steps {
            ui.label(*s);
        }
        ui.separator();
        if self.detail.tool == Some(DetailTool::Bolt) {
            use crate::tools::property_files::{self as files, FileKind, FileRequest};
            let dir = files::folder(&self.tools.path);
            if let Some(request) = self.tools.files.row(ui, &dir, FileKind::Bolt) {
                self.tools.message = Some(match request {
                    FileRequest::Load(name) => match files::load(&dir, FileKind::Bolt, &name) {
                        Ok(values) => {
                            files::apply_to_bolts(&mut self.tools.detailing, &values);
                            files::loaded_message(&name, &[])
                        }
                        Err(e) => e,
                    },
                    FileRequest::Save(name) => {
                        match files::save(&dir, &name, &files::from_bolts(&self.tools.detailing)) {
                            Ok(path) => format!("Свойства сохранены: {}", path.display()),
                            Err(e) => e,
                        }
                    }
                });
            }
            ui.separator();
        }
        let d = &mut self.tools.detailing;
        match (self.detail.tool, self.trim.tool) {
            (Some(DetailTool::Bolt), _) => d.bolt_fields(ui),
            (Some(DetailTool::Weld), _) => d.weld_fields(ui),
            (_, Some(TrimTool::Gusset)) => {
                egui::Grid::new("gusset fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (caption, value) in [
                            ("Толщина, мм", &mut self.editor.draft.thickness),
                            ("Катет, мм", &mut d.gusset_leg),
                            ("Фаска угла, мм", &mut d.gusset_chamfer),
                        ] {
                            let l = ui.label(caption);
                            ui.text_edit_singleline(value).labelled_by(l.id);
                            ui.end_row();
                        }
                    });
            }
            _ => {
                egui::Grid::new("stiffener fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (caption, value) in [
                            ("Толщина, мм", &mut self.editor.draft.thickness),
                            ("Срез угла у стенки, мм", &mut d.stiffener_snip),
                        ] {
                            let l = ui.label(caption);
                            ui.text_edit_singleline(value).labelled_by(l.id);
                            ui.end_row();
                        }
                    });
            }
        }
        if let Some(m) = &self.tools.message {
            ui.separator();
            ui.label(m.clone());
        }
    }

    /// Property pane of a selected level: name and elevation mark; a new mark
    /// renames a level that was named by its mark (ГОСТ Р 21.101, 5.4.3).
    fn level_pane(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        use open_bim_model::numbering_rules::{level_mark, parse_level_mark};
        let project = self.editor.document.project();
        let levels: Vec<&open_bim_model::Level> = self
            .editor
            .selected
            .iter()
            .filter_map(|id| project.levels.get(id))
            .collect();
        ui.heading(format!("Свойства: уровень ({})", levels.len()));
        if levels.len() == 1 {
            let level = levels[0].clone();
            if self.level_buffer.as_ref().is_none_or(|b| b.0 != level.id) {
                self.level_buffer =
                    Some((level.id, level.name.clone(), level_mark(level.elevation_mm)));
            }
            let (_, name, mark) = self.level_buffer.as_mut().expect("filled above");
            egui::Grid::new("level fields")
                .num_columns(2)
                .show(ui, |ui| {
                    let l = ui.label("Имя");
                    ui.text_edit_singleline(name).labelled_by(l.id);
                    ui.end_row();
                    let l = ui.label("Отметка, м");
                    ui.add(egui::TextEdit::singleline(mark).id(egui::Id::new("level mark")))
                        .labelled_by(l.id)
                        .on_hover_text("Например «+4,200» или «-2,800»");
                    ui.end_row();
                });
            if ui.button("Изменить").clicked() {
                match parse_level_mark(mark) {
                    Some(z) => {
                        let mut edited = level.clone();
                        edited.elevation_mm = z;
                        edited.name = if name.trim() == level_mark(level.elevation_mm) {
                            level_mark(z)
                        } else {
                            name.trim().to_string()
                        };
                        if edited.name.is_empty() {
                            self.tools.message = Some("Имя уровня не может быть пустым".into());
                        } else if edited == level {
                            self.tools.message = Some("Нет изменений".into());
                        } else {
                            // Columns bound to the level follow its new mark.
                            let elevations = std::collections::BTreeMap::from([(edited.id, z)]);
                            match crate::tools::column_levels::follow(project, &elevations) {
                                Ok(columns) => {
                                    let mut commands =
                                        vec![open_bim_commands::Command::ReplaceLevel {
                                            level: edited.clone(),
                                        }];
                                    commands.extend(columns);
                                    requests.push(Request::Action(Action::Commit(
                                        format!("Изменить уровень {}", edited.name),
                                        commands,
                                    )));
                                    self.level_buffer = None;
                                }
                                Err(e) => self.tools.message = Some(e),
                            }
                        }
                    }
                    None => self.tools.message = Some(format!("Отметка «{mark}»: число в метрах")),
                }
            }
        } else {
            ui.label(
                levels
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        ui.horizontal(|ui| {
            if ui.button(commands::caption(Cmd::GridDialog)).clicked() {
                self.run(Cmd::GridDialog, requests);
            }
            if ui.button(commands::caption(Cmd::DeleteSelected)).clicked() {
                self.run(Cmd::DeleteSelected, requests);
            }
        });
        if let Some(m) = &self.tools.message {
            ui.label(m.clone());
        }
    }

    /// Tekla contextual toolbar next to the pointer after a selecting click:
    /// profile and material apply at once; «Свойства…» opens the pane.
    fn contextual_toolbar(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        use super::property_pane::{Changes, VARIES, modify};
        let Some(at) = self.context_at.filter(|_| {
            self.contextual
                && self.mode == Mode::Model
                && !self.picking()
                && self.delete_confirm.is_none()
        }) else {
            return;
        };
        let Some(c) = self.selection_common() else {
            return;
        };
        let project = self.editor.document.project();
        let mut change = Changes::default();
        let mut open_pane = false;
        egui::Area::new(egui::Id::new("contextual toolbar"))
            .fixed_pos(at + egui::vec2(18., 18.))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!(
                            "{} ({})",
                            c.kind.value().map(String::as_str).unwrap_or("Детали"),
                            c.parts.len()
                        ));
                        if let Some(profile) = &c.profile {
                            let shown = profile
                                .value()
                                .and_then(|id| project.profiles.get(id))
                                .map_or(VARIES, |p| p.name.as_str());
                            egui::ComboBox::new("ctx profile", "")
                                .selected_text(shown)
                                .height(420.)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show_ui(ui, |ui| {
                                    match super::catalog_picker::profile_menu(
                                        ui,
                                        "ctx",
                                        project,
                                        profile.value().copied(),
                                    ) {
                                        Some(super::catalog_picker::Pick::Project(id)) => {
                                            change.profile = Some(id)
                                        }
                                        Some(super::catalog_picker::Pick::New(p)) => {
                                            change.profile = Some(p.id);
                                            change.new_profile = Some(p);
                                        }
                                        None => {}
                                    }
                                })
                                .response
                                .widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::ComboBox,
                                        true,
                                        "Профиль выбранных",
                                    )
                                });
                        }
                        let shown = c
                            .material
                            .value()
                            .and_then(|id| project.materials.get(id))
                            .map_or(VARIES, |m| m.name.as_str());
                        egui::ComboBox::new("ctx material", "")
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                match super::catalog_picker::material_menu(
                                    ui,
                                    project,
                                    c.material.value().copied(),
                                ) {
                                    Some(super::catalog_picker::Pick::Project(id)) => {
                                        change.material = Some(id)
                                    }
                                    Some(super::catalog_picker::Pick::New(m)) => {
                                        change.material = Some(m.id);
                                        change.new_material = Some(m);
                                    }
                                    None => {}
                                }
                            })
                            .response
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::ComboBox,
                                    true,
                                    "Материал выбранных",
                                )
                            });
                        open_pane = ui.button("Свойства…").clicked();
                    });
                });
            });
        if change != Changes::default() {
            match modify(project, &c.parts, &change) {
                Ok(commands) => requests.push(Request::Action(Action::Commit(
                    "Изменить свойства".into(),
                    commands,
                ))),
                Err(e) => self.tools.message = Some(e),
            }
            self.pane.source.clear();
        }
        if open_pane {
            self.side_panel = Some(SidePanel::Properties);
            self.context_at = None;
        }
    }

    /// Tekla property pane with nothing selected: the object type list starts
    /// a create command; the tool's default properties follow.
    fn object_type(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        let tools = [
            (Cmd::ToolBeam, "Балка"),
            (Cmd::ToolColumn, "Колонна"),
            (Cmd::ToolPlate, "Контурная пластина"),
            (Cmd::ToolBolt, "Болт"),
            (Cmd::ToolWeld, "Сварной шов"),
            (Cmd::ToolAxis, "Ось"),
            (Cmd::GridDialog, "Сетка"),
        ];
        let current = self
            .engine
            .tool
            .map(|k| k.label())
            .unwrap_or("Выберите тип объекта");
        let mut chosen = None;
        egui::ComboBox::new("object type", "Тип объекта")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (cmd, label) in tools {
                    if ui.selectable_label(false, label).clicked() {
                        chosen = Some(cmd);
                    }
                }
            })
            .response
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Тип объекта")
            });
        if let Some(cmd) = chosen {
            self.run(cmd, requests);
        }
    }

    /// Common properties of the selection, recomputed only when the model
    /// revision or the selection changes.
    pub fn selection_common(&mut self) -> Option<Arc<super::property_pane::Common>> {
        use super::property_pane::{cache_key, common};
        let project = self.editor.document.project();
        let key = cache_key(project, &self.editor.selected);
        match &self.pane.cache {
            Some((k, c)) if *k == key => c.clone(),
            _ => {
                let c = common(project, &self.editor.selected).map(Arc::new);
                self.pane.cache = Some((key, c.clone()));
                c
            }
        }
    }

    /// Property file row of the selection pane: «Загрузить» fills the fields
    /// (applied by «Изменить»), «Сохранить как» stores one selected part.
    /// A column bound to levels shows them.
    fn selection_files(
        &mut self,
        ui: &mut egui::Ui,
        kind: crate::tools::property_files::FileKind,
        parts: &[open_bim_model::ObjectId],
    ) {
        use crate::tools::column_levels::{
            BOTTOM_LEVEL, BOTTOM_OFFSET, TOP_LEVEL, TOP_OFFSET, bound,
        };
        use crate::tools::property_files::{self as files, FileRequest};
        let dir = files::folder(&self.tools.path);
        if let Some(request) = self.tools.files.row(ui, &dir, kind) {
            let project = self.editor.document.project();
            let saved = match parts {
                [id] => project
                    .elements
                    .get(id)
                    .and_then(|e| files::from_element(project, e)),
                _ => None,
            };
            self.tools.message = Some(match request {
                FileRequest::Load(name) => match files::load(&dir, kind, &name)
                    .and_then(|v| files::import_catalogue(&mut self.editor, &v).map(|()| v))
                {
                    Ok(values) => {
                        let project = self.editor.document.project();
                        let warnings = self.pane.apply_file(project, &values);
                        format!(
                            "{} — нажмите «Изменить»",
                            files::loaded_message(&name, &warnings)
                        )
                    }
                    Err(e) => e,
                },
                FileRequest::Save(name) => match (parts, saved) {
                    (_, Some(values)) => match files::save(&dir, &name, &values) {
                        Ok(path) => format!("Свойства сохранены: {}", path.display()),
                        Err(e) => e,
                    },
                    ([_], None) => "Сохраняются свойства балки или колонны".into(),
                    _ => "Для «Сохранить как» выберите одну деталь".into(),
                },
            });
        }
        let project = self.editor.document.project();
        if let [id] = parts
            && let Some(e) = project.elements.get(id)
        {
            use open_bim_model::numbering_rules::level_mark;
            for (caption, level, offset) in [
                ("Низ", BOTTOM_LEVEL, BOTTOM_OFFSET),
                ("Верх", TOP_LEVEL, TOP_OFFSET),
            ] {
                if let Some((level, offset)) = bound(&e.properties, level, offset)
                    && let Some(level) = project.levels.get(&level)
                {
                    ui.small(format!(
                        "{caption}: уровень {} ({}), смещение {} мм",
                        level.name,
                        level_mark(level.elevation_mm),
                        super::property_pane::number_text(offset)
                    ));
                }
            }
        }
        ui.separator();
    }

    /// Common properties of the selected parts; «Изменить» applies the edits.
    fn selection_pane(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        use super::property_pane::{Shared, VARIES, modify};
        let Some(c) = self.selection_common() else {
            return;
        };
        self.pane.sync(&c);
        let kind = c.kind.value().cloned().unwrap_or_else(|| "Детали".into());
        ui.heading(format!("Свойства: {kind} ({})", c.parts.len()));
        let file_kind = match kind.as_str() {
            "Балка" => Some(crate::tools::property_files::FileKind::Beam),
            "Колонна" => Some(crate::tools::property_files::FileKind::Column),
            _ => None,
        };
        if let Some(file_kind) = file_kind {
            self.selection_files(ui, file_kind, &c.parts);
        }
        let project = self.editor.document.project();
        let mut apply_length = false;
        egui::Grid::new("selection properties")
            .num_columns(2)
            .show(ui, |ui| {
                let l = ui.label("Имя");
                ui.text_edit_singleline(&mut self.pane.name)
                    .labelled_by(l.id);
                ui.end_row();
                if let Some(profile) = &c.profile {
                    ui.label("Профиль");
                    let shown = super::catalog_picker::profile_name(
                        project,
                        self.pane.pending_profile.as_ref(),
                        self.pane.profile,
                    )
                    .unwrap_or(VARIES)
                    .to_string();
                    egui::ComboBox::new("pane profile", "")
                        .selected_text(shown)
                        .height(420.)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show_ui(ui, |ui| {
                            match super::catalog_picker::profile_menu(
                                ui,
                                "pane",
                                project,
                                self.pane.profile,
                            ) {
                                Some(super::catalog_picker::Pick::Project(id)) => {
                                    self.pane.profile = Some(id)
                                }
                                Some(super::catalog_picker::Pick::New(p)) => {
                                    self.pane.profile = Some(p.id);
                                    self.pane.pending_profile = Some(p);
                                }
                                None => {}
                            }
                        })
                        .response
                        .widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Профиль")
                        });
                    if matches!(profile, Shared::Varies) && self.pane.profile.is_none() {
                        ui.small(VARIES);
                    }
                    ui.end_row();
                }
                ui.label("Материал");
                let shown = super::catalog_picker::material_name(
                    project,
                    self.pane.pending_material.as_ref(),
                    self.pane.material,
                )
                .unwrap_or(VARIES)
                .to_string();
                egui::ComboBox::new("pane material", "")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        match super::catalog_picker::material_menu(ui, project, self.pane.material)
                        {
                            Some(super::catalog_picker::Pick::Project(id)) => {
                                self.pane.material = Some(id)
                            }
                            Some(super::catalog_picker::Pick::New(m)) => {
                                self.pane.material = Some(m.id);
                                self.pane.pending_material = Some(m);
                            }
                            None => {}
                        }
                    })
                    .response
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Материал")
                    });
                ui.end_row();
                let l = ui.label("Класс");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.pane.class).desired_width(60.))
                        .labelled_by(l.id);
                    if let Some(rgb) = open_bim_renderer::class_color(&self.pane.class) {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(14., 14.), egui::Sense::hover());
                        let c = |v: f32| (v * 255.) as u8;
                        ui.painter().rect_filled(
                            r,
                            2.,
                            egui::Color32::from_rgb(c(rgb[0]), c(rgb[1]), c(rgb[2])),
                        );
                    }
                });
                ui.end_row();
                if c.length.is_some() {
                    let l = ui.label("Длина, мм");
                    let field = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.pane.length)
                                .id(egui::Id::new("pane length"))
                                .desired_width(90.),
                        )
                        .labelled_by(l.id);
                    // Enter in the field applies at once, like Tekla's «Изменить».
                    apply_length =
                        field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.end_row();
                    let l = ui.label("Изменять");
                    egui::ComboBox::new("pane length mode", "")
                        .selected_text(self.pane.length_mode.label())
                        .show_ui(ui, |ui| {
                            for mode in crate::interaction::member_edit::LengthMode::ALL {
                                ui.selectable_value(&mut self.pane.length_mode, mode, mode.label());
                            }
                        })
                        .response
                        .on_hover_text(
                            "Какой конец переносится при новой длине и при перетаскивании ручки",
                        )
                        .labelled_by(l.id);
                    ui.end_row();
                } else {
                    ui.label("Размер");
                    ui.label(c.size.value().cloned().unwrap_or_else(|| VARIES.into()));
                    ui.end_row();
                }
            });
        if c.position.is_some() {
            egui::CollapsingHeader::new("Положение")
                .default_open(true)
                .show(ui, |ui| {
                    super::property_pane::position_grid(ui, "pane", &mut self.pane.position);
                    ui.small(
                        "Ручки — на опорной линии; профиль и положение меняются относительно неё",
                    );
                });
        }
        ui.separator();
        if let Some(a) = self.tools.user_property(ui, &mut self.editor) {
            requests.push(Request::Action(a));
        }
        ui.separator();
        let project = self.editor.document.project();
        ui.horizontal(|ui| {
            if ui.button("Изменить").clicked() || apply_length {
                match self
                    .pane
                    .changes(&c)
                    .and_then(|changes| modify(project, &c.parts, &changes))
                {
                    Ok(commands) => requests.push(Request::Action(Action::Commit(
                        format!("Изменить свойства ({})", c.parts.len()),
                        commands,
                    ))),
                    Err(e) => self.tools.message = Some(e),
                }
                self.pane.source.clear();
            }
            if ui
                .button("Получить")
                .on_hover_text("Заново прочитать свойства выбранных")
                .clicked()
            {
                self.pane.source.clear();
            }
        });
    }

    fn views_panel(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        ui.heading(SidePanel::Views.title());
        // Display mode and transparency (Ctrl+1 / Ctrl+2 / Ctrl+4).
        ui.horizontal(|ui| {
            for mode in open_bim_renderer::RenderMode::ALL {
                ui.selectable_value(&mut self.display.mode, mode, mode.label());
            }
        });
        let mut percent = self.display.transparency * 100.;
        let l = ui.label("Прозрачность, %");
        if ui
            .add(egui::Slider::new(&mut percent, 0.0..=90.0).step_by(5.))
            .labelled_by(l.id)
            .changed()
        {
            self.display.transparency = percent / 100.;
            self.display.mode = open_bim_renderer::RenderMode::Transparent;
        }
        let l = ui.label("Прозрачными делать");
        egui::ComboBox::from_id_salt("transparency scope")
            .selected_text(self.display.scope.label())
            .show_ui(ui, |ui| {
                for scope in open_bim_renderer::TransparencyScope::ALL {
                    ui.selectable_value(&mut self.display.scope, scope, scope.label());
                }
            })
            .response
            .labelled_by(l.id);
        ui.separator();
        if ui.button(commands::caption(Cmd::GridViews)).clicked() {
            self.run(Cmd::GridViews, requests);
        }
        ui.separator();
        let mut chosen = None;
        for (i, view) in self.views.iter().enumerate() {
            if ui
                .selectable_label(self.active_view == i, &view.name)
                .clicked()
            {
                chosen = Some(i);
            }
        }
        if let Some(i) = chosen
            && let Some(r) = self.apply_view(i)
        {
            requests.push(r);
        }
        ui.separator();
        ui.small("Ctrl+P — 3D / плоский вид · глубина плана ±1000 мм, фасада ±500 мм");
    }

    fn tree_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(SidePanel::Tree.title());
        let project = self.editor.document.project();
        if project.elements.is_empty() && project.axes.is_empty() {
            ui.label("Модель пуста: Alt+B — балка, ⌘O — открыть, ⌘P — все команды");
        }
        let key = (self.editor.events.len(), project.revision);
        if self.tree.0 != key {
            let rows: Vec<(ObjectId, String)> = project
                .elements
                .values()
                .map(|e| (e.id, format!("{} · #{}", e.name, e.id.0)))
                .chain(
                    project
                        .axes
                        .values()
                        .map(|a| (a.id, format!("{} · #{}", a.name, a.id.0))),
                )
                .collect();
            self.tree = (key, Arc::new(rows));
        }
        let rows = self.tree.1.clone();
        let row_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        egui::ScrollArea::vertical()
            .id_salt("model tree rows")
            .max_height(ui.available_height().max(120.))
            .auto_shrink([false, true])
            .show_rows(ui, row_height, rows.len(), |ui, range| {
                for (id, label) in &rows[range] {
                    let id = *id;
                    ui.push_id(id.0, |ui| {
                        ui.horizontal(|ui| {
                            let mut visible = !self.hidden.contains(&id);
                            let toggle = ui
                                .add(egui::Checkbox::without_text(&mut visible))
                                .on_hover_text("Видимость");
                            toggle.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    visible,
                                    format!("Видимость: {label}"),
                                )
                            });
                            if toggle.changed() {
                                if visible {
                                    self.hidden.remove(&id);
                                } else {
                                    self.hidden.insert(id);
                                }
                            }
                            let row = ui.add(egui::Button::selectable(
                                self.editor.selected.contains(&id),
                                label.as_str(),
                            ));
                            if row.clicked() {
                                let extend = self.extend_selection
                                    || ui.input(|i| i.modifiers.shift || i.modifiers.command);
                                select(&mut self.editor.selected, Some(id), extend);
                            }
                        });
                    });
                }
            });
        ui.separator();
        ui.checkbox(&mut self.section_enabled, "Сечение Z ≥ высота");
        ui.add_enabled(
            self.section_enabled,
            egui::Slider::new(&mut self.section_height, 0.0..=50000.0)
                .suffix(" мм")
                .text("Высота сечения"),
        );
    }
}
