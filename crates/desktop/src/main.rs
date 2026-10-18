use eframe::egui;
use egui_wgpu::wgpu;
use open_bim_desktop::tools::{Editor, number, ui::Action};
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{ObjectId, example_steel_frame};
/// Work area box margin around the model bounds, mm.
const WORK_AREA_MARGIN_MM: f64 = 500.0;
use open_bim_renderer::{
    Camera, ClipPlane, DVec3, GpuRenderer, Scene, ViewState, ViewportCallback, overlay,
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
    select_all: bool,
    section: Option<f64>,
    record: Option<PathBuf>,
    capture_dir: Option<PathBuf>,
    project_path: Option<String>,
    script: Option<PathBuf>,
    size: Option<[f32; 2]>,
    zoom: Option<f32>,
    load_delay_ms: u64,
    bench: Option<PathBuf>,
    bench_frames: usize,
}
impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--example" => options.example = true,
                "--select-all" => options.select_all = true,
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
                "--script" => {
                    options.script = Some(args.next().ok_or("--script требует путь JSON")?.into())
                }
                "--size" => {
                    let text = args.next().ok_or("--size требует ШxВ")?;
                    let (w, h) = text.split_once('x').ok_or("--size: формат 1280x800")?;
                    let parse = |v: &str| {
                        v.parse::<f32>()
                            .ok()
                            .filter(|v| (400.0..=8000.0).contains(v))
                    };
                    options.size = Some([parse(w).ok_or("ширина")?, parse(h).ok_or("высота")?]);
                }
                "--bench" => {
                    options.bench = Some(args.next().ok_or("--bench требует путь JSON")?.into())
                }
                "--bench-frames" => {
                    options.bench_frames = args
                        .next()
                        .and_then(|v| v.parse().ok())
                        .filter(|v| (10..=100_000).contains(v))
                        .ok_or("--bench-frames требует число 10…100000")?
                }
                "--zoom" => {
                    options.zoom = Some(
                        args.next()
                            .and_then(|v| v.parse::<f32>().ok())
                            .filter(|v| (0.25..=4.0).contains(v))
                            .ok_or("--zoom требует число 0.25–4")?,
                    )
                }
                "--load-delay-ms" => {
                    options.load_delay_ms = args
                        .next()
                        .and_then(|v| v.parse().ok())
                        .filter(|v| *v <= 600_000)
                        .ok_or("--load-delay-ms требует миллисекунды")?
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
    bench: Workbench,
    capture_dir: Option<PathBuf>,
    manual_capture: Option<String>,
    capture_count: usize,
    scene: Arc<Scene>,
    camera: Camera,
    view: ViewState,
    error: Arc<Mutex<Option<String>>>,
    capture: Option<String>,
    frames: u32,
    script: Option<open_bim_desktop::script::Runner>,
    /// Last 3D view rectangle (for scripted world clicks).
    viewport: Option<egui::Rect>,
    measure: Option<open_bim_desktop::bench::Bench>,
    /// Requests raised inside the central panel, handled next frame.
    pending: Vec<Request>,
    load_ms: f64,
}
impl Desktop {
    fn new(
        cc: &eframe::CreationContext<'_>,
        options: Options,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let started = std::time::Instant::now();
        let project = if let Some(path) = options.open {
            open_bim_persistence::load(&path)?
        } else if options.example {
            example_steel_frame()?
        } else {
            // Like a new Tekla model: default grid and levels.
            open_bim_desktop::interaction::tools::default_project()?
        };
        let script = match &options.script {
            Some(path) => Some(open_bim_desktop::script::Runner::new(
                open_bim_desktop::script::parse(&std::fs::read_to_string(path)?)?,
            )),
            None => None,
        };
        let scene = Arc::new(Scene::from_project(&project)?);
        let load_ms = started.elapsed().as_secs_f64() * 1000.;
        eprintln!("LOAD_MS: {load_ms:.1}");
        let mut editor = Editor::new(project.clone())?;
        editor.record_path = options.record;
        if let Some(id) = options.selected {
            editor.selected.insert(id);
        }
        if options.select_all {
            editor.selected = project.elements.keys().copied().collect();
        }
        let mut bench = Workbench::new(editor);
        if let Some(path) = options.project_path {
            bench.tools.path = path;
        }
        bench.section_enabled = options.section.is_some();
        bench.section_height = options.section.unwrap_or(1500.0);
        bench.load_delay_ms = options.load_delay_ms;
        let mut camera = Camera::default();
        camera.fit(overlay::model_bounds(&scene, &project).unwrap_or(scene.bounds));
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
        open_bim_desktop::ui::theme::apply(&cc.egui_ctx);
        if let Some(zoom) = options.zoom {
            // Layout verification at a logical size larger than the display.
            cc.egui_ctx.set_zoom_factor(zoom);
        }
        bench.gpu = gpu;
        bench.objects = scene.instances.len();
        Ok(Self {
            bench,
            capture_dir: options.capture_dir,
            manual_capture: None,
            capture_count: 0,
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
            capture: options.capture,
            frames: 0,
            script,
            viewport: None,
            measure: options.bench.map(|path| {
                open_bim_desktop::bench::Bench::new(path, options.bench_frames.max(10))
            }),
            load_ms,
            pending: Vec::new(),
        })
    }
    fn action(&mut self, action: Action) {
        let fit = self.scene.instances.is_empty()
            || matches!(action, Action::New | Action::Example | Action::Open);
        let result = match action {
            Action::Create => self.bench.editor.create(),
            Action::Commit(label, commands) => self.bench.editor.commit(&label, &commands),
            Action::Detail(commands) => self.bench.editor.commit("Деталировка", &commands),
            Action::Connection(commands) => self.bench.editor.commit("Соединение", &commands),
            Action::Edit(edit) => self.bench.editor.edit(&edit),
            Action::Undo => self.bench.editor.history(false),
            Action::Redo => self.bench.editor.history(true),
            Action::New => open_bim_desktop::interaction::tools::default_project()
                .and_then(|p| self.bench.editor.replace(p, "Новый проект")),
            Action::Example => example_steel_frame()
                .map_err(|e| e.to_string())
                .and_then(|p| self.bench.editor.replace(p, "Контрольный каркас")),
            Action::Open => {
                self.bench.start_open(PathBuf::from(&self.bench.tools.path));
                return;
            }
            Action::ExportIfc => {
                let path = std::path::Path::new(&self.bench.tools.path).with_extension("ifc");
                self.bench.tools.message =
                    Some(self.bench.editor.export_ifc(&path).unwrap_or_else(|e| e));
                return;
            }
            Action::ImportIfc => {
                let path = std::path::Path::new(&self.bench.tools.path).with_extension("ifc");
                match self.bench.editor.import_ifc(&path) {
                    Ok((scene, report)) => {
                        self.camera.fit(
                            overlay::model_bounds(&scene, self.bench.editor.document.project())
                                .unwrap_or(scene.bounds),
                        );
                        self.bench.objects = scene.instances.len();
                        self.scene = scene;
                        self.bench.tools.message = Some(report);
                    }
                    Err(error) => self.bench.tools.message = Some(error),
                }
                return;
            }
            Action::Save => {
                match self
                    .bench
                    .editor
                    .save(std::path::Path::new(&self.bench.tools.path))
                {
                    Ok(()) => self.bench.tools.message = Some("Проект сохранён".into()),
                    Err(error) => self.bench.tools.message = Some(error),
                };
                return;
            }
        };
        match result {
            Ok(scene) => {
                if fit {
                    self.camera.fit(
                        overlay::model_bounds(&scene, self.bench.editor.document.project())
                            .unwrap_or(scene.bounds),
                    );
                }
                self.bench.objects = scene.instances.len();
                self.scene = scene;
                self.bench.tools.message = Some(format!(
                    "Готово · ревизия {}",
                    self.bench.editor.document.project().revision
                ));
            }
            Err(error) => self.bench.tools.message = Some(error),
        }
    }
    /// Zoom to the model extent including the grid (Tekla «Вписать»).
    fn fit(&mut self) {
        let bounds = overlay::model_bounds(&self.scene, self.bench.editor.document.project())
            .unwrap_or(self.scene.bounds);
        self.camera.fit(bounds);
    }

