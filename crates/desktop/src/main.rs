use eframe::egui;
use egui_wgpu::wgpu;
use open_bim_desktop::interaction::selection;
use open_bim_desktop::tools::{Editor, number, ui::Action};
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{ObjectId, example_steel_frame};
/// Work area box margin around the model bounds, mm.
const WORK_AREA_MARGIN_MM: f64 = 500.0;
use open_bim_renderer::{
    Camera, ClipPlane, DVec3, GpuRenderer, NamedView, Scene, ViewState, ViewportCallback, overlay,
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
    display: Option<open_bim_renderer::RenderMode>,
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
                "--display" => {
                    use open_bim_renderer::RenderMode;
                    options.display = Some(match args.next().as_deref() {
                        Some("wireframe") => RenderMode::Wireframe,
                        Some("transparent") => RenderMode::Transparent,
                        Some("shaded") => RenderMode::Shaded,
                        _ => return Err("--display: wireframe | transparent | shaded".into()),
                    })
                }
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
    /// Area selection drag start (screen points).
    area_drag: Option<[f64; 2]>,
    /// Handle being dragged (direct modification).
    handle_drag: Option<overlay::Handle>,
    /// Rotation centre of the running orbit gesture.
    orbit: open_bim_desktop::interaction::orbit::Orbit,
    /// Pointer position the snap cycle belongs to.
    last_hover: Option<egui::Pos2>,
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
        if let Some(mode) = options.display {
            bench.display.mode = mode;
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
            area_drag: None,
            handle_drag: None,
            orbit: Default::default(),
            last_hover: None,
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
                Request::ApplyView(view) => {
                    self.camera.yaw = view.yaw;
                    self.camera.pitch = view.pitch;
                    self.camera.orthographic = view.orthographic;
                    self.fit();
                }
                Request::FitSelected => {
                    let project = self.bench.editor.document.project();
                    let points: Vec<DVec3> = self
                        .bench
                        .editor
                        .selected
                        .iter()
                        .flat_map(|id| selection::corners(project, *id).unwrap_or_default())
                        .collect();
                    if let Some(first) = points.first() {
                        let (min, max) = points
                            .iter()
                            .fold((*first, *first), |(a, b), p| (a.min(*p), b.max(*p)));
                        self.camera.fit(open_bim_renderer::Bounds { min, max });
                    }
                }
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
        // Clip planes follow the whole model (parts and grid), every frame.
        self.camera.extent =
            overlay::model_bounds(&self.scene, self.bench.editor.document.project())
                .or(Some(self.scene.bounds));
        // The section tool overrides the lower plane of the view depth.
        self.view.section = if self.bench.section_enabled {
            Some(ClipPlane {
                normal: DVec3::Z,
                offset: self.bench.section_height,
            })
        } else {
            self.bench.clip[0]
        };
        self.view.depth = self.bench.clip[1];
        self.view.display = self.bench.display;
        // Copy the selection into the view only when it changed.
        if *self.view.selected_ids != self.bench.editor.selected {
            self.view.selected_ids = Arc::new(self.bench.editor.selected.clone());
        }
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
            // Keyboard cursor: Alt+arrows (taken before the arrows navigate the view).
            // Not while a field is edited or a control has keyboard focus
            // (Space then presses that control).
            let typing = ctx.egui_wants_keyboard_input() || ctx.memory(|m| m.focused().is_some());
            let kb_arrow = (!typing).then(|| {
                use open_bim_desktop::interaction::keyboard_cursor::Arrow;
                [
                    (egui::Key::ArrowLeft, Arrow::Left),
                    (egui::Key::ArrowRight, Arrow::Right),
                    (egui::Key::ArrowUp, Arrow::Up),
                    (egui::Key::ArrowDown, Arrow::Down),
                ]
                .into_iter()
                .find(|(k, _)| ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, *k)))
                .map(|(_, a)| a)
            });
            let kb_space = if typing {
                None
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::Space)) {
                Some(selection::Click::Toggle)
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                Some(selection::Click::Replace)
            } else {
                None
            };
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
                    // Trackpad: pinch zooms about the cursor, two-finger scroll
                    // rotates (Shift: pans), the rotate gesture turns about the
                    // vertical; the mouse wheel zooms.
                    let surface = || {
                        hover.and_then(|ray| {
                            self.scene
                                .pick_hit(ray, &self.view)
                                .map(|(_, t)| ray.origin + ray.direction * t)
                        })
                    };
                    let scroll =
                        open_bim_desktop::interaction::orbit::split_scroll(&input.raw.events);
                    let pinch = input.zoom_delta() as f64;
                    let now = input.time;
                    if scroll.wheel != 0.0 {
                        // Zoom towards the model surface under the cursor (Tekla).
                        self.camera.zoom_to(scroll.wheel, hover, surface());
                    }
                    if (pinch - 1.0).abs() > 1e-6 && pinch > 0.0 {
                        self.camera.zoom_to(pinch.ln() / 0.002, hover, surface());
                    }
                    if scroll.trackpad != [0.0, 0.0] {
                        let [dx, dy] = scroll.trackpad;
                        if scroll.shift {
                            self.camera.pan(dx, dy, rect.height() as f64);
                        } else {
                            let pivot = self.orbit.pivot(now, surface(), self.camera.target);
                            self.camera.orbit_about(pivot, -dx, -dy);
                        }
                    }
                    let turn = input.rotation_delta() as f64;
                    if turn != 0.0 {
                        let pivot = self.orbit.pivot(now, surface(), self.camera.target);
                        self.camera.orbit_about(pivot, turn / 0.008, 0.0);
                    }
                    let delta = input.pointer.delta();
                    if middle_drag && (input.modifiers.ctrl || input.modifiers.command) {
                        // Rotation centre: the surface under the cursor at the start.
                        let pivot = self.orbit.pivot(now, surface(), self.camera.target);
                        self.camera
                            .orbit_about(pivot, delta.x as f64, delta.y as f64);
                    } else if middle_drag {
                        self.camera
                            .pan(delta.x as f64, delta.y as f64, rect.height() as f64);
                    }
                });
            }
            if fit_home {
                self.fit();
            }
            let projector = overlay::Projector {
                camera: &self.camera,
                origin: self.scene.origin,
                min: [rect.min.x as f64, rect.min.y as f64],
                size: [rect.width() as f64, rect.height() as f64],
            };
            if let Some(arrow) = kb_arrow.flatten() {
                self.bench.keyboard_step(&projector, arrow);
            }
            if let Some(mode) = kb_space
                && let Some(r) = self.bench.keyboard_space(mode)
            {
                self.pending.push(r);
            }
            let modifiers = ctx.input(|i| i.modifiers);
            let project = self.bench.editor.document.project();
            // Snapping runs while a command picks points or a handle is dragged.
            let previous = match &self.handle_drag {
                Some(h) => selection::anchor(project, h),
                None => self
                    .bench
                    .engine
                    .reference
                    .or(self.bench.engine.picks.last().copied())
                    .or(self.bench.edit_pick.points.last().copied())
                    .or(self.bench.detail.points.last().copied())
                    .or(self.bench.trim.points.last().copied()),
            };
            let snapping = self.bench.picking() || self.handle_drag.is_some();
            let candidates = match (snapping, response.hover_pos()) {
                (true, Some(pos)) => {
                    // The elevation field moves a horizontal work plane.
                    if let Ok(z) = number(&self.bench.tools.workplane)
                        && self.bench.engine.workplane.axes[2] == DVec3::Z
                    {
                        self.bench.engine.workplane.origin.z = z;
                    }
                    let px = pos - rect.min;
                    let view = open_bim_desktop::interaction::snap::View {
                        camera: &self.camera,
                        origin: self.scene.origin,
                        size: [rect.width() as f64, rect.height() as f64],
                    };
                    let mut switches = self.bench.switches;
                    if !self.bench.tools.snap_enabled {
                        switches = open_bim_desktop::interaction::snap::Switches {
                            reference: false,
                            geometry: false,
                            end: false,
                            mid: false,
                            intersection: false,
                            perpendicular: false,
                            nearest: false,
                            ..switches
                        };
                    }
                    // Tab / Shift+Tab cycle the snap points under a resting pointer.
                    if self.last_hover.is_none_or(|l| l.distance(pos) > 3.) {
                        self.bench.snap_cycle = 0;
                        self.last_hover = Some(pos);
                    }
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)) {
                        self.bench.snap_cycle = self.bench.snap_cycle.wrapping_sub(1);
                    } else if ctx
                        .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
                    {
                        self.bench.snap_cycle = self.bench.snap_cycle.wrapping_add(1);
                    }
                    self.bench.snapper.candidates(
                        project,
                        &self.scene,
                        view,
                        [px.x as f64, px.y as f64],
                        switches,
                        self.bench.snap_tolerance_px,
                        previous,
                        &self.bench.engine.workplane,
                    )
                }
                _ => Vec::new(),
            };
            let hit = (!candidates.is_empty()).then(|| {
                let n = candidates.len();
                candidates[(self.bench.snap_cycle % n + n) % n]
            });
            let cursor = hit.map(|h| (open_bim_model::Point3(h.point.to_array()), h));
            // Without the mouse over the view the keyboard cursor is shown instead.
            let kb_mark = self
                .bench
                .kb_cursor
                .filter(|_| hit.is_none())
                .map(|p| (p, open_bim_renderer::overlay::Glyph::Reference));
            self.bench.cursor = hit.map(|h| h.point);
            self.bench.last_snap = hit.map(|h| h.kind);
            // Tekla right button: context menu.
            let mut menu_cmd = None;
            response.context_menu(|ui| {
                use open_bim_desktop::ui::commands::{Cmd, caption};
                for cmd in [
                    Cmd::CancelTool,
                    Cmd::FinishPicks,
                    Cmd::PanelProperties,
                    Cmd::MoveByPicks,
                    Cmd::CopyByPicks,
                    Cmd::DeleteSelected,
                    Cmd::HideSelected,
                    Cmd::ShowOnlySelected,
                    Cmd::ShowAll,
                    Cmd::FitSelected,
                    Cmd::PartDrawing,
                    Cmd::GridDialog,
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
            let click_mode = if modifiers.ctrl && !modifiers.mac_cmd {
                selection::Click::Toggle
            } else if modifiers.shift || modifiers.command || self.bench.extend_selection {
                selection::Click::Add
            } else {
                selection::Click::Replace
            };
            if response.clicked() {
                if self.bench.picks_objects() {
                    let id = hover.and_then(|ray| self.scene.pick(ray, &self.view));
                    if let Some(r) = self.bench.pick_object(id) {
                        self.pending.push(r);
                    }
                } else if self.bench.picking() {
                    if let Some((point, _)) = cursor {
                        let point = DVec3::from_array(point.0);
                        if modifiers.ctrl && !modifiers.mac_cmd {
                            // Tekla Ctrl+pick: temporary reference point, not a pick.
                            self.bench.engine.reference = Some(point);
                            self.bench.tools.message =
                                Some("Временная опорная точка задана".into());
                        } else if let Some(r) = self.bench.pick(point) {
                            self.pending.push(r);
                        }
                    }
                } else if let Some(view) = response
                    .interact_pointer_pos()
                    .and_then(|p| gizmo_axis_at(&self.camera, rect, p))
                {
                    // Tekla navigation control: click an axis for that plane view.
                    self.pending.push(Request::View(view));
                    self.pending.push(Request::Fit);
                } else {
                    let id = hover.and_then(|ray| self.scene.pick(ray, &self.view));
                    // Keep the contextual toolbar inside the 3D view.
                    self.bench.context_at = response.interact_pointer_pos().map(|p| {
                        egui::pos2(
                            p.x.min(rect.max.x - 520.).max(rect.min.x),
                            p.y.min(rect.max.y - 60.).max(rect.min.y),
                        )
                    });
                    // Alt+click selects the whole assembly once (Tekla).
                    let saved = self.bench.select;
                    self.bench.select.assemblies |= modifiers.alt;
                    // Level labels and level lines win over the grid line copies.
                    let axis = response.interact_pointer_pos().and_then(|p| {
                        use open_bim_desktop::interaction::grid_edit::{pick_axis, pick_level};
                        let project = self.bench.editor.document.project();
                        let at = [p.x as f64, p.y as f64];
                        open_bim_desktop::interaction::grid_edit::pick_annotation(
                            project, &projector, at, 6.,
                        )
                        .or_else(|| {
                            pick_level(project, &projector, at, 6.)
                                .filter(|_| self.bench.select.levels)
                        })
                        .or_else(|| pick_axis(project, &projector, at, 6.))
                    });
                    self.bench.select_clicked(id, axis, click_mode);
                    self.bench.select = saved;
                }
            }
            // Double click opens the properties of the clicked part (Tekla).
            // On a grid line (no part under the cursor) it opens the grid dialog.
            if response.double_clicked() && !self.bench.picking() {
                let on_grid = hover
                    .and_then(|ray| self.scene.pick(ray, &self.view))
                    .is_none()
                    && response.interact_pointer_pos().is_some_and(|p| {
                        open_bim_desktop::interaction::grid_edit::pick_axis(
                            self.bench.editor.document.project(),
                            &projector,
                            [p.x as f64, p.y as f64],
                            6.,
                        )
                        .is_some()
                    });
                if on_grid {
                    self.pending.extend(
                        self.bench
                            .execute(open_bim_desktop::ui::commands::Cmd::GridDialog),
                    );
                } else {
                    self.bench.mode = open_bim_desktop::ui::Mode::Model;
                    self.bench.side_panel =
                        Some(open_bim_desktop::ui::chrome::SidePanel::Properties);
                    self.bench.pane.source.clear();
                }
            }
            // Primary drag: a selected handle (direct modification) or an area.
            if response.drag_started_by(egui::PointerButton::Primary)
                && !self.bench.picking()
                && let Some(start) = ctx.input(|i| i.pointer.press_origin())
            {
                let start = [start.x as f64, start.y as f64];
                let handles = if self.bench.direct_modification {
                    overlay::handles(
                        self.bench.editor.document.project(),
                        &self.bench.editor.selected,
                        &projector,
                    )
                } else {
                    Vec::new()
                };
                match selection::handle_at(&handles, start, 8.) {
                    Some(h) => self.handle_drag = Some(h),
                    None => self.area_drag = Some(start),
                }
            }
            if response.drag_stopped() {
                if let Some(h) = self.handle_drag.take() {
                    match cursor.map(|(p, _)| {
                        open_bim_desktop::interaction::member_edit::drag_handle(
                            self.bench.editor.document.project(),
                            &h,
                            DVec3::from_array(p.0),
                            self.bench.pane.length_mode,
                        )
                    }) {
                        Some(Ok(commands)) => self.pending.push(Request::Action(
                            open_bim_desktop::tools::ui::Action::Commit(
                                "Переместить ручку".into(),
                                commands,
                            ),
                        )),
                        Some(Err(e)) => self.bench.tools.message = Some(e),
                        None => {}
                    }
                }
                if let (Some(from), Some(to)) =
                    (self.area_drag.take(), response.interact_pointer_pos())
                {
                    self.bench.select_area(
                        &projector,
                        from,
                        [to.x as f64, to.y as f64],
                        click_mode,
                    );
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
            let workplane = &self.bench.engine.workplane;
            let project = self.bench.editor.document.project();
            let picks: Vec<DVec3> = if let Some(m) = &self.bench.measure {
                m.clone()
            } else if let Some(h) = &self.handle_drag {
                selection::anchor(project, h).into_iter().collect()
            } else if let Some(p) = &self.bench.workplane_picks {
                p.clone()
            } else if self.bench.edit_pick.op.is_some() {
                self.bench.edit_pick.points.clone()
            } else if !self.bench.detail.points.is_empty() {
                self.bench.detail.points.clone()
            } else if !self.bench.trim.points.is_empty() {
                self.bench.trim.points.clone()
            } else {
                self.bench.engine.picks.clone()
            };
            // The reference line of the snapped object (member axis or grid line).
            let highlight = hit.and_then(|h| {
                let id = h.object?;
                if let Some(axis) = project.axes.get(&id) {
                    return Some((
                        DVec3::from_array(axis.start.0),
                        DVec3::from_array(axis.end.0),
                    ));
                }
                match &project.elements.get(&id)?.shape {
                    open_bim_model::ElementShape::Member { length, frame, .. } => Some((
                        DVec3::from_array(frame.origin.0),
                        DVec3::from_array(
                            frame
                                .world(open_bim_model::Point3([0., 0., length.value()]))
                                .0,
                        ),
                    )),
                    _ => None,
                }
            });
            let polygon = self.bench.engine.tool.is_some_and(|k| {
                matches!(
                    self.bench.engine.rule(k),
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
                    cursor: cursor
                        .map(|(p, hit)| {
                            (
                                DVec3::from_array(p.0),
                                open_bim_desktop::viewport::glyph(hit.kind),
                            )
                        })
                        .or(kb_mark),
                    plate_preview: self.bench.plate_preview(self.bench.cursor),
                    snap_label: hit.map(|h| h.kind.label().to_string()),
                    highlight,
                    reference: self.bench.engine.reference,
                    direct: self.bench.direct_modification,
                    light,
                },
            );
            open_bim_desktop::viewport::paint(ui.painter(), &marks);
            // Last measurement: dimension line with the distance.
            if let Some((a, b)) = self.bench.measured
                && let (Some(pa), Some(pb)) = (projector.screen(a), projector.screen(b))
            {
                let (pa, pb) = (
                    egui::pos2(pa[0] as f32, pa[1] as f32),
                    egui::pos2(pb[0] as f32, pb[1] as f32),
                );
                let ink = if light {
                    egui::Color32::BLACK
                } else {
                    egui::Color32::WHITE
                };
                ui.painter()
                    .line_segment([pa, pb], egui::Stroke::new(1.5, ink));
                for p in [pa, pb] {
                    ui.painter().circle_filled(p, 3., ink);
                }
                ui.painter().text(
                    pa.lerp(pb, 0.5) - egui::vec2(0., 10.),
                    egui::Align2::CENTER_BOTTOM,
                    format!("{:.1} мм", a.distance(b)),
                    egui::FontId::proportional(13.),
                    ink,
                );
            }
            // Rotation centre while an orbit gesture runs.
            if let Some(pivot) = self.orbit.active(ctx.input(|i| i.time))
                && let Some(c) = projector.screen(pivot)
            {
                let c = egui::pos2(c[0] as f32, c[1] as f32);
                let ink = egui::Stroke::new(2., egui::Color32::from_rgb(230, 120, 0));
                ui.painter().circle_stroke(c, 7., ink);
                ui.painter()
                    .line_segment([c - egui::vec2(11., 0.), c + egui::vec2(11., 0.)], ink);
                ui.painter()
                    .line_segment([c - egui::vec2(0., 11.), c + egui::vec2(0., 11.)], ink);
                ctx.request_repaint();
            }
            // Length of selected beams and columns at their midpoints (up to 20).
            {
                let project = self.bench.editor.document.project();
                for id in self.bench.editor.selected.iter().take(20) {
                    let Some(e) = project.elements.get(id) else {
                        continue;
                    };
                    let Some((a, b)) = open_bim_desktop::interaction::member_edit::ends(&e.shape)
                    else {
                        continue;
                    };
                    let Some(m) = projector.screen((a + b) * 0.5) else {
                        continue;
                    };
                    let text = format!(
                        "L = {}",
                        open_bim_desktop::interaction::member_edit::length_text(a.distance(b))
                    );
                    let at = egui::pos2(m[0] as f32, m[1] as f32 - 12.);
                    let font = egui::FontId::proportional(13.);
                    let galley = ui.painter().layout_no_wrap(
                        text,
                        font,
                        egui::Color32::from_rgb(20, 60, 130),
                    );
                    let rect = egui::Align2::CENTER_BOTTOM
                        .anchor_size(at, galley.size())
                        .expand(3.);
                    ui.painter()
                        .rect_filled(rect, 3., egui::Color32::from_white_alpha(225));
                    ui.painter().galley(
                        rect.min + egui::vec2(3., 3.),
                        galley,
                        egui::Color32::BLACK,
                    );
                }
            }
            // Area selection rectangle: solid = window (left→right), dashed = crossing.
            if let (Some(from), Some(to)) = (self.area_drag, response.interact_pointer_pos()) {
                let from = egui::pos2(from[0] as f32, from[1] as f32);
                let r = egui::Rect::from_two_pos(from, to);
                let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(30, 90, 168));
                if to.x >= from.x {
                    ui.painter().rect_filled(
                        r,
                        0.,
                        egui::Color32::from_rgba_unmultiplied(30, 90, 168, 30),
                    );
                    ui.painter()
                        .rect_stroke(r, 0., stroke, egui::StrokeKind::Middle);
                } else {
                    let c = [
                        r.left_top(),
                        r.right_top(),
                        r.right_bottom(),
                        r.left_bottom(),
                        r.left_top(),
                    ];
                    ui.painter()
                        .extend(egui::Shape::dashed_line(&c, stroke, 6., 4.));
                }
            }
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
/// Named view for a click on the axis gizmo labels (bottom-right corner):
/// X → right side, Y → front, Z → top.
fn gizmo_axis_at(camera: &Camera, rect: egui::Rect, p: egui::Pos2) -> Option<NamedView> {
    let centre = rect.max - egui::vec2(48., 48.);
    overlay::gizmo_directions(camera)
        .iter()
        .zip([NamedView::Right, NamedView::Front, NamedView::Top])
        .find(|(d, _)| {
            let tip = centre + egui::vec2(d[0] as f32, d[1] as f32) * 36.;
            tip.distance(p) <= 10.
        })
        .map(|(_, v)| v)
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
