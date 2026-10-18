//! Application shell shared by the native app and headless UI tests: menus,
//! panels, command registry, palette, help and background jobs. The 3D
//! viewport (GPU) is drawn by the binary inside the central panel.
pub mod chrome;
pub mod commands;
pub mod icons;
pub mod jobs;
pub mod palette;
pub mod theme;

use crate::drawing_editor::{DrawingEditor, ExportFormat};
use crate::reports_ui::ReportsUi;
use crate::tools::{
    Editor, ToolKind,
    ui::{Action, ToolsUi},
};
use commands::{Cmd, caption};
use eframe::egui;
use jobs::{Job, Poll};
use open_bim_model::{ObjectId, Project, SheetSize};
use open_bim_renderer::{NamedView, Scene};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Model,
    Drawings,
    Reports,
}

/// Work the binary performs (scene, camera, GPU-side state).
pub enum Request {
    Action(Action),
    Fit,
    View(NamedView),
    /// A project finished loading in the background.
    Loaded(Arc<Scene>),
    ToggleProjection,
}

pub enum ActiveJob {
    Open(Job<(Project, Arc<Scene>)>, PathBuf),
    Check(Job<(open_bim_numbering::Numbering, open_bim_coordination::Report)>),
    Release(Job<open_bim_reports::release::ReleaseSummary>),
}

impl ActiveJob {
    fn job_label(&self) -> (&str, &jobs::Progress, std::time::Instant) {
        match self {
            Self::Open(j, _) => (&j.label, &j.progress, j.started),
            Self::Check(j) => (&j.label, &j.progress, j.started),
            Self::Release(j) => (&j.label, &j.progress, j.started),
        }
    }
    fn cancel(&self) {
        match self {
            Self::Open(j, _) => j.cancel(),
            Self::Check(j) => j.cancel(),
            Self::Release(j) => j.cancel(),
        }
    }
}

/// Cached model tree labels shared with the virtualised list.
type TreeRows = Arc<Vec<(ObjectId, String)>>;

pub struct Workbench {
    pub mode: Mode,
    pub editor: Editor,
    pub tools: ToolsUi,
    pub drawings: DrawingEditor,
    pub reports: ReportsUi,
    pub extend_selection: bool,
    pub section_enabled: bool,
    pub section_height: f64,
    pub hidden: BTreeSet<ObjectId>,
    pub palette: palette::Palette,
    pub help: bool,
    pub job: Option<ActiveJob>,
    /// Scene object count and renderer label for the status bar.
    pub objects: usize,
    pub gpu: String,
    /// Delay added to background project loading (verification of cancel).
    pub load_delay_ms: u64,
    /// Ribbon tab and the side panel opened from the side pane strip.
    pub ribbon_tab: chrome::RibbonTab,
    pub side_panel: Option<chrome::SidePanel>,
    /// Light (Tekla-like) or dark theme; applied on the next frame.
    pub light_theme: bool,
    pub theme_applied: Option<bool>,
    pub quick_launch: String,
    pub search: String,
    /// Green dashed work area box around the model (Вид tab).
    pub work_area: bool,
    /// Picks for «рабочая плоскость по трём точкам».
    pub workplane_picks: Option<Vec<open_bim_renderer::DVec3>>,
    /// Selection switches (bottom toolbar).
    pub select_parts: bool,
    pub select_assemblies: bool,
    /// Snap kind under the cursor (status bar).
    pub last_snap: Option<crate::interaction::snap::SnapKind>,
    /// Screen-space snapping and its switches (bottom toolbar).
    pub snapper: crate::interaction::snap::Snapper,
    pub switches: crate::interaction::snap::Switches,
    pub snap_tolerance_px: f64,
    /// Pick-command engine (Tekla-style creation by points).
    pub engine: crate::interaction::Engine,
    /// World point under the cursor on the work plane (set by the viewport).
    pub cursor: Option<open_bim_renderer::DVec3>,
    /// Model tree rows keyed by (journal length, revision).
    tree: ((usize, u64), TreeRows),
}

