use eframe::egui;
use egui_wgpu::wgpu;
use open_bim_desktop::tools::{
    Editor, number,
    ui::{Action, ToolsUi, select, snap_label},
};
use open_bim_model::{ObjectId, Project, example_steel_frame};
use open_bim_renderer::{
    Camera, ClipPlane, DVec3, GpuRenderer, NamedView, Scene, ViewState, ViewportCallback,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Options {
    example: bool,
    open: Option<String>,
    capture: Option<String>,
    gpu_error_test: bool,
    selected: Option<ObjectId>,
    section: Option<f64>,
    record: Option<PathBuf>,
    capture_dir: Option<PathBuf>,
    project_path: Option<String>,
}
impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--example" => options.example = true,
                "--record" => {
                    options.record = Some(args.next().ok_or("--record требует путь")?.into())
                }
                "--capture-dir" => {
                    options.capture_dir =
                        Some(args.next().ok_or("--capture-dir требует путь")?.into())
                }
                "--project-path" => {
                    options.project_path = Some(args.next().ok_or("--project-path требует путь")?)
                }
                "--gpu-error-test" => options.gpu_error_test = true,
                "--open" => options.open = Some(args.next().ok_or("--open требует путь")?),
                "--capture" => {
                    options.capture = Some(args.next().ok_or("--capture требует путь PNG")?)
                }
                "--select" => {
                    options.selected = Some(ObjectId(
                        args.next()
                            .ok_or("--select требует ID")?
                            .parse()
                            .map_err(|_| "Некорректный ID")?,
                    ))
                }
                "--section" => {
                    options.section = Some(
                        args.next()
                            .ok_or("--section требует высоту")?
                            .parse::<f64>()
                            .map_err(|_| "Некорректная высота")?,
                    )
                }
                _ => return Err(format!("Неизвестный аргумент: {arg}")),
            }
        }
        if options.section.is_some_and(|v| !v.is_finite()) {
            return Err("Высота должна быть конечной".into());
        }
        Ok(options)
    }
}
struct Desktop {
    editor: Editor,
    tools: ToolsUi,
    capture_dir: Option<PathBuf>,
    manual_capture: Option<String>,
    capture_count: usize,
    extend_selection: bool,
    scene: Arc<Scene>,
    camera: Camera,
    view: ViewState,
    error: Arc<Mutex<Option<String>>>,
    gpu: String,
    capture: Option<String>,
    frames: u32,
    section_enabled: bool,
    section_height: f64,
}
impl Desktop {
    fn new(
        cc: &eframe::CreationContext<'_>,
        options: Options,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let project = if let Some(path) = options.open {
            open_bim_persistence::load(&path)?
        } else if options.example {
            example_steel_frame()?
        } else {
            Project::default()
        };
        let scene = Arc::new(Scene::from_project(&project)?);
        let mut editor = Editor::new(project.clone())?;
        editor.record_path = options.record;
        if let Some(id) = options.selected {
            editor.selected.insert(id);
        }
        let mut tools = ToolsUi::default();
        if let Some(path) = options.project_path {
            tools.path = path;
        }
        let mut camera = Camera::default();
        camera.fit(scene.bounds);
        let error = Arc::new(Mutex::new(None));
        let state = cc.wgpu_render_state.as_ref().ok_or("GPU недоступен")?;
        let gpu = format!(
            "{} · {:?}",
            state.adapter.get_info().name,
            state.adapter.get_info().backend
        );
        eprintln!(
            "GPU: {gpu}; project={} objects={}",
            project.name,
            scene.instances.len()
        );
        let error_copy = error.clone();
        let ctx = cc.egui_ctx.clone();
        state.device.on_uncaptured_error(Arc::new(move |fault| {
            let message = format!("{fault}");
            eprintln!("GPU error: {message}");
            *error_copy.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
            ctx.request_repaint();
        }));
        let renderer = GpuRenderer::new(&state.device, state.target_format);
        state.renderer.write().callback_resources.insert(renderer);
        if options.gpu_error_test {
            // Real wgpu validation failure; use the same handler as runtime GPU errors.
            let _invalid = state.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("explicit GPU error diagnostic"),
                size: state.device.limits().max_buffer_size.saturating_add(1),
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            });
        }
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Ok(Self {
            editor,
            tools,
            capture_dir: options.capture_dir,
            manual_capture: None,
            capture_count: 0,
            extend_selection: false,
            scene,
            camera,
            view: ViewState {
                selected: options.selected,
                section: options.section.map(|offset| ClipPlane {
                    normal: DVec3::Z,
                    offset,
                }),
                ..Default::default()
            },
            error,
            gpu,
            capture: options.capture,
            frames: 0,
            section_enabled: options.section.is_some(),
            section_height: options.section.unwrap_or(1500.0),
        })
    }
    fn action(&mut self, action: Action) {
        let fit = self.scene.instances.is_empty()
            || matches!(action, Action::New | Action::Example | Action::Open);
        let result = match action {
            Action::Create => self.editor.create(),
            Action::Detail(commands) => self.editor.commit("Деталировка", &commands),
            Action::Connection(commands) => self.editor.commit("Соединение", &commands),
            Action::Edit(edit) => self.editor.edit(&edit),
            Action::Undo => self.editor.history(false),
            Action::Redo => self.editor.history(true),
            Action::New => self.editor.replace(Project::default(), "Новый проект"),
            Action::Example => example_steel_frame()
                .map_err(|e| e.to_string())
                .and_then(|p| self.editor.replace(p, "Контрольный каркас")),
            Action::Open => self.editor.open(std::path::Path::new(&self.tools.path)),
            Action::Save => {
                match self.editor.save(std::path::Path::new(&self.tools.path)) {
                    Ok(()) => self.tools.message = Some("Проект сохранён".into()),
                    Err(error) => self.tools.message = Some(error),
                };
                return;
            }
        };
        match result {
            Ok(scene) => {
                if fit {
                    self.camera.fit(scene.bounds);
                }
                self.scene = scene;
                self.tools.message = Some(format!(
                    "Готово · ревизия {}",
                    self.editor.document.project().revision
                ));
            }
            Err(error) => self.tools.message = Some(error),
        }
    }
    fn screenshot(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        if ctx.input(|i| i.key_pressed(egui::Key::F12))
            && let Some(directory) = &self.capture_dir
        {
            self.capture_count += 1;
            self.manual_capture = Some(
                directory
                    .join(format!("step-{}.png", self.capture_count))
                    .to_string_lossy()
                    .into_owned(),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self.capture.is_some() && self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Screenshot { image, .. } = event
                && let Some(path) = self.capture.clone().or_else(|| self.manual_capture.take())
            {
                let rgba: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::save_buffer(
                    &path,
                    &rgba,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {
                        eprintln!("CAPTURE_SAVED: {path} {}x{}", image.size[0], image.size[1])
                    }
                    Err(error) => eprintln!("CAPTURE_ERROR: {error}"),
                }
                if self.capture.take().is_some() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if self.capture.is_some() {
            ctx.request_repaint();
        }
    }
}
impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.screenshot(&ctx);
        let mut action = None;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.editor.cancel();
            self.tools.message = Some("Инструмент отменён".into());
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            action = Some(if ctx.input(|i| i.modifiers.shift) {
                Action::Redo
            } else {
                Action::Undo
            });
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            action = Some(Action::Save);
        }
        egui::Panel::top("tools").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Open BIM");
                ui.separator();
                if ui.button("Новый проект").clicked() {
                    action = Some(Action::New);
                }
                if ui.button("Контрольный каркас").clicked() {
                    action = Some(Action::Example);
                }
                ui.separator();
                if ui.button("Вписать · F").clicked() {
                    self.camera.fit(self.scene.bounds);
                }
                for (name, view) in [
                    ("Изометрия · 1", NamedView::Isometric),
                    ("Спереди · 2", NamedView::Front),
                    ("Сверху · 3", NamedView::Top),
                    ("Справа · 4", NamedView::Right),
                ] {
                    if ui.button(name).clicked() {
                        self.camera.set_view(view);
                    }
                }
            });
        });
        egui::Panel::top("history").show(ui, |ui| {
            ui.horizontal(|ui| {
                let (undo, redo) = self.editor.document.history_depth();
                if ui
                    .add_enabled(undo > 0, egui::Button::new("Отменить · ⌘Z"))
                    .clicked()
                {
                    action = Some(Action::Undo);
                }
                if ui
                    .add_enabled(redo > 0, egui::Button::new("Повторить · ⇧⌘Z"))
                    .clicked()
                {
                    action = Some(Action::Redo);
                }
                ui.checkbox(&mut self.extend_selection, "Групповой выбор");
                ui.label("Shift/⌘ + выбор — добавить объект");
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {} объектов · мм",
                    self.editor.document.project().name,
                    self.scene.instances.len()
                ));
                ui.separator();
                ui.label(&self.gpu);
            });
        });
        egui::Panel::right("model tools")
            .default_size(320.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(a) = self.tools.show(ui, &mut self.editor) {
                        action = Some(a);
                    }
                });
            });
        egui::Panel::left("model").default_size(225.0).show(ui,|ui| {
            ui.heading("Модель");ui.label("Стальные конструкции");ui.separator();
            egui::ScrollArea::vertical().max_height(340.0).show(ui,|ui| {
                let project=self.editor.document.project();
                for (id,name) in project.elements.values().map(|e|(e.id,e.name.as_str())).chain(project.axes.values().map(|a|(a.id,a.name.as_str()))) {
                    ui.push_id(id.0,|ui| {ui.horizontal(|ui| {
                        let mut visible=!self.view.hidden.contains(&id);
                        if ui.checkbox(&mut visible,"").changed() {if visible {self.view.hidden.remove(&id);} else {self.view.hidden.insert(id);}}
                        if ui.selectable_label(self.editor.selected.contains(&id),format!("{name} · #{}",id.0)).clicked() {let extend=self.extend_selection || ui.input(|i|i.modifiers.shift || i.modifiers.command);select(&mut self.editor.selected,Some(id),extend);}
                    });});
                }
            });
            ui.separator();ui.checkbox(&mut self.section_enabled,"Сечение Z ≥ высота");
            ui.add_enabled(self.section_enabled,egui::Slider::new(&mut self.section_height,0.0..=5000.0).suffix(" мм"));
            self.view.section=self.section_enabled.then_some(ClipPlane {normal:DVec3::Z,offset:self.section_height});
            ui.separator();ui.label("ЛКМ — выбор / точка инструмента\nПКМ — вращение\nСКМ / Shift+ПКМ — панорама\nКолесо / + − — масштаб\n1–4 — виды, F — вписать\nEsc — отменить инструмент");
        });
        if let Some(action) = action {
            self.action(action);
        }
        self.view.selected_ids = self.editor.selected.clone();
        self.view.selected = self.editor.selected.first().copied();
        egui::CentralPanel::default().show(ui, |ui| {
            let fault = self.error.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(fault) = fault {
                ui.heading("Ошибка GPU");
                ui.label("Трёхмерный вид остановлен. Данные модели сохранены в памяти.");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(fault);
                });
                return;
            }
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            if response.hovered() || response.has_focus() {
                ctx.input(|input| {
                    if !input.modifiers.command && !input.modifiers.ctrl {
                        for (key, view) in [
                            (egui::Key::Num1, NamedView::Isometric),
                            (egui::Key::Num2, NamedView::Front),
                            (egui::Key::Num3, NamedView::Top),
                            (egui::Key::Num4, NamedView::Right),
                        ] {
                            if input.key_pressed(key) {
                                self.camera.set_view(view);
                            }
                        }
                        if input.key_pressed(egui::Key::F) {
                            self.camera.fit(self.scene.bounds);
                        }
                        if input.key_pressed(egui::Key::Plus)
                            || input.key_pressed(egui::Key::Equals)
                        {
                            self.camera.zoom(100.0);
                        }
                        if input.key_pressed(egui::Key::Minus) {
                            self.camera.zoom(-100.0);
                        }
                        let dx = if input.key_pressed(egui::Key::ArrowLeft) {
                            -15.0
                        } else if input.key_pressed(egui::Key::ArrowRight) {
                            15.0
                        } else {
                            0.0
                        };
                        let dy = if input.key_pressed(egui::Key::ArrowUp) {
                            -15.0
                        } else if input.key_pressed(egui::Key::ArrowDown) {
                            15.0
                        } else {
                            0.0
                        };
                        if input.modifiers.shift {
                            self.camera.pan(dx, dy, rect.height() as f64);
                        } else {
                            self.camera.orbit(dx, dy);
                        }
                    }
                    if response.hovered() {
                        self.camera.zoom(input.smooth_scroll_delta.y as f64);
                    }
                    let delta = input.pointer.delta();
                    if response.dragged_by(egui::PointerButton::Middle)
                        || (response.dragged_by(egui::PointerButton::Secondary)
                            && input.modifiers.shift)
                    {
                        self.camera
                            .pan(delta.x as f64, delta.y as f64, rect.height() as f64);
                    } else if response.dragged_by(egui::PointerButton::Secondary) {
                        self.camera.orbit(delta.x as f64, delta.y as f64);
                    }
                });
            }
            let hover = response.hover_pos().and_then(|point| {
                let uv = [
                    ((point.x - rect.min.x) / rect.width()) as f64,
                    ((point.y - rect.min.y) / rect.height()) as f64,
                ];
                self.camera
                    .ray(self.scene.origin, uv, rect.aspect_ratio() as f64)
            });
            let cursor = if self.editor.active {
                hover.and_then(|ray| {
                    let z = number(&self.tools.workplane).ok()?;
                    if ray.direction.z.abs() < 1e-10 {
                        return None;
                    }
                    let distance = (z - ray.origin.z) / ray.direction.z;
                    if distance < 0.0 {
                        return None;
                    }
                    let p =
                        open_bim_model::Point3((ray.origin + distance * ray.direction).to_array());
                    let hit = if self.tools.snap_enabled {
                        self.editor.snap.snap(
                            p,
                            self.tools.snap_tolerance,
                            self.tools.snap_mode,
                            &self.view.hidden,
                        )
                    } else {
                        None
                    };
                    Some((hit.map_or(p, |h| h.point), hit))
                })
            } else {
                None
            };
            if response.clicked() {
                response.request_focus();
                if self.editor.active {
                    if let Some((point, _)) = cursor {
                        self.editor.accept_point(point);
                    }
                } else {
                    let id = hover.and_then(|ray| self.scene.pick(ray, &self.view));
                    let extend = self.extend_selection
                        || ctx.input(|i| i.modifiers.shift || i.modifiers.command);
                    select(&mut self.editor.selected, id, extend);
                    self.view.selected_ids = self.editor.selected.clone();
                    self.view.selected = self.editor.selected.first().copied();
                    eprintln!("PICK: {:?}", self.view.selected);
                }
            }
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    scene: self.scene.clone(),
                    camera: self.camera.clone(),
                    state: self.view.clone(),
                    size: [rect.width(), rect.height()],
                },
            ));
            let screen = |point: open_bim_model::Point3| {
                self.camera
                    .project(
                        self.scene.origin,
                        DVec3::from_array(point.0),
                        rect.aspect_ratio() as f64,
                    )
                    .map(|uv| {
                        egui::pos2(
                            rect.min.x + uv[0] as f32 * rect.width(),
                            rect.min.y + uv[1] as f32 * rect.height(),
                        )
                    })
            };
            for axis in self.editor.document.project().axes.values() {
                if self.view.hidden.contains(&axis.id) {
                    continue;
                }
                if let (Some(a), Some(b)) = (screen(axis.start), screen(axis.end)) {
                    ui.painter().line_segment(
                        [a, b],
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(85, 130, 130)),
                    );
                    ui.painter().text(
                        a,
                        egui::Align2::CENTER_BOTTOM,
                        &axis.name,
                        egui::FontId::proportional(12.0),
                        egui::Color32::LIGHT_BLUE,
                    );
                }
            }
            if self.editor.active {
                for points in self.editor.picked.windows(2) {
                    if let (Some(a), Some(b)) = (screen(points[0]), screen(points[1])) {
                        ui.painter()
                            .line_segment([a, b], egui::Stroke::new(2.0, egui::Color32::YELLOW));
                    }
                }
                if let Some((point, hit)) = cursor {
                    if let Some(position) = screen(point) {
                        ui.painter().circle_stroke(
                            position,
                            5.0,
                            egui::Stroke::new(2.0, egui::Color32::YELLOW),
                        );
                        if let Some(previous) = self.editor.picked.last().and_then(|p| screen(*p)) {
                            ui.painter().line_segment(
                                [previous, position],
                                egui::Stroke::new(1.0, egui::Color32::YELLOW),
                            );
                        }
                    }
                    ui.painter().text(
                        rect.left_bottom() - egui::vec2(-12.0, 12.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!(
                            "{}: {:.3}, {:.3}, {:.3} мм",
                            hit.map_or("Точка", |h| snap_label(h.kind)),
                            point.0[0],
                            point.0[1],
                            point.0[2]
                        ),
                        egui::FontId::monospace(13.0),
                        egui::Color32::YELLOW,
                    );
                }
            }
            if self.scene.instances.is_empty() && self.editor.document.project().axes.is_empty() {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Проект пуст\nОткройте контрольный каркас",
                    egui::FontId::proportional(20.0),
                    egui::Color32::LIGHT_GRAY,
                );
            }
            ui.painter().text(
                rect.left_top() + egui::vec2(15.0, 15.0),
                egui::Align2::LEFT_TOP,
                if self.camera.orthographic {
                    "ОРТОГРАФИЯ"
                } else {
                    "ПЕРСПЕКТИВА"
                },
                egui::FontId::monospace(12.0),
                egui::Color32::GRAY,
            );
        });
    }
}
fn main() -> eframe::Result {
    let options = Options::parse().map_err(|e| eframe::Error::AppCreation(e.into()))?;
    eframe::run_native(
        "Open BIM",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([1000.0, 600.0]),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(Desktop::new(cc, options)?))),
    )
}