    fn handle(&mut self, requests: Vec<Request>) {
        for request in requests {
            match request {
                Request::Action(action) => self.action(action),
                Request::Fit => self.fit(),
                Request::ToggleProjection => self.camera.orthographic = !self.camera.orthographic,
                Request::View(view) => self.camera.set_view(view),
                Request::Loaded(scene) => {
                    self.bench.objects = scene.instances.len();
                    self.scene = scene;
                    self.fit();
                }
            }
        }
    }
    fn benchmark(&mut self, ctx: &egui::Context) {
        use open_bim_desktop::bench::{Results, peak_rss_mb, percentile};
        let Some(bench) = &mut self.measure else {
            return;
        };
        ctx.request_repaint();
        // Moving camera so every measured frame redraws the whole scene.
        self.camera.orbit(0.6, 0.);
        if !bench.tick() {
            return;
        }
        let mut frames: Vec<f64> = bench
            .intervals
            .iter()
            .map(|d| d.as_secs_f64() * 1000.)
            .collect();
        let mut picks = Vec::new();
        let size = ctx.content_rect().size();
        let aspect = (size.x / size.y.max(1.)) as f64;
        for i in 0..200 {
            let uv = [
                0.05 + 0.9 * ((i * 37) % 200) as f64 / 200.,
                0.05 + 0.9 * ((i * 61) % 200) as f64 / 200.,
            ];
            let started = std::time::Instant::now();
            if let Some(ray) = self.camera.ray(self.scene.origin, uv, aspect) {
                let _ = self.scene.pick(ray, &self.view);
            }
            picks.push(started.elapsed().as_secs_f64() * 1000.);
        }
        let results = Results {
            objects: self.scene.instances.len(),
            window_points: [size.x, size.y],
            pixels_per_point: ctx.pixels_per_point(),
            gpu: self.bench.gpu.clone(),
            load_ms: self.load_ms,
            warmup_frames: bench.warmup,
            frames: frames.len(),
            frame_ms_p50: percentile(&mut frames, 0.5),
            frame_ms_p95: percentile(&mut frames, 0.95),
            frame_ms_max: percentile(&mut frames, 1.0),
            picks: picks.len(),
            pick_ms_p95: percentile(&mut picks, 0.95),
            pick_ms_max: percentile(&mut picks, 1.0),
            peak_rss_mb: peak_rss_mb(),
        };
        match serde_json::to_string_pretty(&results)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(&bench.path, json).map_err(|e| e.to_string()))
        {
            Ok(()) => eprintln!("BENCH_SAVED: {}", bench.path.display()),
            Err(e) => eprintln!("BENCH_ERROR: {e}"),
        }
        self.measure = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
                        eprintln!("CAPTURE_SAVED: {path} {}x{}", image.size[0], image.size[1]);
                        if let Some(runner) = &mut self.script {
                            runner.captured();
                        }
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
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        use open_bim_desktop::script::Effect;
        let (camera, origin, viewport) = (&self.camera, self.scene.origin, self.viewport);
        let project = |w: [f64; 3]| {
            let rect = viewport?;
            let uv = camera.project(origin, DVec3::from_array(w), rect.aspect_ratio() as f64)?;
            Some([
                rect.min.x + uv[0] as f32 * rect.width(),
                rect.min.y + uv[1] as f32 * rect.height(),
            ])
        };
        let Some(runner) = &mut self.script else {
            return;
        };
        runner.set_busy(self.bench.busy());
        match runner.frame(raw_input, &project) {
            Some(Effect::Capture(path)) => {
                self.manual_capture = Some(path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            Some(Effect::Quit) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }
        if runner.finished() {
            eprintln!("SCRIPT: finished");
            self.script = None;
        }
        ctx.request_repaint();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.screenshot(&ctx);
        self.benchmark(&ctx);
        let mut requests = self.bench.panels(ui);
        requests.append(&mut self.pending);
        self.handle(requests);
        self.view.hidden = self.bench.hidden.clone();
        self.view.section = self.bench.section_enabled.then_some(ClipPlane {
            normal: DVec3::Z,
            offset: self.bench.section_height,
        });
        self.view.selected_ids = self.bench.editor.selected.clone();
        self.view.selected = self.bench.editor.selected.first().copied();
        egui::CentralPanel::default().show(ui, |ui| {
            if self.bench.central(ui) {
                return;
            }
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
            self.viewport = Some(rect);
            let viewport_hovered = response.hovered();
            let mut fit_home = false;
            let middle_drag = response.dragged_by(egui::PointerButton::Middle);
            let hover = response.hover_pos().and_then(|point| {
                let uv = [
                    ((point.x - rect.min.x) / rect.width()) as f64,
                    ((point.y - rect.min.y) / rect.height()) as f64,
                ];
                self.camera
                    .ray(self.scene.origin, uv, rect.aspect_ratio() as f64)
            });
            if viewport_hovered {
                ctx.input(|input| {
                    // Tekla keys: arrows move the view, Ctrl+arrows rotate 15°,
                    // Shift+arrows 5°, Page Up/Down zoom, Home fits the model.
                    let arrow = |k| input.key_pressed(k);
                    let (dx, dy) = (
                        arrow(egui::Key::ArrowRight) as i32 - arrow(egui::Key::ArrowLeft) as i32,
                        arrow(egui::Key::ArrowDown) as i32 - arrow(egui::Key::ArrowUp) as i32,
                    );
                    if dx != 0 || dy != 0 {
                        // `orbit` turns 0.008 rad per pixel.
                        let step = |deg: f64| deg.to_radians() / 0.008;
                        if input.modifiers.ctrl {
                            self.camera
                                .orbit(dx as f64 * step(15.), dy as f64 * step(15.));
                        } else if input.modifiers.shift {
                            self.camera
                                .orbit(dx as f64 * step(5.), dy as f64 * step(5.));
                        } else if !input.modifiers.command {
                            self.camera.pan(
                                -dx as f64 * 40.,
                                -dy as f64 * 40.,
                                rect.height() as f64,
                            );
                        }
                    }
                    if input.key_pressed(egui::Key::PageUp) {
                        self.camera.zoom_at(100.0, None);
                    }
                    if input.key_pressed(egui::Key::PageDown) {
                        self.camera.zoom_at(-100.0, None);
                    }
                    if input.key_pressed(egui::Key::Home) {
                        fit_home = true;
                    }
                    // Tekla mouse: wheel zooms about the cursor, middle drag pans,
                    // Ctrl + middle drag rotates; the right button opens the context menu.
                    if viewport_hovered && input.smooth_scroll_delta.y != 0.0 {
                        self.camera
                            .zoom_at(input.smooth_scroll_delta.y as f64, hover);
                    }
                    let delta = input.pointer.delta();
                    if middle_drag && (input.modifiers.ctrl || input.modifiers.command) {
                        self.camera.orbit(delta.x as f64, delta.y as f64);
                    } else if middle_drag {
                        self.camera
                            .pan(delta.x as f64, delta.y as f64, rect.height() as f64);
                    }
                });
            }
            if fit_home {
                self.fit();
            }
            let cursor = if self.bench.picking() {
                hover.and_then(|_| {
                    // The elevation field moves a horizontal work plane.
                    if let Ok(z) = number(&self.bench.tools.workplane)
                        && self.bench.engine.workplane.axes[2] == DVec3::Z
                    {
                        self.bench.engine.workplane.origin.z = z;
                    }
                    let px = response.hover_pos()? - rect.min;
                    let view = open_bim_desktop::interaction::snap::View {
                        camera: &self.camera,
                        origin: self.scene.origin,
                        size: [rect.width() as f64, rect.height() as f64],
                    };
                    let mut switches = self.bench.switches;
                    if !self.bench.tools.snap_enabled {
                        switches = open_bim_desktop::interaction::snap::Switches {
                            reference: false,
                            end: false,
                            mid: false,
                            intersection: false,
                            perpendicular: false,
                            nearest: false,
                            ..switches
                        };
                    }
                    let hit = self.bench.snapper.snap(
                        self.bench.editor.document.project(),
                        &self.scene,
                        view,
                        [px.x as f64, px.y as f64],
                        switches,
                        self.bench.snap_tolerance_px,
                        self.bench.engine.picks.last().copied(),
                        &self.bench.engine.workplane,
                    )?;
                    Some((open_bim_model::Point3(hit.point.to_array()), hit))
                })
            } else {
                None
            };
            self.bench.cursor = cursor.map(|(p, _)| DVec3::from_array(p.0));
            self.bench.last_snap = cursor.map(|(_, hit)| hit.kind);
            // Tekla right button: context menu (interrupt, repeat, properties).
            let mut menu_cmd = None;
            response.context_menu(|ui| {
                use open_bim_desktop::ui::commands::{Cmd, caption};
                for cmd in [
                    Cmd::CancelTool,
                    Cmd::FinishPicks,
                    Cmd::PanelProperties,
                    Cmd::Fit,
                    Cmd::HideSelected,
                    Cmd::ShowAll,
                ] {
                    if ui.button(caption(cmd)).clicked() {
                        menu_cmd = Some(cmd);
                        ui.close();
                    }
                }
            });
            if let Some(cmd) = menu_cmd
                && let Some(r) = self.bench.execute(cmd)
            {
                self.pending.push(r);
            }
            if response.middle_clicked()
                && (self.bench.engine.active() || self.bench.detail.tool.is_some())
                && let Some(r) = self
                    .bench
                    .execute(open_bim_desktop::ui::commands::Cmd::FinishPicks)
            {
                self.pending.push(r);
            }
            if response.clicked() {
                if self.bench.picks_objects() {
                    let id = hover.and_then(|ray| self.scene.pick(ray, &self.view));
                    if let Some(r) = self.bench.pick_object(id) {
                        self.pending.push(r);
                    }
                    self.view.selected_ids = self.bench.editor.selected.clone();
                } else if self.bench.picking() {
                    if let Some((point, _)) = cursor
                        && let Some(r) = self.bench.pick(DVec3::from_array(point.0))
                    {
                        self.pending.push(r);
                    }
                } else {
                    let id = hover.and_then(|ray| self.scene.pick(ray, &self.view));
                    let extend = self.bench.extend_selection
                        || ctx.input(|i| i.modifiers.shift || i.modifiers.command);
                    self.bench.select_picked(id, extend);
                    self.view.selected_ids = self.bench.editor.selected.clone();
                    self.view.selected = self.bench.editor.selected.first().copied();
                }
            }
            let light = self.bench.light_theme;
            open_bim_desktop::viewport::gradient(
                ui.painter(),
                rect,
                if light {
                    overlay::GRADIENT_LIGHT
                } else {
                    overlay::GRADIENT_DARK
                },
            );
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    scene: self.scene.clone(),
                    camera: self.camera.clone(),
                    state: self.view.clone(),
                    size: [rect.width(), rect.height()],
                },
            ));
            let projector = overlay::Projector {
                camera: &self.camera,
                origin: self.scene.origin,
                min: [rect.min.x as f64, rect.min.y as f64],
                size: [rect.width() as f64, rect.height() as f64],
            };
            let workplane = &self.bench.engine.workplane;
            let picks: Vec<DVec3> = match &self.bench.workplane_picks {
                Some(p) => p.clone(),
                None => self.bench.engine.picks.clone(),
            };
            let polygon = self.bench.engine.tool.is_some_and(|k| {
                matches!(
                    open_bim_desktop::interaction::picks(k),
                    open_bim_desktop::interaction::Picks::AtLeast(_)
                )
            });
            let marks = overlay::build(
                &projector,
                &overlay::Input {
                    project: self.bench.editor.document.project(),
                    hidden: &self.view.hidden,
                    selected: &self.bench.editor.selected,
                    workplane: (workplane.origin, workplane.axes),
                    work_area: overlay::model_bounds(
                        &self.scene,
                        self.bench.editor.document.project(),
                    )
                    .filter(|_| self.bench.work_area)
                    .map(|b| overlay::work_area(b, WORK_AREA_MARGIN_MM)),
                    picks: &picks,
                    polygon,
                    cursor: cursor.map(|(p, hit)| {
                        (
                            DVec3::from_array(p.0),
                            open_bim_desktop::viewport::glyph(hit.kind),
                        )
                    }),
                    light,
                },
            );
            open_bim_desktop::viewport::paint(ui.painter(), &marks);
            if let Some((point, hit)) = cursor {
                ui.painter().text(
                    rect.left_bottom() - egui::vec2(-12.0, 12.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!(
                        "{}: {:.1}, {:.1}, {:.1} мм",
                        hit.kind.label(),
                        point.0[0],
                        point.0[1],
                        point.0[2]
                    ),
                    egui::FontId::monospace(13.0),
                    if light {
                        egui::Color32::from_rgb(20, 40, 70)
                    } else {
                        egui::Color32::YELLOW
                    },
                );
            }
            if self.scene.instances.is_empty()
                && self.bench.editor.document.project().axes.is_empty()
            {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Проект пуст\nОткройте контрольный каркас",
                    egui::FontId::proportional(20.0),
                    if light {
                        egui::Color32::from_rgb(40, 55, 80)
                    } else {
                        egui::Color32::LIGHT_GRAY
                    },
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
                if light {
                    egui::Color32::from_rgb(40, 55, 80)
                } else {
                    egui::Color32::GRAY
                },
            );
        });
    }
}
fn main() -> eframe::Result {
    let options = Options::parse().map_err(|e| eframe::Error::AppCreation(e.into()))?;
    let size = options.size.unwrap_or([1280.0, 800.0]);
    eframe::run_native(
        "Open BIM",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(size)
                .with_min_inner_size([1000.0, 600.0]),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(Desktop::new(cc, options)?))),
    )
}