impl Workbench {
    pub fn new(editor: Editor) -> Self {
        Self {
            mode: Mode::Model,
            editor,
            tools: ToolsUi::default(),
            drawings: DrawingEditor::new(),
            reports: ReportsUi::default(),
            extend_selection: false,
            section_enabled: false,
            section_height: 1500.,
            hidden: BTreeSet::new(),
            palette: Default::default(),
            help: false,
            job: None,
            objects: 0,
            gpu: String::new(),
            load_delay_ms: 0,
            tree: ((usize::MAX, 0), Arc::new(Vec::new())),
            engine: Default::default(),
            ribbon_tab: Default::default(),
            side_panel: Some(chrome::SidePanel::Properties),
            light_theme: true,
            work_area: true,
            theme_applied: None,
            quick_launch: String::new(),
            search: String::new(),
            workplane_picks: None,
            select_parts: true,
            select_assemblies: false,
            last_snap: None,
            snapper: Default::default(),
            switches: Default::default(),
            snap_tolerance_px: 12.,
            cursor: None,
        }
    }

    fn message(&mut self, text: impl Into<String>) {
        self.tools.message = Some(text.into());
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Start loading a project on a worker thread (scene meshes included).
    pub fn start_open(&mut self, path: PathBuf) {
        let delay = self.load_delay_ms;
        let target = path.clone();
        let job = Job::spawn(
            format!("Открытие {}", path.display()),
            move |progress| {
                progress.set(0, 2);
                for _ in 0..delay / 50 {
                    if progress.cancelled() {
                        return Err("Отменено".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let project = open_bim_persistence::load(&target).map_err(|e| e.to_string())?;
                progress.set(1, 2);
                if progress.cancelled() {
                    return Err("Отменено".into());
                }
                let scene = Scene::from_project(&project).map_err(|e| e.to_string())?;
                progress.set(2, 2);
                Ok((project, Arc::new(scene)))
            },
        );
        self.job = Some(ActiveJob::Open(job, path));
    }

    pub fn start_check(&mut self) {
        let project = self.editor.document.project().clone();
        self.job = Some(ActiveJob::Check(Job::spawn(
            "Нумерация и проверка коллизий",
            move |_| crate::tools::coordination_ui::CoordinationUi::compute(&project),
        )));
    }

    pub fn start_release(&mut self) {
        let commands =
            match open_bim_reports::release::drawing_commands(self.editor.document.project()) {
                Ok(c) => c,
                Err(e) => return self.message(e),
            };
        if !commands.is_empty()
            && let Err(e) = self.editor.commit_data("Комплект чертежей", &commands)
        {
            return self.message(e);
        }
        let project = self.editor.document.project().clone();
        let directory = PathBuf::from(&self.reports.directory);
        self.job = Some(ActiveJob::Release(Job::spawn(
            "Выпуск комплекта",
            move |progress| {
                open_bim_reports::release::export_with(&project, &directory, &|done, total| {
                    progress.set(done, total);
                    !progress.cancelled()
                })
            },
        )));
    }

    pub fn cancel_job(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
            let label = job.job_label().0.to_string();
            self.editor.record(&format!("Отмена: {label}"));
            self.message(format!("Операция отменена: {label}. Модель не изменена."));
        } else {
            self.message("Нет выполняемой операции");
        }
    }

    /// Finish a completed job; returns a request for the binary if needed.
    pub fn poll_job(&mut self) -> Option<Request> {
        let job = self.job.as_ref()?;
        let finished = match job {
            ActiveJob::Open(j, _) => !matches!(j.poll(), Poll::Running),
            ActiveJob::Check(j) => !matches!(j.poll(), Poll::Running),
            ActiveJob::Release(j) => !matches!(j.poll(), Poll::Running),
        };
        if !finished {
            return None;
        }
        match self.job.take()? {
            ActiveJob::Open(j, path) => match j.wait() {
                Ok((project, scene)) => {
                    if let Err(e) = self
                        .editor
                        .adopt(project, &format!("Открыть {}", path.display()))
                    {
                        self.message(format!("Ошибка файла: {e}"));
                        return None;
                    }
                    self.drawings = DrawingEditor::new();
                    self.reports.reports = None;
                    self.hidden.clear();
                    self.message(format!("Открыт {}", path.display()));
                    return Some(Request::Loaded(scene));
                }
                Err(e) => {
                    self.editor
                        .record(&format!("Ошибка файла {}: {e}", path.display()));
                    self.message(format!("Ошибка файла: {e}. Текущая модель не изменена."));
                }
            },
            ActiveJob::Check(j) => match j.wait() {
                Ok((numbering, report)) => {
                    self.tools.coordination.accept(numbering, report);
                    self.editor.record("Нумерация и проверка коллизий");
                }
                Err(e) => self.tools.coordination.message = Some(e),
            },
            ActiveJob::Release(j) => match j.wait() {
                Ok(summary) => {
                    self.editor.record(&format!(
                        "Выпуск комплекта: {} чертежей, {} файлов",
                        summary.drawings.len(),
                        summary.files.len()
                    ));
                    self.reports.message = Some(format!(
                        "Выпущено чертежей: {}, файлов: {}",
                        summary.drawings.len(),
                        summary.files.len()
                    ));
                    let _ = self.reports.refresh(&mut self.editor);
                }
                Err(e) => self.reports.message = Some(e),
            },
        }
        None
    }

    /// Run a command; model-changing commands are refused while a job runs.
    pub fn execute(&mut self, cmd: Cmd) -> Option<Request> {
        let neutral = matches!(
            cmd,
            Cmd::CancelJob
                | Cmd::Palette
                | Cmd::Help
                | Cmd::ModeModel
                | Cmd::ModeDrawings
                | Cmd::ModeReports
        );
        if self.busy() && !neutral && cmd != Cmd::CancelTool {
            self.message("Дождитесь завершения или отмените операцию (Esc)");
            return None;
        }
        let tool = |kind| (Mode::Model, kind);
        let report = |r: Result<(), String>, this: &mut Self| {
            if let Err(e) = r {
                this.message(e);
            }
        };
        match cmd {
            Cmd::NewProject => return Some(Request::Action(Action::New)),
            Cmd::OpenProject => self.start_open(PathBuf::from(&self.tools.path)),
            Cmd::SaveProject => return Some(Request::Action(Action::Save)),
            Cmd::ExampleFrame => return Some(Request::Action(Action::Example)),
            Cmd::Undo => return Some(Request::Action(Action::Undo)),
            Cmd::Redo => return Some(Request::Action(Action::Redo)),
            Cmd::ExportIfc => return Some(Request::Action(Action::ExportIfc)),
            Cmd::ImportIfc => return Some(Request::Action(Action::ImportIfc)),
            Cmd::ModeModel => self.mode = Mode::Model,
            Cmd::ModeDrawings => self.mode = Mode::Drawings,
            Cmd::ModeReports => self.mode = Mode::Reports,
            Cmd::Palette => self.palette.toggle(),
            Cmd::Help => self.help = !self.help,
            Cmd::Fit => return Some(Request::Fit),
            Cmd::ViewIso => return Some(Request::View(NamedView::Isometric)),
            Cmd::ViewFront => return Some(Request::View(NamedView::Front)),
            Cmd::ViewTop => return Some(Request::View(NamedView::Top)),
            Cmd::ViewRight => return Some(Request::View(NamedView::Right)),
            Cmd::ToolAxis
            | Cmd::ToolBeam
            | Cmd::ToolColumn
            | Cmd::ToolPlate
            | Cmd::ToolGusset
            | Cmd::ToolStiffener => {
                let (mode, kind) = tool(match cmd {
                    Cmd::ToolAxis => ToolKind::Axis,
                    Cmd::ToolBeam => ToolKind::Beam,
                    Cmd::ToolColumn => ToolKind::Column,
                    Cmd::ToolPlate => ToolKind::Plate,
                    Cmd::ToolGusset => ToolKind::Gusset,
                    _ => ToolKind::Stiffener,
                });
                self.mode = mode;
                self.editor.begin(kind);
                self.engine.start(kind);
                self.message(self.engine.prompt());
            }
            Cmd::CreateElement => return Some(Request::Action(Action::Create)),
            Cmd::CancelTool => {
                if self.busy() {
                    self.cancel_job();
                } else if self.palette.open {
                    self.palette.open = false;
                } else if self.workplane_picks.is_some() {
                    self.workplane_picks = None;
                    self.message("Задание рабочей плоскости прервано");
                } else if self.engine.numeric.is_some() {
                    self.engine.numeric = None;
                } else {
                    self.engine.cancel();
                    self.editor.cancel();
                    self.message("Команда прервана");
                }
            }
            Cmd::PanelProperties
            | Cmd::PanelEdit
            | Cmd::PanelDetailing
            | Cmd::PanelConnections
            | Cmd::PanelClash
            | Cmd::PanelModelTree => {
                let panel = match cmd {
                    Cmd::PanelProperties => chrome::SidePanel::Properties,
                    Cmd::PanelEdit => chrome::SidePanel::Edit,
                    Cmd::PanelDetailing => chrome::SidePanel::Detailing,
                    Cmd::PanelConnections => chrome::SidePanel::Connections,
                    Cmd::PanelClash => chrome::SidePanel::Clash,
                    _ => chrome::SidePanel::Tree,
                };
                self.mode = Mode::Model;
                self.side_panel = if self.side_panel == Some(panel) {
                    None
                } else {
                    Some(panel)
                };
                self.message(format!("Панель: {}", panel.title()));
            }
            Cmd::DeleteSelected => {
                let project = self.editor.document.project();
                let ids: Vec<ObjectId> = self
                    .editor
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| project.elements.contains_key(id) || project.axes.contains_key(id))
                    .collect();
                if ids.is_empty() {
                    self.message("Ничего не выбрано");
                } else {
                    let commands = ids
                        .iter()
                        .map(|id| open_bim_commands::Command::Delete {
                            id: *id,
                            policy: open_bim_commands::DeletePolicy::Cascade,
                        })
                        .collect();
                    self.message(format!("Удалено объектов: {}", ids.len()));
                    return Some(Request::Action(Action::Commit("Удалить".into(), commands)));
                }
            }
            Cmd::ToggleProjection => return Some(Request::ToggleProjection),
            Cmd::ShowAll => {
                self.hidden.clear();
                self.message("Все объекты показаны");
            }
            Cmd::HideSelected => {
                self.hidden.extend(self.editor.selected.iter().copied());
                self.message(format!("Скрыто объектов: {}", self.editor.selected.len()));
            }
            Cmd::ToggleSection => {
                self.section_enabled = !self.section_enabled;
                self.message(if self.section_enabled {
                    "Сечение включено"
                } else {
                    "Сечение выключено"
                });
            }
            Cmd::ToggleWorkArea => {
                self.work_area = !self.work_area;
                self.message(if self.work_area {
                    "Рабочая область показана"
                } else {
                    "Рабочая область скрыта"
                });
            }
            Cmd::ToggleTheme => {
                self.light_theme = !self.light_theme;
                self.message(if self.light_theme {
                    "Светлая тема"
                } else {
                    "Тёмная тема"
                });
            }
            Cmd::WorkPlaneThreePoints => {
                self.mode = Mode::Model;
                self.engine.cancel();
                self.workplane_picks = Some(Vec::new());
                self.message(self.workplane_prompt());
            }
            Cmd::WorkPlaneReset => {
                self.engine.workplane = crate::interaction::Workplane::default();
                self.tools.workplane = "0".into();
                self.message("Рабочая плоскость: глобальная XY");
            }
            Cmd::ToggleOrtho => {
                self.switches.ortho = !self.switches.ortho;
                self.message(if self.switches.ortho {
                    "Ортогонально: включено"
                } else {
                    "Ортогонально: выключено"
                });
            }
            Cmd::FinishPicks => {
                let outcome = self.engine.finish();
                if let Some(kind) = self.engine.tool
                    && !self.editor.active
                {
                    self.editor.begin(kind);
                }
                return self.outcome(outcome);
            }
            Cmd::EditProperty => {
                self.mode = Mode::Model;
                self.tools.focus_property = true;
            }
            Cmd::ApplyProperty => match self.tools.property_action() {
                Ok(action) => return Some(Request::Action(action)),
                Err(e) => self.message(e),
            },
            Cmd::NumberAndCheck => {
                self.mode = Mode::Model;
                self.start_check();
            }
            Cmd::PartDrawing => {
                self.mode = Mode::Drawings;
                let r = self.drawings.create_part(&mut self.editor);
                report(r, self);
            }
            Cmd::AssemblyDrawing => {
                self.mode = Mode::Drawings;
                let r = self.drawings.create_assembly(&mut self.editor);
                report(r, self);
            }
            Cmd::GeneralDrawing => {
                self.mode = Mode::Drawings;
                let r = self
                    .drawings
                    .create_general(&mut self.editor, SheetSize::A1);
                report(r, self);
            }
            Cmd::UpdateDrawing => {
                let r = self.drawings.update(&mut self.editor);
                report(r, self);
            }
            Cmd::ExportPdf | Cmd::ExportDxf => {
                let format = if cmd == Cmd::ExportPdf {
                    ExportFormat::Pdf
                } else {
                    ExportFormat::Dxf
                };
                let dir = PathBuf::from(&self.drawings.export_dir);
                match self
                    .drawings
                    .export(self.editor.document.project(), format, &dir)
                {
                    Ok(path) => {
                        self.editor.record(&format!("Экспорт {}", path.display()));
                        self.message(format!("Сохранено: {}", path.display()));
                    }
                    Err(e) => self.message(e),
                }
            }
            Cmd::RefreshReports => {
                self.mode = Mode::Reports;
                let r = self.reports.refresh(&mut self.editor);
                report(r, self);
            }
            Cmd::AutoAssemblies => {
                self.mode = Mode::Reports;
                let r = self.reports.auto_assemblies(&mut self.editor).map(|_| ());
                report(r, self);
            }
            Cmd::ExportCsv => {
                let r = self.reports.export_csv(&mut self.editor).map(|_| ());
                report(r, self);
            }
            Cmd::ReleaseSet => {
                self.mode = Mode::Reports;
                self.start_release();
            }
            Cmd::CancelJob => self.cancel_job(),
        }
        None
    }

    pub fn workplane_prompt(&self) -> String {
        match self.workplane_picks.as_ref().map_or(0, Vec::len) {
            0 => "Укажите начало рабочей плоскости".into(),
            1 => "Укажите точку на оси X рабочей плоскости".into(),
            _ => "Укажите точку в плоскости XY (сторона оси Y)".into(),
        }
    }

    /// A pick from the viewport (world point, already snapped).
    pub fn pick(&mut self, point: open_bim_renderer::DVec3) -> Option<Request> {
        if let Some(picks) = &mut self.workplane_picks {
            picks.push(point);
            if picks.len() == 3 {
                let (a, b, c) = (picks[0], picks[1], picks[2]);
                self.workplane_picks = None;
                match crate::interaction::Workplane::from_points(a, b, c) {
                    Ok(wp) => {
                        self.engine.workplane = wp;
                        self.message("Рабочая плоскость задана по трём точкам");
                    }
                    Err(e) => self.message(e),
                }
            } else {
                self.message(self.workplane_prompt());
            }
            return None;
        }
        let outcome = self.engine.pick(point);
        self.outcome(outcome)
    }

    /// Typed coordinates for the next pick.
    pub fn pick_numeric(&mut self, text: &str) -> Option<Request> {
        let outcome = self.engine.pick_numeric(text, self.cursor);
        self.outcome(outcome)
    }

    fn outcome(&mut self, outcome: crate::interaction::Outcome) -> Option<Request> {
        use crate::interaction::Outcome;
        self.editor.active = self.engine.active();
        self.editor.picked = self
            .engine
            .picks
            .iter()
            .map(|p| open_bim_model::Point3(p.to_array()))
            .collect();
        match outcome {
            Outcome::Complete { kind, points } => match crate::interaction::commands(
                self.editor.document.project(),
                kind,
                &points,
                &self.engine.workplane,
                &self.editor.draft,
            ) {
                Ok(commands) => {
                    self.message(self.engine.prompt());
                    Some(Request::Action(Action::Commit(
                        format!("Создать: {}", kind.label()),
                        commands,
                    )))
                }
                Err(e) => {
                    self.message(e);
                    None
                }
            },
            Outcome::Ignored(text) => {
                self.message(text);
                None
            }
            Outcome::Continue | Outcome::Cancelled => {
                self.message(self.engine.prompt());
                None
            }
        }
    }

    /// Typing a digit, «@» or «-» during a pick opens the coordinate box.
    fn numeric_input(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.engine.active() {
            self.engine.numeric = None;
            return;
        }
        if self.engine.numeric.is_none() && !ctx.egui_wants_keyboard_input() {
            let start = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Text(t)
                        if t.starts_with(|c: char| c.is_ascii_digit() || c == '@' || c == '-') =>
                    {
                        Some(t.clone())
                    }
                    _ => None,
                })
            });
            if let Some(text) = start {
                self.engine.numeric = Some(text);
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("numeric location")));
            }
        }
        let Some(mut text) = self.engine.numeric.clone() else {
            return;
        };
        let mut submit = false;
        egui::Window::new("Ввод координат")
            .id(egui::Id::new("numeric window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0., -80.))
            .show(ctx, |ui| {
                let label =
                    ui.label("x,y,z — абсолютно · @dx,dy,dz — относительно · d — по направлению");
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut text)
                            .id(egui::Id::new("numeric location"))
                            .desired_width(320.),
                    )
                    .labelled_by(label.id);
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                if let Some(e) = &self.engine.error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
            });
        if self.engine.numeric.is_some() {
            self.engine.numeric = Some(text.clone());
        }
        if submit && let Some(r) = self.pick_numeric(&text) {
            requests.push(r);
        }
    }

    /// Top, bottom and side panels plus overlays. Call before the central panel.
    pub fn panels(&mut self, ui: &mut egui::Ui) -> Vec<Request> {
        let ctx = ui.ctx().clone();
        let mut requests = Vec::new();
        for cmd in commands::pressed(&ctx) {
            if let Some(r) = self.execute(cmd) {
                requests.push(r);
            }
        }
        if let Some(cmd) = self.palette.show(&ctx)
            && let Some(r) = self.execute(cmd)
        {
            requests.push(r);
        }
        palette::help(&ctx, &mut self.help);
        self.numeric_input(&ctx, &mut requests);
        if let Some(r) = self.poll_job() {
            requests.push(r);
        }
        if self.tools.coordination.requested {
            self.tools.coordination.requested = false;
            if let Some(r) = self.execute(Cmd::NumberAndCheck) {
                requests.push(r);
            }
        }
        self.job_window(&ctx, &mut requests);
        self.chrome(ui, &mut requests);
        requests
    }

    fn job_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let Some(job) = &self.job else { return };
        let (label, progress, started) = job.job_label();
        let (label, fraction, counts, elapsed) = (
            label.to_string(),
            progress.fraction(),
            progress.counts(),
            started.elapsed(),
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        let mut cancel = false;
        egui::Window::new("Выполняется операция")
            .id(egui::Id::new("job window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0., 0.))
            .show(ctx, |ui| {
                ui.label(&label);
                let bar = match fraction {
                    Some(f) => {
                        egui::ProgressBar::new(f).text(format!("{} из {}", counts.0, counts.1))
                    }
                    None => egui::ProgressBar::new(0.).animate(true),
                };
                ui.add(bar.desired_width(360.));
                ui.label(format!("Прошло {:.1} с", elapsed.as_secs_f32()));
                cancel = ui.button(caption(Cmd::CancelJob) + " · Esc").clicked();
            });
        if cancel {
            let _ = requests;
            self.cancel_job();
        }
    }

    /// Central panel content outside the 3D model mode; returns false in
    /// model mode so the binary draws the GPU viewport.
    pub fn central(&mut self, ui: &mut egui::Ui) -> bool {
        match self.mode {
            Mode::Drawings => {
                self.drawings.show_sheet(ui, &mut self.editor);
                true
            }
            Mode::Reports => {
                self.reports.show_table(ui);
                true
            }
            Mode::Model => false,
        }
    }
}
