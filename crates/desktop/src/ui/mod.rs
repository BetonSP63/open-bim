//! Application shell shared by the native app and headless UI tests: menus,
//! panels, command registry, palette, help and background jobs. The 3D
//! viewport (GPU) is drawn by the binary inside the central panel.
pub mod auto_connections;
pub mod catalog_picker;
pub mod chrome;
pub mod commands;
pub mod component_editor;
pub mod components;
pub mod custom_form;
pub mod icons;
pub mod jobs;
pub mod keytips;
pub mod palette;
pub mod plugins;
pub mod property_fields;
pub mod property_pane;
pub mod representation;
pub mod ribbon;
pub mod theme;
pub mod views;

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
    /// Zoom to the selected objects (Shift+Space).
    FitSelected,
    /// Switch the camera and view depth to a model view.
    ApplyView(Box<views::ModelView>),
    /// «Секущая плоскость по точке»: a clip plane through the point, facing
    /// the viewer; the part of the model in front of it is cut away.
    ClipAt(open_bim_renderer::DVec3),
}

/// Clip plane through `point` facing the eye: what lies between the eye
/// and the plane is cut away.
pub fn clip_facing(
    eye: open_bim_renderer::DVec3,
    target: open_bim_renderer::DVec3,
    point: open_bim_renderer::DVec3,
) -> open_bim_renderer::ClipPlane {
    let normal = (target - eye).normalize_or(open_bim_renderer::DVec3::NEG_Z);
    open_bim_renderer::ClipPlane {
        normal,
        offset: normal.dot(point),
    }
}

/// Keyboard focus areas cycled by F6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusArea {
    Viewport,
    Ribbon,
    QuickLaunch,
    SidePane,
    Toolbar,
    Search,
}

impl FocusArea {
    pub fn next(self) -> Self {
        match self {
            Self::Viewport => Self::Ribbon,
            Self::Ribbon => Self::QuickLaunch,
            Self::QuickLaunch => Self::SidePane,
            Self::SidePane => Self::Toolbar,
            Self::Toolbar | Self::Search => Self::Viewport,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Viewport => "3D-вид (Alt+стрелки — курсор, Пробел — указать/выбрать)",
            Self::Ribbon => "лента (Tab — кнопки, Пробел — выполнить)",
            Self::QuickLaunch => "быстрый запуск (введите команду, Enter)",
            Self::SidePane => "боковые панели (Tab — поля панели)",
            Self::Toolbar => "панель привязок и выбора (Tab — переключатели)",
            Self::Search => "поиск в модели (текст, Enter)",
        }
    }
}

pub enum ActiveJob {
    Open(Box<Job<(Project, Arc<Scene>)>>, PathBuf),
    Check(Job<(open_bim_numbering::Numbering, open_bim_coordination::Report)>),
    Release(Job<open_bim_reports::release::ReleaseSummary>),
    /// Plugin run: label, staged commands, created objects, and the undo
    /// depth the commands were built against.
    Plugin(Job<PluginRun>, (usize, usize)),
}

pub struct PluginRun {
    pub label: String,
    pub commands: Vec<open_bim_commands::Command>,
    pub created: Vec<ObjectId>,
    /// Follow-up of the last step (placed plugins whose inputs changed):
    /// added to that undo step instead of a new one.
    pub amend: bool,
    pub problems: Vec<String>,
}

impl ActiveJob {
    fn job_label(&self) -> (&str, &jobs::Progress, std::time::Instant) {
        match self {
            Self::Open(j, _) => (&j.label, &j.progress, j.started),
            Self::Check(j) => (&j.label, &j.progress, j.started),
            Self::Release(j) => (&j.label, &j.progress, j.started),
            Self::Plugin(j, _) => (&j.label, &j.progress, j.started),
        }
    }
    fn cancel(&self) {
        match self {
            Self::Open(j, _) => j.cancel(),
            Self::Check(j) => j.cancel(),
            Self::Release(j) => j.cancel(),
            Self::Plugin(j, _) => j.cancel(),
        }
    }
}

/// Cached model tree labels shared with the virtualised list.
type TreeRows = Arc<Vec<(ObjectId, String)>>;

/// Common surface treatments of steel parts; any other text is allowed.
pub const TREATMENTS: [&str; 6] = [
    "Без покрытия",
    "Грунт ГФ-021 + эмаль ПФ-115, 2 слоя",
    "Грунт ГФ-021",
    "Горячее цинкование",
    "Огнезащитное покрытие",
    "Эмаль ХВ-124 по грунту ХС-010",
];

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
    /// Name of the active view shown in the window title.
    pub view_title: &'static str,
    /// Property pane: search text and «Показать меньше свойств».
    pub property_search: String,
    pub fewer_properties: bool,
    /// Light or dark theme; applied on the next frame.
    pub light_theme: bool,
    pub theme_applied: Option<bool>,
    pub quick_launch: String,
    pub search: String,
    /// Bolt (two points) and weld (two parts) pick commands.
    pub detail: crate::interaction::tools::DetailPicker,
    /// Component catalogue (Ctrl+F) and the armed component.
    pub components: components::ComponentState,
    /// «Редактор компонентов»: the user component being made or changed.
    pub component_editor: component_editor::EditorState,
    /// Open grid dialog («Сетка осей…»).
    pub grid: Option<crate::interaction::tools::GridSpec>,
    /// Fitting, cutting and construction commands in progress.
    pub trim: crate::interaction::trim::TrimPicker,
    /// Part display mode and transparency («Отображение»).
    pub display: open_bim_renderer::Display,
    /// «Сведения о документе» being edited (main inscription attributes).
    pub document_info: Option<open_bim_model::DocumentInfo>,
    /// General notes being edited, one item per line.
    pub document_notes: String,
    /// The dialog edits the existing grid (otherwise it creates a new one).
    pub grid_editing: bool,
    /// Keyboard cursor (Alt+arrows): a snap point while picking, or the
    /// object it rests on when idle.
    pub kb_cursor: Option<open_bim_renderer::DVec3>,
    pub kb_object: Option<ObjectId>,
    /// Ribbon KeyTips state and Alt press tracking (Alt alone toggles them).
    pub keytips: keytips::KeyTips,
    pub alt_down: bool,
    pub alt_clean: bool,
    /// F6 / ⌘F: area of the window that takes keyboard focus next frame.
    pub focus_target: Option<FocusArea>,
    pub focus_area: FocusArea,
    /// Edit buffer of the selected grid line in the property pane.
    pub axis_buffer: Option<open_bim_model::GridAxis>,
    /// Level being edited in the pane: (id, name, mark text).
    pub level_buffer: Option<(ObjectId, String, String)>,
    /// Green dashed work area box around the model (Вид tab).
    pub work_area: bool,
    /// Picks for «рабочая плоскость по трём точкам».
    pub workplane_picks: Option<Vec<open_bim_renderer::DVec3>>,
    /// Selection switches (bottom toolbar).
    pub select: crate::interaction::selection::Filter,
    /// Move/copy/mirror/rotate by picked points.
    pub edit_pick: crate::interaction::selection::EditPicker,
    /// Direct modification (D): drag handles of selected parts.
    pub direct_modification: bool,
    /// Drag and drop: dragging a selected part moves the selection (Ctrl copies).
    pub drag_drop: bool,
    /// «Копировать/Переместить — специальное»: the dialog fields and which
    /// dialog is open (Some(true) copy, Some(false) move).
    pub special: crate::interaction::special::Dialog,
    pub special_open: Option<bool>,
    /// Selection filter (Ctrl+G) and whether its dialog is open.
    pub filter: crate::interaction::selection::PropertyFilter,
    pub filter_open: bool,
    /// Alt+P: the selection before the current one.
    pub previous_selection: BTreeSet<ObjectId>,
    selection_seen: BTreeSet<ObjectId>,
    /// Rollover highlight (H) and the part under the cursor.
    pub rollover: bool,
    pub hovered: Option<ObjectId>,
    /// Component symbols in the model (status colour, code).
    pub show_components: bool,
    /// Delete waiting for confirmation: ids and the total number removed.
    pub delete_confirm: Option<(BTreeSet<ObjectId>, usize)>,
    /// Tab / Shift+Tab: index into the snap candidates under the cursor.
    pub snap_cycle: usize,
    /// Property pane edit buffer.
    pub pane: property_pane::PropertyPane,
    /// «Фаски углов» of the selected plates.
    pub corners: crate::interaction::corners::CornerUi,
    /// «Обработка поверхности» typed or picked for the selected parts.
    pub treatment: String,
    /// Model views (3D, plans, elevations) and the active one.
    pub views: Vec<views::ModelView>,
    pub active_view: usize,
    /// View depth of the active view (two clip planes).
    pub clip: [Option<open_bim_renderer::ClipPlane>; 2],
    /// Contextual toolbar (Ctrl+K) and where the last selecting click was.
    pub contextual: bool,
    pub context_at: Option<egui::Pos2>,
    /// Measure command picks and the last measured segment.
    pub measure: Option<Vec<open_bim_renderer::DVec3>>,
    pub measured: Option<(open_bim_renderer::DVec3, open_bim_renderer::DVec3)>,
    /// «Измерить угол»: picks (vertex, first side, second side) and the
    /// last measured angle.
    pub angle_picks: Option<Vec<open_bim_renderer::DVec3>>,
    pub measured_angle: Option<[open_bim_renderer::DVec3; 3]>,
    /// «Запросить объект» / «Сравнить детали» result window.
    pub inquiry: Option<crate::interaction::inquire::Inquiry>,
    pub plugins: plugins::PluginsUi,
    /// Transactions and revision already checked for placed plugins.
    pub plugins_seen: (u64, u64),
    /// The next pick places the clip plane.
    pub clip_pick: bool,
    /// Object groups, the object representation and its window.
    pub groups: Vec<crate::interaction::object_groups::Group>,
    pub representation: Vec<crate::interaction::object_groups::RepresentationRow>,
    pub representation_open: bool,
    /// The group edited in the window.
    pub group_edited: usize,
    /// Bumped on every change of groups or rows; with the scene it keys
    /// the applied representation.
    pub representation_version: u64,
    applied: Option<((usize, u64), representation::Shared)>,
    /// «Контрольные номера» window.
    pub control: crate::interaction::control_numbers::ControlNumbers,
    pub control_open: bool,
    /// Snap kind under the cursor (status bar).
    pub last_snap: Option<crate::interaction::snap::SnapKind>,
    /// Screen-space snapping and its switches (bottom toolbar).
    pub snapper: crate::interaction::snap::Snapper,
    pub switches: crate::interaction::snap::Switches,
    pub snap_tolerance_px: f64,
    /// Right-click snap override: only this kind for the next pick.
    pub snap_override: Option<crate::interaction::snap::SnapKind>,
    /// Shift direction lock and X/Y/Z coordinate locks of the running pick.
    pub locks: crate::interaction::snap::Locks,
    /// Pick-command engine (creation by points).
    pub engine: crate::interaction::Engine,
    /// World point under the cursor on the work plane (set by the viewport).
    pub cursor: Option<open_bim_renderer::DVec3>,
    /// Model tree rows keyed by (journal length, revision).
    tree: ((usize, u64), TreeRows),
    /// The running open job restores an autosave snapshot.
    recovering: bool,
    /// Grouping of the model tree («Органайзер»).
    pub organizer: crate::organizer::Organizer,
}

impl Workbench {
    pub fn new(editor: Editor) -> Self {
        let mut bench = Self {
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
            view_title: "Вид 1 — 3d",
            property_search: String::new(),
            fewer_properties: false,
            light_theme: true,
            work_area: true,
            detail: Default::default(),
            components: Default::default(),
            component_editor: Default::default(),
            grid: None,
            trim: Default::default(),
            display: Default::default(),
            document_info: None,
            document_notes: String::new(),
            grid_editing: false,
            axis_buffer: None,
            level_buffer: None,
            kb_cursor: None,
            kb_object: None,
            keytips: Default::default(),
            alt_down: false,
            alt_clean: false,
            focus_target: None,
            focus_area: FocusArea::Viewport,
            theme_applied: None,
            quick_launch: String::new(),
            search: String::new(),
            workplane_picks: None,
            select: Default::default(),
            edit_pick: Default::default(),
            direct_modification: true,
            drag_drop: true,
            special: Default::default(),
            special_open: None,
            filter: Default::default(),
            filter_open: false,
            previous_selection: BTreeSet::new(),
            selection_seen: BTreeSet::new(),
            rollover: true,
            show_components: true,
            hovered: None,
            delete_confirm: None,
            snap_cycle: 0,
            pane: Default::default(),
            corners: Default::default(),
            treatment: TREATMENTS[1].into(),
            views: vec![views::ModelView::model_3d()],
            active_view: 0,
            clip: [None, None],
            contextual: true,
            context_at: None,
            measure: None,
            measured: None,
            angle_picks: None,
            measured_angle: None,
            inquiry: None,
            plugins: Default::default(),
            plugins_seen: (0, 0),
            clip_pick: false,
            groups: crate::interaction::object_groups::default_groups(),
            representation: Vec::new(),
            representation_open: false,
            group_edited: 0,
            representation_version: 0,
            applied: None,
            control: Default::default(),
            control_open: false,
            last_snap: None,
            snapper: Default::default(),
            switches: Default::default(),
            snap_tolerance_px: 12.,
            snap_override: None,
            locks: Default::default(),
            cursor: None,
            recovering: false,
            organizer: Default::default(),
        };
        let project = bench.editor.document.project().clone();
        bench.tools.autosave.mark_saved(&project);
        bench
    }

    fn message(&mut self, text: impl Into<String>) {
        self.tools.message = Some(text.into());
    }

    /// The «standard» property file is loaded when a command starts.
    fn load_standard(&mut self, kind: Option<crate::tools::property_files::FileKind>) {
        use crate::tools::property_files::{self as files, FileKind, STANDARD};
        let Some(kind) = kind else { return };
        let dir = files::folder(&self.tools.path);
        let Some(loaded) = files::standard(&dir, kind) else {
            return;
        };
        let note = match loaded {
            Ok(values) if kind == FileKind::Bolt => {
                files::apply_to_bolts(&mut self.tools.detailing, &values);
                files::loaded_message(STANDARD, &[])
            }
            Ok(values) => match files::import_catalogue(&mut self.editor, &values) {
                Ok(()) => {
                    let project = self.editor.document.project();
                    let warnings = files::apply_to_draft(project, &mut self.editor.draft, &values);
                    files::loaded_message(STANDARD, &warnings)
                }
                Err(e) => e,
            },
            Err(e) => e,
        };
        let prompt = self.tools.message.take().unwrap_or_default();
        self.message(format!("{prompt}\n{note}").trim().to_string());
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Open a project; a newer autosave snapshot of it is offered first.
    pub fn start_open(&mut self, path: PathBuf) {
        if let Some(offer) = self.tools.autosave.recovery(&path) {
            self.tools.autosave.offer = Some(offer);
            return;
        }
        self.recovering = false;
        self.spawn_open(path);
    }

    /// Restore the offered snapshot; the project path stays the same, so
    /// ⌘S writes the restored model to the project file.
    pub fn recover_autosave(&mut self) {
        let Some(offer) = self.tools.autosave.offer.take() else {
            return;
        };
        self.tools.path = offer.project.to_string_lossy().into_owned();
        self.recovering = true;
        self.spawn_open(offer.snapshot);
    }

    /// Open the project file and keep the snapshot on disk.
    pub fn decline_autosave(&mut self) {
        let Some(offer) = self.tools.autosave.offer.clone() else {
            return;
        };
        self.tools.autosave.decline(&offer);
        if offer.project_missing {
            self.message(format!(
                "Файла {} нет; автосохранение осталось: {}",
                offer.project.display(),
                offer.snapshot.display()
            ));
        } else {
            self.start_open(offer.project);
        }
    }

    /// ⌘S: previous file kept as «.bak», autosave snapshot removed.
    pub fn save_project(&mut self) -> Result<String, String> {
        let path = PathBuf::from(&self.tools.path);
        let message = self
            .tools
            .autosave
            .save(&path, self.editor.document.project())?;
        self.editor.record("Сохранить");
        Ok(message)
    }

    /// Timer autosave; called every frame, writes only when due and changed.
    pub fn autosave_tick(&mut self, now: std::time::Instant) {
        if self.busy() || self.tools.autosave.offer.is_some() {
            return;
        }
        let path = PathBuf::from(&self.tools.path);
        match self
            .tools
            .autosave
            .tick(now, &path, self.editor.document.project())
        {
            Ok(Some(snapshot)) => self
                .editor
                .record(&format!("Автосохранение {}", snapshot.display())),
            Ok(None) => {}
            Err(e) => self.message(e),
        }
    }

    /// Start loading a project on a worker thread (scene meshes included).
    fn spawn_open(&mut self, path: PathBuf) {
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
        self.job = Some(ActiveJob::Open(Box::new(job), path));
    }

    pub fn start_check(&mut self) {
        let project = self.editor.document.project().clone();
        let fresh = std::mem::take(&mut self.tools.coordination.renumber_all);
        self.job = Some(ActiveJob::Check(Job::spawn(
            "Нумерация и проверка коллизий",
            move |_| crate::tools::coordination_ui::CoordinationUi::compute(&project, fresh),
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
            ActiveJob::Plugin(j, _) => !matches!(j.poll(), Poll::Running),
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
                    let project = self.editor.document.project().clone();
                    if std::mem::take(&mut self.recovering) {
                        self.tools.autosave.mark_recovered(&project);
                        self.message(format!(
                            "Восстановлено из {}. Сохраните проект (⌘S), чтобы записать его в {}",
                            path.display(),
                            self.tools.path
                        ));
                    } else {
                        self.tools.autosave.mark_saved(&project);
                        self.message(format!("Открыт {}", path.display()));
                    }
                    return Some(Request::Loaded(scene));
                }
                Err(e) => {
                    self.recovering = false;
                    self.editor
                        .record(&format!("Ошибка файла {}: {e}", path.display()));
                    self.message(format!("Ошибка файла: {e}. Текущая модель не изменена."));
                }
            },
            ActiveJob::Check(j) => match j.wait() {
                Ok((numbering, report)) => {
                    self.tools.coordination.accept(numbering, report);
                    if let Err(e) = self.tools.coordination.store(&mut self.editor) {
                        self.tools.coordination.message = Some(e);
                    }
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
            ActiveJob::Plugin(j, depth) => match j.wait() {
                Ok(run) if self.editor.document.history_depth() != depth => {
                    let _ = run;
                    self.plugins.message = Some(
                        "Модель изменилась во время работы плагина; запустите его снова".into(),
                    );
                }
                Ok(run) if run.amend => {
                    let mut text = run.label.clone();
                    for p in &run.problems {
                        text.push_str("; ");
                        text.push_str(p);
                    }
                    self.plugins.message = Some(text.clone());
                    if !run.problems.is_empty() {
                        self.message(text);
                    }
                    if !run.commands.is_empty() {
                        return Some(Request::Action(Action::Amend(run.commands)));
                    }
                }
                Ok(run) if run.commands.is_empty() => {
                    self.plugins.message = Some(format!("{}: изменений нет", run.label));
                }
                Ok(run) => {
                    let text = format!("{}: создано объектов {}", run.label, run.created.len());
                    self.plugins.message = Some(text.clone());
                    self.message(text);
                    self.editor.selected = run.created.into_iter().collect();
                    return Some(Request::Action(Action::Commit(run.label, run.commands)));
                }
                Err(e) => self.plugins.message = Some(e),
            },
        }
        None
    }

    /// Run a plugin in the background on a copy of the model.
    pub fn start_plugin(&mut self, run: plugins::RunRequest) {
        if self.busy() {
            self.plugins.message = Some("Дождитесь завершения текущей операции".into());
            return;
        }
        let project = self.editor.document.project().clone();
        let label = match run.placed {
            Some(_) => format!("Изменить плагин «{}»", run.plugin.manifest.name),
            None => format!("Плагин «{}»", run.plugin.manifest.name),
        };
        let depth = self.editor.document.history_depth();
        let job_label = label.clone();
        self.plugins.message = Some(format!("{label}: выполняется…"));
        self.job = Some(ActiveJob::Plugin(
            Job::spawn(job_label, move |_| {
                let mut session = open_bim_api::Session::new(project).map_err(|e| e.to_string())?;
                let created = match run.placed {
                    Some(id) => session.rerun_plugin(&run.plugin, id, Some(&run.attributes)),
                    None => session.run_plugin(&run.plugin, &run.inputs, &run.attributes),
                }
                .map_err(|e| e.to_string())?;
                Ok(PluginRun {
                    label,
                    commands: session.into_staged(),
                    created,
                    amend: false,
                    problems: Vec::new(),
                })
            }),
            depth,
        ));
    }

    /// After a new undo step that changed input parts of placed plugins,
    /// run those plugins again in the background; their changes are added
    /// to the same undo step. Called every frame.
    pub fn follow_plugins(&mut self) {
        let document = &self.editor.document;
        let seen = (document.transactions(), document.project().revision);
        if seen == self.plugins_seen || self.busy() {
            return;
        }
        let new_step = seen.0 > self.plugins_seen.0 && document.history_depth().1 == 0;
        self.plugins_seen = seen;
        let Some(touched) = document.last_touched().filter(|_| new_step) else {
            return;
        };
        let project = document.project();
        if open_bim_api::Session::dependent_plugins(project, touched).is_empty() {
            return;
        }
        let (project, touched) = (project.clone(), touched.clone());
        let dirs = plugins::search_dirs(&self.tools.path, &self.plugins.folder);
        let depth = self.editor.document.history_depth();
        let label = "Обновление плагинов".to_string();
        self.job = Some(ActiveJob::Plugin(
            Job::spawn(label.clone(), move |_| {
                let mut session = open_bim_api::Session::new(project).map_err(|e| e.to_string())?;
                session.set_plugins(open_bim_api::plugins::discover(&dirs).0);
                let problems = session.rerun_dependent(&touched);
                Ok(PluginRun {
                    label: format!("{label}: готово"),
                    commands: session.into_staged(),
                    created: Vec::new(),
                    amend: true,
                    problems,
                })
            }),
            depth,
        ));
    }

    /// Run a command; model-changing commands are refused while a job runs.
    pub fn execute(&mut self, cmd: Cmd) -> Option<Request> {
        self.sync_selection();
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
            Cmd::ExportNc => return Some(Request::Action(Action::ExportNc)),
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
            Cmd::DisplayWireframe | Cmd::DisplayTransparent | Cmd::DisplayShaded => {
                use open_bim_renderer::RenderMode;
                self.display.mode = match cmd {
                    Cmd::DisplayWireframe => RenderMode::Wireframe,
                    Cmd::DisplayTransparent => RenderMode::Transparent,
                    _ => RenderMode::Shaded,
                };
                self.message(format!(
                    "Отображение: {} (прозрачность {:.0} %)",
                    self.display.mode.label(),
                    self.display.transparency * 100.
                ));
            }
            Cmd::ToolAxis | Cmd::ToolBeam | Cmd::ToolColumn | Cmd::ToolPlate => {
                let (mode, kind) = tool(match cmd {
                    Cmd::ToolAxis => ToolKind::Axis,
                    Cmd::ToolBeam => ToolKind::Beam,
                    Cmd::ToolColumn => ToolKind::Column,
                    _ => ToolKind::Plate,
                });
                self.mode = mode;
                self.detail.cancel();
                let first = self.editor.begin(kind);
                if cmd == Cmd::ToolBeam {
                    // «Сдвоенный профиль» is its own command.
                    self.editor.draft.twin = crate::tools::twin::Twin::Single;
                }
                self.engine.start(kind);
                self.message(self.engine.prompt());
                if first {
                    self.load_standard(crate::tools::property_files::FileKind::of_tool(kind));
                }
            }
            Cmd::ToolTwin => {
                use crate::tools::twin::Twin;
                let kept = self.editor.draft.twin;
                let r = self.execute(Cmd::ToolBeam);
                self.editor.draft.twin = if kept == Twin::Single {
                    Twin::BackToBack
                } else {
                    kept
                };
                self.message(format!(
                    "Сдвоенный профиль ({}): {}",
                    self.editor.draft.twin.label().to_lowercase(),
                    self.engine.prompt()
                ));
                return r;
            }
            Cmd::ToolBolt | Cmd::ToolWeld | Cmd::ToolWeldPolygon | Cmd::ToolWeldOnPart => {
                self.engine.cancel();
                self.editor.cancel();
                self.mode = Mode::Model;
                use crate::interaction::tools::DetailTool;
                let tool = match cmd {
                    Cmd::ToolBolt => DetailTool::Bolt,
                    Cmd::ToolWeldPolygon => DetailTool::WeldPolygon,
                    Cmd::ToolWeldOnPart => DetailTool::WeldOnPart,
                    _ => DetailTool::Weld,
                };
                self.detail = crate::interaction::tools::DetailPicker::start(tool);
                // The command's properties show in the side pane.
                self.side_panel = Some(chrome::SidePanel::Properties);
                self.message(self.prompt());
                if cmd == Cmd::ToolBolt && !std::mem::replace(&mut self.tools.bolts_used, true) {
                    self.load_standard(Some(crate::tools::property_files::FileKind::Bolt));
                }
            }
            Cmd::DocumentInfo => {
                self.document_info = Some(self.editor.document.project().document.clone());
                self.document_notes = self
                    .editor
                    .document
                    .project()
                    .document
                    .general_notes
                    .join("\n");
                self.message(
                    "Сведения о документе: графы основной надписи (ГОСТ Р 21.101, прил. Е)",
                );
            }
            Cmd::ConstructionLine
            | Cmd::ConstructionCircle
            | Cmd::ModelDimension
            | Cmd::FitEnd
            | Cmd::CutLine
            | Cmd::CutPart
            | Cmd::CutPolygon
            | Cmd::SplitPart
            | Cmd::CombineParts
            | Cmd::ExtendTo
            | Cmd::ToolGusset
            | Cmd::ToolStiffener
            | Cmd::AssemblyAdd => {
                use crate::interaction::trim::{TrimPicker, TrimTool};
                self.engine.cancel();
                self.detail.cancel();
                self.mode = Mode::Model;
                self.side_panel = Some(chrome::SidePanel::Properties);
                self.trim = TrimPicker::start(match cmd {
                    Cmd::ConstructionLine => TrimTool::ConstructionLine,
                    Cmd::ConstructionCircle => TrimTool::ConstructionCircle,
                    Cmd::ModelDimension => TrimTool::Dimension,
                    Cmd::FitEnd => TrimTool::FitEnd,
                    Cmd::CutLine => TrimTool::CutLine,
                    Cmd::CutPart => TrimTool::CutPart,
                    Cmd::CutPolygon => TrimTool::CutPolygon,
                    Cmd::SplitPart => TrimTool::Split,
                    Cmd::CombineParts => TrimTool::Combine,
                    Cmd::ExtendTo => TrimTool::Extend,
                    Cmd::ToolGusset => TrimTool::Gusset,
                    Cmd::ToolStiffener => TrimTool::Stiffener,
                    Cmd::AssemblyAdd => TrimTool::AddToAssembly,
                    _ => unreachable!("matched above"),
                });
                // A selected part is taken as the first part.
                if self.trim.need() == Some(crate::interaction::trim::Need::Part)
                    && self.trim.tool != Some(TrimTool::AddToAssembly)
                    && let [id] = self.editor.selected.iter().copied().collect::<Vec<_>>()[..]
                    && self.editor.document.project().elements.contains_key(&id)
                {
                    self.trim.parts.push(id);
                }
                self.message(self.prompt());
            }
            Cmd::AssemblyCreate
            | Cmd::AssemblyRemove
            | Cmd::AssemblySetMain
            | Cmd::AssemblyExplode => {
                use crate::interaction::assembly_edit as asm;
                let project = self.editor.document.project();
                let selected = &self.editor.selected;
                let result = match cmd {
                    Cmd::AssemblyCreate => asm::create_from(project, selected),
                    Cmd::AssemblyRemove => asm::remove_from(project, selected),
                    Cmd::AssemblySetMain => {
                        match selected.iter().copied().collect::<Vec<_>>()[..] {
                            [part] => asm::set_main(project, part),
                            _ => Err("Выберите одну деталь — она станет главной".into()),
                        }
                    }
                    _ => asm::explode(project, selected),
                };
                return match result {
                    Ok(commands) => Some(Request::Action(Action::Commit(
                        commands::caption(cmd),
                        commands,
                    ))),
                    Err(e) => {
                        self.message(e);
                        None
                    }
                };
            }
            Cmd::GridDialog => {
                // An existing grid opens prefilled for editing (by double-clicking a grid).
                let existing = crate::interaction::grid_edit::spec_from_project(
                    self.editor.document.project(),
                );
                self.grid_editing = existing.is_some();
                self.grid = Some(existing.unwrap_or_default());
                self.message("Сетка: X/Y — шаги («0 4*6000»), Z — отметки");
            }
            Cmd::CreateElement => return Some(Request::Action(Action::Create)),
            Cmd::DrawDimension
            | Cmd::DrawLine
            | Cmd::DrawPolyline
            | Cmd::DrawRectangle
            | Cmd::DrawCircle
            | Cmd::DrawHatch
            | Cmd::DrawWipeout
            | Cmd::DrawText
            | Cmd::DrawNode => {
                use crate::drawing_editor::tools::DrawTool;
                self.mode = Mode::Drawings;
                if self.drawings.current.is_none() {
                    self.message("Сначала выберите или создайте чертёж");
                    return None;
                }
                self.drawings.start_tool(match cmd {
                    Cmd::DrawDimension => DrawTool::Dimension,
                    Cmd::DrawLine => DrawTool::Line,
                    Cmd::DrawPolyline => DrawTool::Polyline,
                    Cmd::DrawRectangle => DrawTool::Rectangle,
                    Cmd::DrawCircle => DrawTool::Circle,
                    Cmd::DrawHatch => DrawTool::Hatch,
                    Cmd::DrawWipeout => DrawTool::Wipeout,
                    Cmd::DrawText => DrawTool::Text,
                    Cmd::DrawNode => DrawTool::Node,
                    _ => unreachable!("matched above"),
                });
                self.message(self.drawings.message.clone().unwrap_or_default());
            }
            // In the drawing editor Enter, Esc and Delete act on the sheet.
            Cmd::FinishPicks
                if self.mode == Mode::Drawings && self.drawings.tool.tool.is_some() =>
            {
                if let Err(e) = self.drawings.tool_finish(&mut self.editor) {
                    self.drawings.message = Some(e);
                }
            }
            Cmd::CancelTool if self.mode == Mode::Drawings && self.drawings.tool.tool.is_some() => {
                self.drawings.cancel_tool();
                self.message("Инструмент чертежа выключен");
            }
            Cmd::DeleteSelected | Cmd::UndoPick
                if self.mode == Mode::Drawings
                    && (self.drawings.selected_object.is_some()
                        || self.drawings.selected_dimension.is_some()
                        || self.drawings.selected_text.is_some()) =>
            {
                if let Err(e) = self.drawings.delete_selected(&mut self.editor) {
                    self.drawings.message = Some(e);
                }
            }
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
                } else if self.measure.take().is_some() {
                    self.measured = None;
                    self.message("Измерение завершено");
                } else if std::mem::take(&mut self.clip_pick) {
                    self.message("Секущая плоскость не задана");
                } else if self.angle_picks.take().is_some() {
                    self.measured_angle = None;
                    self.message("Измерение угла завершено");
                } else if self.trim.cancel()
                    || self.detail.cancel()
                    || self.edit_pick.cancel()
                    || self.components.cancel()
                    || self.special.cancel_pick()
                {
                    self.message("Команда прервана");
                } else if self.delete_confirm.take().is_some() {
                    self.message("Удаление отменено");
                } else if !self.engine.active() && !self.editor.selected.is_empty() {
                    self.editor.selected.clear();
                    self.message("Выбор снят");
                } else {
                    self.engine.cancel();
                    self.editor.cancel();
                    self.kb_cursor = None;
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
                    Cmd::PanelModelTree => chrome::SidePanel::Tree,
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
                let ids: BTreeSet<ObjectId> = self
                    .editor
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| project.contains_id(*id))
                    .collect();
                if ids.is_empty() {
                    self.message("Ничего не выбрано");
                    return None;
                }
                match crate::interaction::selection::delete_count(project, &ids) {
                    // Dependants (holes, bolts, welds, connections): ask first.
                    Ok(total) if total > ids.len() => {
                        self.message(format!(
                            "Подтвердите удаление: связанных объектов {}",
                            total - ids.len()
                        ));
                        self.delete_confirm = Some((ids, total));
                    }
                    Ok(_) => return Some(self.delete_request(&ids)),
                    Err(e) => self.message(e),
                }
            }
            Cmd::MoveByPicks | Cmd::CopyByPicks | Cmd::MirrorByPicks | Cmd::RotateByPicks => {
                use crate::interaction::selection::EditOp;
                if self.editor.selected.is_empty() {
                    self.message("Сначала выберите объекты");
                    return None;
                }
                self.engine.cancel();
                self.editor.cancel();
                self.detail.cancel();
                self.edit_pick = crate::interaction::selection::EditPicker {
                    op: Some(match cmd {
                        Cmd::MoveByPicks => EditOp::Move,
                        Cmd::CopyByPicks => EditOp::Copy,
                        Cmd::MirrorByPicks => EditOp::Mirror,
                        _ => EditOp::Rotate,
                    }),
                    points: Vec::new(),
                };
                self.mode = Mode::Model;
                self.message(self.prompt());
            }
            Cmd::CopySpecial | Cmd::MoveSpecial => {
                let copy = cmd == Cmd::CopySpecial;
                self.engine.cancel();
                self.editor.cancel();
                self.detail.cancel();
                self.edit_pick.cancel();
                self.special_open = Some(copy);
                self.mode = Mode::Model;
                self.message(if copy {
                    "Копировать — специальное: выберите способ и нажмите «Копировать»"
                } else {
                    "Переместить — специальное: выберите способ и нажмите «Переместить»"
                });
            }
            Cmd::SelectPrevious => {
                self.sync_selection();
                if self.previous_selection.is_empty() {
                    self.message("Предыдущего выбора нет");
                } else {
                    let project = self.editor.document.project();
                    let previous: BTreeSet<ObjectId> = self
                        .previous_selection
                        .iter()
                        .copied()
                        .filter(|id| project.contains_id(*id))
                        .collect();
                    self.editor.selected = previous;
                    self.sync_selection();
                    self.message(format!(
                        "Предыдущий выбор: {} объектов",
                        self.editor.selected.len()
                    ));
                }
            }
            Cmd::ToggleComponentSymbols => {
                self.show_components = !self.show_components;
                self.message(if self.show_components {
                    "Символы узлов: показаны"
                } else {
                    "Символы узлов: скрыты"
                });
            }
            Cmd::ToggleRollover => {
                self.rollover = !self.rollover;
                self.hovered = None;
                self.message(if self.rollover {
                    "Подсветка под курсором: включена"
                } else {
                    "Подсветка под курсором: выключена"
                });
            }
            Cmd::SelectionFilter => {
                self.filter_open = !self.filter_open;
                self.message(if self.filter.active() {
                    "Фильтр выбора включён"
                } else {
                    "Фильтр выбора: задайте поля и включите"
                });
            }
            Cmd::ToggleDragDrop => {
                self.drag_drop = !self.drag_drop;
                self.message(if self.drag_drop {
                    "Перетаскивание: включено (Ctrl — копировать)"
                } else {
                    "Перетаскивание: выключено"
                });
            }
            Cmd::FitSelected => {
                if self.editor.selected.is_empty() {
                    self.message("Ничего не выбрано");
                } else {
                    return Some(Request::FitSelected);
                }
            }
            Cmd::ShowOnlySelected => {
                let project = self.editor.document.project();
                self.hidden = project
                    .elements
                    .keys()
                    .chain(project.axes.keys())
                    .copied()
                    .filter(|id| !self.editor.selected.contains(id))
                    .collect();
                self.message(format!(
                    "Показаны только выбранные: {}",
                    self.editor.selected.len()
                ));
            }
            Cmd::ToggleDirectModification => {
                self.direct_modification = !self.direct_modification;
                self.message(if self.direct_modification {
                    "Прямое изменение: включено"
                } else {
                    "Прямое изменение: выключено"
                });
            }
            Cmd::GridViews => {
                self.views = views::grid_views(self.editor.document.project());
                self.active_view = 0;
                self.mode = Mode::Model;
                self.side_panel = Some(chrome::SidePanel::Views);
                self.message(format!("Создано видов: {}", self.views.len()));
            }
            Cmd::PanelViews => {
                self.mode = Mode::Model;
                self.side_panel = if self.side_panel == Some(chrome::SidePanel::Views) {
                    None
                } else {
                    Some(chrome::SidePanel::Views)
                };
                self.message(format!("Видов модели: {}", self.views.len()));
            }
            Cmd::PlaneOr3d => {
                if self.views.len() == 1 {
                    self.views = views::grid_views(self.editor.document.project());
                }
                // Plane view at the work plane level, or back to 3D.
                let target = if self.active_view != 0 {
                    0
                } else {
                    let z = self.engine.workplane.origin.z;
                    self.views
                        .iter()
                        .position(|v| matches!(v.kind, views::ViewKind::Plan { elevation } if (elevation - z).abs() < 1.))
                        .or_else(|| self.views.iter().position(|v| matches!(v.kind, views::ViewKind::Plan { .. })))
                        .unwrap_or(0)
                };
                return self.apply_view(target);
            }
            Cmd::ToggleContextual => {
                self.contextual = !self.contextual;
                self.message(if self.contextual {
                    "Контекстная панель включена"
                } else {
                    "Контекстная панель выключена"
                });
            }
            Cmd::Measure => {
                self.engine.cancel();
                self.editor.cancel();
                self.detail.cancel();
                self.edit_pick.cancel();
                self.measure = Some(Vec::new());
                self.mode = Mode::Model;
                self.message(self.prompt());
            }
            Cmd::MeasureAngle => {
                self.engine.cancel();
                self.editor.cancel();
                self.detail.cancel();
                self.edit_pick.cancel();
                self.measure = None;
                self.angle_picks = Some(Vec::new());
                self.mode = Mode::Model;
                self.message(self.prompt());
            }
            Cmd::ControlNumbers => {
                self.control_open = !self.control_open;
                self.message(if self.control_open {
                    "Контрольные номера: задайте префикс, начало и порядок"
                } else {
                    "Окно контрольных номеров закрыто"
                });
            }
            Cmd::ObjectRepresentation => {
                self.representation_open = !self.representation_open;
                self.message(if self.representation_open {
                    "Представление объектов: группы и их вид"
                } else {
                    "Окно представления объектов закрыто"
                });
            }
            Cmd::ClipPlanePick => {
                self.engine.cancel();
                self.editor.cancel();
                self.detail.cancel();
                self.measure = None;
                self.angle_picks = None;
                self.clip_pick = true;
                self.mode = Mode::Model;
                self.message(self.prompt());
            }
            Cmd::ClipPlaneRemove => {
                self.clip_pick = false;
                self.clip[0] = None;
                self.message("Секущая плоскость убрана");
            }
            Cmd::InquireObject | Cmd::CompareParts => {
                use crate::interaction::inquire;
                let project = self.editor.document.project();
                let result = if cmd == Cmd::InquireObject {
                    inquire::inquire_selected(project, &self.editor.selected)
                } else {
                    inquire::compare_selected(project, &self.editor.selected)
                };
                match result {
                    Ok(i) => {
                        self.message(i.note.clone().unwrap_or_else(|| i.title.clone()));
                        self.inquiry = Some(i);
                    }
                    Err(e) => self.message(e),
                }
            }
            Cmd::CycleFocus => {
                self.focus_area = self.focus_area.next();
                self.focus_target = Some(self.focus_area);
                self.message(format!("Фокус: {}", self.focus_area.label()));
            }
            Cmd::AutoConnections => return self.auto_connections(),
            Cmd::FitToProfile => return self.fit_to_profile(),
            Cmd::ComponentCatalog => {
                self.trim.cancel();
                self.detail.cancel();
                self.components.cancel();
                self.components.window = true;
                self.message("Каталог компонентов: выберите узел");
            }
            Cmd::ComponentEditor => {
                self.components.window = false;
                let base = components::catalog().remove(0);
                self.new_component(&base);
            }
            Cmd::FocusSearch => {
                self.focus_area = FocusArea::Search;
                self.focus_target = Some(FocusArea::Search);
                self.message(format!("Фокус: {}", FocusArea::Search.label()));
            }
            Cmd::SelectAll => {
                let project = self.editor.document.project();
                let mut all: BTreeSet<ObjectId> = BTreeSet::new();
                if self.select.parts || self.select.assemblies {
                    all.extend(project.elements.keys());
                }
                if self.select.grids {
                    all.extend(project.axes.keys());
                }
                if self.select.levels {
                    all.extend(project.levels.keys());
                }
                all.retain(|id| !self.hidden.contains(id) && self.filter.allows(project, *id));
                self.message(format!("Выбрано объектов: {}", all.len()));
                self.editor.selected = all;
                self.sync_selection();
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
            Cmd::UndoPick => {
                self.engine.plate_mode = self.editor.draft.plate_mode;
                if self.trim.tool.is_some() {
                    if self.trim.points.pop().is_none() {
                        self.trim.parts.pop();
                    }
                } else if let Some(points) = &mut self.workplane_picks {
                    points.pop();
                } else if self.edit_pick.op.is_some() {
                    self.edit_pick.points.pop();
                } else if self.special.picking.is_some() {
                    self.special.picks.pop();
                } else if self.detail.picks_points() {
                    self.detail.points.pop();
                } else if !self.engine.active() && !self.editor.selected.is_empty() {
                    // macOS «delete» is Backspace: without a command it deletes.
                    return self.execute(Cmd::DeleteSelected);
                } else if let crate::interaction::Outcome::Ignored(e) = self.engine.undo_pick() {
                    self.message(e);
                    return None;
                }
                self.message(self.prompt());
            }
            Cmd::FinishPicks if self.trim.tool.is_some() => return self.finish_trim(),
            Cmd::FinishPicks if self.detail.weld_by_points() => return self.finish_weld(),
            Cmd::FinishPicks if self.detail.tool.is_some() => match self.detail.finish_parts() {
                Ok(()) => self.message(self.prompt()),
                Err(e) => self.message(e),
            },
            Cmd::FinishPicks => {
                self.engine.plate_mode = self.editor.draft.plate_mode;
                let outcome = self.engine.finish();
                if let Some(kind) = self.engine.tool
                    && !self.editor.active
                {
                    let _ = self.editor.begin(kind);
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
            Cmd::Plugins => {
                self.plugins.open = !self.plugins.open;
                self.message(if self.plugins.open {
                    "Плагины: выберите плагин, задайте входы и атрибуты"
                } else {
                    "Окно плагинов закрыто"
                });
            }
            Cmd::DrawingWizard => {
                let wizard = &mut self.drawings.wizard;
                wizard.open = !wizard.open;
                self.message(if self.drawings.wizard.open {
                    "Мастер чертежей: выберите правила и нажмите «Создать чертежи»"
                } else {
                    "Мастер чертежей закрыт"
                });
            }
            Cmd::DocumentManager => {
                let manager = &mut self.drawings.manager;
                manager.open = !manager.open;
                let count = self.editor.document.project().drawings.len();
                self.message(if self.drawings.manager.open {
                    format!("Менеджер документов: чертежей {count}")
                } else {
                    "Менеджер документов закрыт".into()
                });
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
            Cmd::GeneralData => {
                self.mode = Mode::Drawings;
                let r = self.drawings.create_general_data(&mut self.editor);
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
            Cmd::IssueSet => {
                self.mode = Mode::Drawings;
                let r = self.drawings.issue_set(&mut self.editor);
                report(r, self);
            }
            Cmd::RecordChange => {
                self.mode = Mode::Drawings;
                let r = self.drawings.record_change(&mut self.editor);
                report(r, self);
            }
            Cmd::ExportPackage => {
                self.mode = Mode::Drawings;
                let r = self.drawings.export_package(&self.editor);
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
        self.engine.reference = None;
        // An override lasts one pick; a held direction starts again from the new point.
        self.snap_override = None;
        self.locks.direction = None;
        if matches!(
            self.trim.need(),
            Some(crate::interaction::trim::Need::Point | crate::interaction::trim::Need::Polygon)
        ) {
            self.trim.points.push(point);
            if self.trim.complete() {
                return self.finish_trim();
            }
            self.message(self.prompt());
            return None;
        }
        if self.clip_pick {
            self.clip_pick = false;
            self.message("Секущая плоскость задана: модель перед ней скрыта");
            return Some(Request::ClipAt(point));
        }
        if let Some(picks) = &mut self.angle_picks {
            picks.push(point);
            if let [v, a, b] = picks[..] {
                picks.clear();
                match crate::interaction::inquire::angle_deg(v, a, b) {
                    Some(deg) => {
                        self.measured_angle = Some([v, a, b]);
                        self.message(format!(
                            "Угол {deg:.2}° (дополнительный {:.2}°)",
                            180. - deg
                        ));
                    }
                    None => self.message("Точки совпадают: укажите вершину и две разные точки"),
                }
            } else {
                self.measured_angle = None;
                self.message(self.prompt());
            }
            return None;
        }
        if let Some(picks) = &mut self.measure {
            picks.push(point);
            if let [a, b] = picks[..] {
                let d = b - a;
                self.measured = Some((a, b));
                picks.clear();
                self.message(format!(
                    "Расстояние {:.1} мм · ΔX {:.1} · ΔY {:.1} · ΔZ {:.1}",
                    d.length(),
                    d.x,
                    d.y,
                    d.z
                ));
            } else {
                self.measured = None;
                self.message(self.prompt());
            }
            return None;
        }
        if self.special.picking.is_some() {
            match self.special.pick(point, &self.engine.workplane) {
                Ok(true) => self.message(
                    "Значения заданы — проверьте и нажмите «Копировать» или «Переместить»",
                ),
                Ok(false) => self.message(self.prompt()),
                Err(e) => self.message(e),
            }
            return None;
        }
        if self.edit_pick.op.is_some() {
            return match self.edit_pick.pick(point, self.engine.workplane.axes[2]) {
                Ok(Some(edit)) => {
                    self.message(self.edit_pick.prompt().unwrap_or_else(|| "Готово".into()));
                    Some(Request::Action(Action::Edit(edit)))
                }
                Ok(None) => {
                    self.message(self.prompt());
                    None
                }
                Err(e) => {
                    self.edit_pick.points.pop();
                    self.message(e);
                    None
                }
            };
        }
        if self.detail.picks_points() && self.detail.weld_by_points() {
            self.detail.points.push(point);
            self.message(self.prompt());
            return None;
        }
        if self.detail.picks_points() {
            self.detail.points.push(point);
            if self.detail.points.len() < 2 {
                self.message(self.prompt());
                return None;
            }
            let (a, b) = (self.detail.points[0], self.detail.points[1]);
            let parts = std::mem::take(&mut self.detail.parts);
            self.detail.cancel();
            return match crate::interaction::tools::bolt_by_picks(
                self.editor.document.project(),
                &parts,
                a,
                b,
                self.engine.workplane.axes[2],
                &self.tools.detailing,
            ) {
                Ok(commands) => {
                    self.message("Болтовая группа создана");
                    Some(Request::Action(Action::Commit(
                        "Создать: болт".into(),
                        commands,
                    )))
                }
                Err(e) => {
                    self.message(e);
                    None
                }
            };
        }
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
        self.engine.plate_mode = self.editor.draft.plate_mode;
        let outcome = self.engine.pick(point);
        self.outcome(outcome)
    }

    /// Whether a viewport click picks a part (bolt parts, weld, the parts of
    /// fitting and cutting) instead of a point.
    pub fn picks_objects(&self) -> bool {
        self.detail.picks_objects()
            || self.components.picks_objects()
            || self.trim.need() == Some(crate::interaction::trim::Need::Part)
    }

    /// Gusset/stiffener sizes from the plate defaults and the preset fields.
    fn plate_preset(&self) -> Result<crate::interaction::trim::PlatePreset, String> {
        let d = &self.tools.detailing;
        let corner = if self.trim.tool == Some(crate::interaction::trim::TrimTool::Gusset) {
            &d.gusset_chamfer
        } else {
            &d.stiffener_snip
        };
        Ok(crate::interaction::trim::PlatePreset {
            thickness: crate::tools::number(&self.editor.draft.thickness)?,
            material: self.editor.draft.material,
            leg: crate::tools::number(&d.gusset_leg)?,
            corner: crate::tools::number(corner)?,
        })
    }

    /// Run the fitting/cutting/construction command once its inputs are in.
    fn finish_trim(&mut self) -> Option<Request> {
        use crate::interaction::trim::{self, TrimTool};
        let tool = self.trim.tool?;
        let project = self.editor.document.project();
        let normal = self.engine.workplane.axes[2];
        let (parts, pts) = (self.trim.parts.clone(), self.trim.points.clone());
        let result = match (tool, &parts[..], &pts[..]) {
            (TrimTool::ConstructionLine | TrimTool::ConstructionCircle, [], [a, b]) => {
                trim::construction(project, tool, *a, *b, normal)
            }
            (TrimTool::Dimension, [], [a, b]) => trim::dimension(project, *a, *b),
            (TrimTool::FitEnd, [p], [a, b]) => trim::fit_end(project, *p, *a, *b, normal),
            (TrimTool::CutLine, [p], [a, b, side]) => {
                trim::cut_line(project, *p, *a, *b, *side, normal)
            }
            (TrimTool::CutPart, [t, c], []) => trim::cut_part(project, *t, *c),
            (TrimTool::CutPolygon, [p], polygon) if polygon.len() >= 3 => {
                trim::cut_polygon(project, *p, polygon, normal)
            }
            (TrimTool::CutPolygon, _, _) => Err("Нужно не меньше 3 вершин выреза".into()),
            (TrimTool::Split, [p], [a]) => trim::split(project, *p, *a),
            (TrimTool::Combine, [a, b], []) => trim::combine(project, *a, *b),
            (TrimTool::Extend, [p], [a]) => trim::extend_to(project, *p, *a),
            (TrimTool::Gusset, [a, b], []) => self
                .plate_preset()
                .and_then(|pr| trim::gusset(project, *a, *b, pr)),
            (TrimTool::Stiffener, [b], [at]) => self
                .plate_preset()
                .and_then(|pr| trim::stiffener(project, *b, *at, pr)),
            (TrimTool::AddToAssembly, [target], []) => {
                crate::interaction::assembly_edit::add_to(project, &self.editor.selected, *target)
            }
            _ => return None,
        };
        // The command stays armed for the next object.
        self.trim = crate::interaction::trim::TrimPicker::start(tool);
        match result {
            Ok(commands) => {
                self.message(self.prompt());
                Some(Request::Action(Action::Commit(
                    tool.label().into(),
                    commands,
                )))
            }
            Err(e) => {
                self.message(e);
                None
            }
        }
    }

    /// Bolt: collect the main and secondary parts. Weld: first click the main
    /// part, then the secondary part; the command re-arms.
    pub fn pick_object(&mut self, id: Option<ObjectId>) -> Option<Request> {
        let Some(id) = id.filter(|id| self.editor.document.project().elements.contains_key(id))
        else {
            self.message("Укажите деталь модели");
            return None;
        };
        if self.components.armed.is_some() {
            return self.pick_component_part(id);
        }
        if self.trim.need() == Some(crate::interaction::trim::Need::Part) {
            self.trim.parts.push(id);
            if self.trim.complete() {
                return self.finish_trim();
            }
            self.message(self.prompt());
            return None;
        }
        if self.detail.weld_by_points() {
            if self.detail.weld_parts.contains(&id) {
                self.message("Привариваемая деталь должна отличаться от основной");
                return None;
            }
            self.detail.weld_parts.push(id);
            self.editor.selected = self.detail.weld_parts.iter().copied().collect();
            self.message(self.prompt());
            return None;
        }
        if self.detail.tool == Some(crate::interaction::tools::DetailTool::Bolt) {
            self.detail.parts.insert(id);
            self.editor.selected = self.detail.parts.clone();
            self.message(self.prompt());
            return None;
        }
        let Some(main) = self.detail.main else {
            self.detail.main = Some(id);
            self.message(self.prompt());
            return None;
        };
        self.detail.main = None;
        match crate::interaction::tools::weld_between(
            self.editor.document.project(),
            main,
            id,
            &self.tools.detailing,
        ) {
            Ok(commands) => {
                self.message(self.prompt());
                Some(Request::Action(Action::Commit(
                    "Создать: сварной шов".into(),
                    commands,
                )))
            }
            Err(e) => {
                self.message(e);
                None
            }
        }
    }

    /// Activate a model view: remember the work plane of the current one,
    /// switch camera, view depth and work plane (each view keeps its own).
    pub fn apply_view(&mut self, index: usize) -> Option<Request> {
        let view = self.views.get(index)?.clone();
        if let Some(current) = self.views.get_mut(self.active_view) {
            current.workplane = self.engine.workplane;
        }
        self.active_view = index;
        self.engine.workplane = view.workplane;
        if view.workplane.axes[2] == open_bim_renderer::DVec3::Z {
            self.tools.workplane = format!("{:.0}", view.workplane.origin.z);
        }
        self.clip = view.clip;
        self.message(format!("Вид: {}", view.name));
        Some(Request::ApplyView(Box::new(view)))
    }

    /// Double click on a component symbol: select the component and open its
    /// properties in the «Узлы» panel.
    pub fn open_component(&mut self, id: ObjectId) {
        let project = self.editor.document.project();
        if let Err(e) = self.tools.connection.load(project, id) {
            self.message(e);
            return;
        }
        self.editor.selected = BTreeSet::from([id]);
        self.sync_selection();
        self.mode = Mode::Model;
        self.side_panel = Some(chrome::SidePanel::Connections);
        self.message("Свойства узла: измените значения и нажмите «Изменить»");
    }

    /// Enter / middle click in a weld by points: create the weld and start the
    /// next one with the same command.
    fn finish_weld(&mut self) -> Option<Request> {
        let tool = self.detail.tool?;
        let result = crate::interaction::tools::weld_by_points(
            self.editor.document.project(),
            &self.detail.weld_parts,
            &self.detail.points,
            &self.tools.detailing,
        );
        match result {
            Ok(commands) => {
                self.detail = crate::interaction::tools::DetailPicker::start(tool);
                self.message("Сварной шов создан");
                Some(Request::Action(Action::Commit(
                    "Создать: сварной шов".into(),
                    commands,
                )))
            }
            Err(e) => {
                self.message(e);
                None
            }
        }
    }

    /// Remember the selection it replaces when the selection changed (Alt+P).
    pub fn sync_selection(&mut self) {
        if self.editor.selected != self.selection_seen {
            self.previous_selection =
                std::mem::replace(&mut self.selection_seen, self.editor.selected.clone());
        }
    }

    /// Run the open special dialog: copy N times or move once.
    pub fn run_special(&mut self, copy: bool) -> Option<Request> {
        use crate::interaction::special;
        let project = self.editor.document.project();
        let result = self.special.special(&self.engine.workplane).and_then(|op| {
            if copy {
                let n = self.special.copies()?;
                special::copy_commands(project, &self.editor.selected, &op, n).map(|(c, ids)| {
                    (
                        format!("Копировать {}", op.label()),
                        c,
                        format!("Создано копий: {n} ({} объектов)", ids.len()),
                    )
                })
            } else {
                special::move_commands(project, &self.editor.selected, &op).map(|c| {
                    (
                        format!("Переместить {}", op.label()),
                        c,
                        format!("Перемещено объектов: {}", self.editor.selected.len()),
                    )
                })
            }
        });
        match result {
            Ok((label, commands, text)) => {
                self.message(text);
                Some(Request::Action(Action::Commit(label, commands)))
            }
            Err(e) => {
                self.message(e);
                None
            }
        }
    }

    /// Drag and drop: move the selection by the drag vector, or copy it.
    pub fn drop_selection(
        &mut self,
        from: open_bim_renderer::DVec3,
        to: open_bim_renderer::DVec3,
        copy: bool,
    ) -> Option<Request> {
        let delta = open_bim_model::Point3((to - from).to_array());
        if (to - from).length() < 1e-6 || self.editor.selected.is_empty() {
            return None;
        }
        self.message(if copy {
            "Скопировано перетаскиванием"
        } else {
            "Перемещено перетаскиванием"
        });
        Some(Request::Action(Action::Edit(if copy {
            crate::tools::Edit::Copy(delta)
        } else {
            crate::tools::Edit::Move(delta)
        })))
    }

    fn special_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        use crate::interaction::special::{PickTarget, Tab};
        let Some(copy) = self.special_open else {
            return;
        };
        let mut open = true;
        let mut run = false;
        let mut pick = None;
        let selected = self.editor.selected.len();
        let d = &mut self.special;
        let fields = |ui: &mut egui::Ui, id: &str, names: &[&str], values: &mut [String]| {
            ui.horizontal(|ui| {
                for (name, value) in names.iter().zip(values.iter_mut()) {
                    let l = ui.label(*name);
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .id(egui::Id::new((id, *name)))
                            .desired_width(70.),
                    )
                    .labelled_by(l.id);
                }
            });
        };
        egui::Window::new(if copy {
            "Копировать — специальное"
        } else {
            "Переместить — специальное"
        })
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in Tab::ALL {
                    ui.selectable_value(&mut d.tab, tab, tab.label());
                }
            });
            ui.separator();
            match d.tab {
                Tab::Linear => {
                    fields(ui, "lin", &["dX", "dY", "dZ"], &mut d.delta);
                    if ui.button("Указать две точки").clicked() {
                        pick = Some(PickTarget::Vector);
                    }
                }
                Tab::Rotate => {
                    ui.label("Центр поворота, мм");
                    fields(ui, "rot", &["X0", "Y0", "Z0"], &mut d.pivot);
                    ui.horizontal(|ui| {
                        if ui.button("Указать центр").clicked() {
                            pick = Some(PickTarget::Pivot);
                        }
                        ui.label("Ось");
                        for (i, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                            ui.selectable_value(&mut d.axis, i, name);
                        }
                    });
                    let mut pair = [d.angle.clone(), d.dz.clone()];
                    fields(ui, "rot2", &["Угол, °", "dZ, мм"], &mut pair);
                    [d.angle, d.dz] = pair;
                }
                Tab::Mirror => {
                    ui.label("Линия зеркала на рабочей плоскости");
                    let mut three = [
                        d.mirror_origin[0].clone(),
                        d.mirror_origin[1].clone(),
                        d.mirror_angle.clone(),
                    ];
                    fields(ui, "mir", &["X0", "Y0", "Угол, °"], &mut three);
                    let [x, y, a] = three;
                    d.mirror_origin = [x, y];
                    d.mirror_angle = a;
                    if ui.button("Указать две точки").clicked() {
                        pick = Some(PickTarget::MirrorLine);
                    }
                }
                Tab::ToPlane => {
                    ui.label(match d.planes {
                        Some(_) => "Плоскости заданы",
                        None => "Плоскости не заданы",
                    });
                    if ui
                        .button("Указать: 3 точки исходной + 3 точки новой")
                        .clicked()
                    {
                        pick = Some(PickTarget::Planes);
                    }
                }
            }
            if copy && matches!(d.tab, Tab::Linear | Tab::Rotate) {
                fields(
                    ui,
                    "n",
                    &["Количество копий"],
                    std::slice::from_mut(&mut d.copies),
                );
            }
            if let Some(prompt) = d.prompt() {
                ui.colored_label(egui::Color32::from_rgb(0, 110, 200), prompt);
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(format!("Выбрано: {selected}"));
                if ui
                    .add_enabled(
                        selected > 0,
                        egui::Button::new(if copy {
                            "Копировать"
                        } else {
                            "Переместить"
                        }),
                    )
                    .clicked()
                {
                    run = true;
                }
            });
        });
        if let Some(target) = pick {
            self.special.start_pick(target);
            self.message(self.prompt());
        }
        if run && let Some(r) = self.run_special(copy) {
            requests.push(r);
        }
        if !open {
            self.special_open = None;
            self.special.cancel_pick();
        }
    }

    fn filter_window(&mut self, ctx: &egui::Context) {
        if !self.filter_open {
            return;
        }
        let mut open = true;
        let f = &mut self.filter;
        egui::Window::new("Фильтр выбора")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.checkbox(&mut f.enabled, "Фильтр включён");
                egui::Grid::new("selection filter")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (name, value) in [
                            ("Имя", &mut f.name),
                            ("Профиль", &mut f.profile),
                            ("Материал", &mut f.material),
                            ("Класс", &mut f.class),
                        ] {
                            let l = ui.label(name);
                            ui.add(egui::TextEdit::singleline(value).desired_width(160.))
                                .labelled_by(l.id);
                            ui.end_row();
                        }
                    });
                ui.label("Несколько значений — через пробел; * и ? — шаблоны (I20*).");
                let groups = &self.groups;
                egui::ComboBox::new("filter group", "Группа объектов")
                    .selected_text(f.group.as_ref().map_or("—".into(), |g| g.name.clone()))
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(f.group.is_none(), "—").clicked() {
                            f.group = None;
                        }
                        for g in groups {
                            if ui
                                .selectable_label(f.group.as_ref() == Some(g), &g.name)
                                .clicked()
                            {
                                f.group = Some(g.clone());
                            }
                        }
                    });
                if ui.button("Очистить").clicked() {
                    *f = Default::default();
                }
            });
        self.filter_open = open;
    }

    fn delete_request(&mut self, ids: &BTreeSet<ObjectId>) -> Request {
        let commands = ids
            .iter()
            .map(|id| open_bim_commands::Command::Delete {
                id: *id,
                policy: open_bim_commands::DeletePolicy::Cascade,
            })
            .collect();
        self.message(format!("Удалено объектов: {}", ids.len()));
        Request::Action(Action::Commit("Удалить".into(), commands))
    }

    fn delete_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let Some((ids, total)) = self.delete_confirm.clone() else {
            return;
        };
        let mut answer = None;
        egui::Window::new("Удаление")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Удалить выбранные объекты ({}) вместе со связанными ({}): отверстия, болты, швы, узлы?",
                    ids.len(),
                    total - ids.len()
                ));
                ui.horizontal(|ui| {
                    if ui.button("Удалить").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Отмена").clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                self.delete_confirm = None;
                requests.push(self.delete_request(&ids));
            }
            Some(false) => {
                self.delete_confirm = None;
                self.message("Удаление отменено");
            }
            None => {}
        }
    }

    fn control_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        use crate::interaction::control_numbers::{Order, assemblies_of, clear};
        if !self.control_open {
            return;
        }
        let mut open = true;
        let project = self.editor.document.project();
        let targets = assemblies_of(project, &self.editor.selected);
        let mut commands = None;
        let c = &mut self.control;
        egui::Window::new("Контрольные номера")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("control grid").num_columns(2).show(ui, |ui| {
                    for (label, value) in [
                        ("Префикс", &mut c.prefix),
                        ("Начальный номер", &mut c.start),
                        ("Шаг", &mut c.step),
                    ] {
                        let l = ui.label(label);
                        ui.add(egui::TextEdit::singleline(value).desired_width(80.))
                            .labelled_by(l.id);
                        ui.end_row();
                    }
                    let l = ui.label("Порядок");
                    egui::ComboBox::new("control order", "")
                        .selected_text(c.order.label())
                        .show_ui(ui, |ui| {
                            for o in Order::ALL {
                                ui.selectable_value(&mut c.order, o, o.label());
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                });
                ui.label(if self.editor.selected.is_empty() {
                    format!("Все сборки модели: {}", targets.len())
                } else {
                    format!("Сборки выбранных объектов: {}", targets.len())
                });
                ui.horizontal(|ui| {
                    if ui.button("Присвоить").clicked() {
                        commands = Some(c.commands(project, &targets).map(|cmds| ("Контрольные номера", cmds)));
                    }
                    if ui.button("Сбросить").clicked() {
                        commands = Some(Ok(("Сбросить контрольные номера", clear(project, &targets))));
                    }
                });
                ui.small("Номер — по положению основной детали; хранится в свойстве сборки «control_number».");
            });
        self.control_open = open;
        match commands {
            Some(Ok((label, cmds))) if !cmds.is_empty() => {
                self.message(format!("{label}: сборок {}", cmds.len()));
                requests.push(Request::Action(Action::Commit(label.into(), cmds)));
            }
            Some(Ok(_)) => self.message("Нечего менять"),
            Some(Err(e)) => self.message(e),
            None => {}
        }
    }

    fn inquiry_window(&mut self, ctx: &egui::Context) {
        let Some(i) = &self.inquiry else {
            return;
        };
        let mut open = true;
        egui::Window::new(i.title.clone())
            .id(egui::Id::new("inquiry window"))
            .open(&mut open)
            .collapsible(false)
            .default_width(420.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(420.)
                    .show(ui, |ui| {
                        egui::Grid::new("inquiry grid")
                            .striped(true)
                            .num_columns(i.header.len())
                            .show(ui, |ui| {
                                for h in &i.header {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for row in &i.rows {
                                    for cell in row {
                                        ui.label(cell);
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(note) = &i.note {
                    ui.separator();
                    ui.label(note);
                }
            });
        if !open {
            self.inquiry = None;
        }
    }

    fn document_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let Some(info) = &mut self.document_info else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Сведения о документе")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.)
            .show(ctx, |ui| {
                egui::Grid::new("document fields").num_columns(2).show(ui, |ui| {
                    for (caption, value) in [
                        ("Базовое обозначение", &mut info.base_designation),
                        ("Объект (графа 2)", &mut info.object),
                        ("Здание (графа 3)", &mut info.building),
                        ("Организация (графа 9)", &mut info.organisation),
                        ("Заказчик (графа 27)", &mut info.customer),
                        ("Инв. № подл. (графа 20)", &mut info.inventory_number),
                        ("Взам. инв. № (графа 22)", &mut info.replaced_inventory),
                    ] {
                        let label = ui.label(caption);
                        ui.add(egui::TextEdit::singleline(value).desired_width(280.))
                            .labelled_by(label.id);
                        ui.end_row();
                    }
                    let label = ui.label("Стадия (графа 6)");
                    egui::ComboBox::from_id_salt("document stage")
                        .selected_text(info.stage.code())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut info.stage, open_bim_model::Stage::Working, "Р — рабочая");
                            ui.selectable_value(&mut info.stage, open_bim_model::Stage::Design, "П — проектная");
                        })
                        .response
                        .labelled_by(label.id);
                    ui.end_row();
                });
                ui.separator();
                ui.label("Подписи (графы 10–13), дата ДД.ММ.ГГ");
                egui::Grid::new("document people").num_columns(4).show(ui, |ui| {
                    ui.label("Строка");
                    ui.label("Должность");
                    ui.label("Фамилия");
                    ui.label("Дата");
                    ui.end_row();
                    if info.others.len() < open_bim_model::MAX_OTHER_SIGNERS {
                        info.others
                            .resize(open_bim_model::MAX_OTHER_SIGNERS, Default::default());
                    }
                    let rows: Vec<(&str, &mut open_bim_model::Person, bool)> = [
                        ("Разраб.", &mut info.developer, false),
                        ("Пров.", &mut info.checker, false),
                    ]
                    .into_iter()
                    .chain(info.others.iter_mut().map(|p| ("—", p, true)))
                    .chain([
                        ("Н. контр.", &mut info.norm_control, false),
                        ("Утверждает", &mut info.approver, true),
                    ])
                    .collect();
                    for (row, person, position) in rows {
                        let label = ui.label(row);
                        if position {
                            ui.add(egui::TextEdit::singleline(&mut person.position).desired_width(90.))
                                .labelled_by(label.id);
                        } else {
                            ui.label("");
                        }
                        ui.add(egui::TextEdit::singleline(&mut person.name).desired_width(140.))
                            .labelled_by(label.id);
                        ui.add(egui::TextEdit::singleline(&mut person.date).desired_width(70.))
                            .labelled_by(label.id);
                        ui.end_row();
                    }
                });
                ui.small("Подписи ставятся вручную или электронной подписью; программа выводит фамилии и даты.");
                ui.separator();
                let label = ui.label("Общие указания (лист «Общие данные»), по пункту в строке");
                ui.add(
                    egui::TextEdit::multiline(&mut self.document_notes)
                        .desired_rows(4)
                        .desired_width(440.),
                )
                .labelled_by(label.id);
                apply = ui.button("Применить").clicked();
            });
        if apply {
            let mut info = info.clone();
            info.others.retain(|p| !p.is_empty());
            info.general_notes = self
                .document_notes
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            match info.validate() {
                Ok(()) => {
                    requests.push(Request::Action(Action::Commit(
                        "Сведения о документе".into(),
                        vec![open_bim_commands::Command::SetDocumentInfo {
                            info: Box::new(info),
                        }],
                    )));
                    self.document_info = None;
                }
                Err(e) => self.message(e.to_string()),
            }
        } else if !open {
            self.document_info = None;
        }
    }

    fn grid_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let Some(spec) = &mut self.grid else { return };
        let mut open = true;
        let mut create = false;
        let mut modify = false;
        egui::Window::new("Сетка осей")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("grid fields")
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.label("");
                        ui.label("Координаты, мм");
                        ui.label("Метки");
                        ui.end_row();
                        for (axis, coords, names) in [
                            ("X", &mut spec.x, &mut spec.labels_x),
                            ("Y", &mut spec.y, &mut spec.labels_y),
                            ("Z", &mut spec.z, &mut spec.labels_z),
                        ] {
                            let label = ui.label(axis);
                            ui.add(egui::TextEdit::singleline(coords).desired_width(160.))
                                .labelled_by(label.id);
                            ui.add(egui::TextEdit::singleline(names).desired_width(140.))
                                .labelled_by(label.id);
                            ui.end_row();
                        }
                        let label = ui.label("Удлинение");
                        ui.add(egui::TextEdit::singleline(&mut spec.extension).desired_width(80.))
                            .labelled_by(label.id);
                        ui.end_row();
                    });
                ui.small("«0 4*6000» — начало 0 и четыре шага по 6000 мм");
                ui.horizontal(|ui| {
                    if self.grid_editing && ui.button("Изменить сетку").clicked() {
                        modify = true;
                    }
                    create = ui.button("Создать новую сетку").clicked();
                });
            });
        let result = if modify {
            Some(
                crate::interaction::grid_edit::update_commands(
                    self.editor.document.project(),
                    spec,
                )
                .map(|c| ("Изменить: сетка", c)),
            )
        } else if create {
            Some(
                crate::interaction::tools::grid_commands(self.editor.document.project(), spec)
                    .map(|c| ("Создать: сетка", c)),
            )
        } else {
            None
        };
        if let Some(result) = result {
            match result {
                Ok((label, commands)) => {
                    self.grid = None;
                    requests.push(Request::Action(Action::Commit(label.into(), commands)));
                    requests.push(Request::Fit);
                }
                Err(e) => self.message(e),
            }
        } else if !open {
            self.grid = None;
        }
    }

    /// Typed coordinates for the next pick.
    pub fn pick_numeric(&mut self, text: &str) -> Option<Request> {
        // A typed point replaces the keyboard cursor as the reference.
        self.kb_cursor = None;
        // A grid node («Б/2», «Б/2/+3,600») is an exact pick of that intersection.
        // «К1000,0» is the R mode on the Russian layout, not a grid node.
        if let Some(node) = crate::interaction::grid_edit::grid_node(
            self.editor.document.project(),
            text,
            self.engine.workplane.origin.z,
        )
        .filter(|n| n.is_ok() || crate::interaction::numeric::mode_prefix(text).is_none())
        {
            return match node {
                Ok(point) => {
                    self.engine.numeric = None;
                    self.engine.error = None;
                    self.pick(point)
                }
                Err(e) => {
                    self.engine.error = Some(e.clone());
                    self.message(e);
                    None
                }
            };
        }
        // Fitting/cutting/construction and bolt commands take typed points too.
        if self.trim.tool.is_some() || self.detail.picks_points() {
            let previous = self
                .trim
                .points
                .last()
                .or(self.detail.points.last())
                .copied();
            let direction = previous.zip(self.cursor).map(|(p, c)| c - p);
            return match crate::interaction::numeric::parse(
                text,
                &self.engine.workplane,
                previous,
                direction,
            ) {
                Ok(point) => {
                    self.engine.numeric = None;
                    self.engine.error = None;
                    self.pick(point)
                }
                Err(e) => {
                    self.engine.error = Some(e.clone());
                    self.message(e);
                    None
                }
            };
        }
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

    /// The point the next pick starts from (direction lock, X/Y/Z locks).
    pub fn previous_pick(&self) -> Option<open_bim_renderer::DVec3> {
        self.engine
            .reference
            .or(self.engine.picks.last().copied())
            .or(self.edit_pick.points.last().copied())
            .or(self.detail.points.last().copied())
            .or(self.trim.points.last().copied())
            .or(self
                .workplane_picks
                .as_ref()
                .and_then(|p| p.last().copied()))
            .or(self.measure.as_ref().and_then(|p| p.last().copied()))
            .or(self.angle_picks.as_ref().and_then(|p| p.last().copied()))
    }

    /// X, Y, Z while picking: lock that work plane coordinate at the
    /// point under the pointer (or the previous pick); the same key releases it.
    /// Locks end with the command.
    fn lock_keys(&mut self, ctx: &egui::Context) {
        if !self.picking() {
            self.locks = Default::default();
            self.snap_override = None;
            return;
        }
        if self.engine.numeric.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        for (k, key, chars) in [
            (0, egui::Key::X, ['x', 'X', 'ч', 'Ч']),
            (1, egui::Key::Y, ['y', 'Y', 'н', 'Н']),
            (2, egui::Key::Z, ['z', 'Z', 'я', 'Я']),
        ] {
            if !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                continue;
            }
            // The typed letter must not open the coordinate box.
            ctx.input_mut(|i| {
                i.events.retain(
                    |e| !matches!(e, egui::Event::Text(t) if t.chars().all(|c| chars.contains(&c))),
                )
            });
            let Some(at) = self.cursor.or(self.previous_pick()) else {
                self.message("Наведите курсор на точку, затем нажмите X, Y или Z");
                continue;
            };
            let name = ["X", "Y", "Z"][k];
            let on = self.locks.toggle(k, at, &self.engine.workplane);
            self.message(if on {
                format!(
                    "Координата {name} зафиксирована: {:.1} мм (повторное нажатие {name} — снять)",
                    self.locks.coordinates[k].unwrap_or_default()
                )
            } else {
                format!("Фиксация {name} снята")
            });
        }
    }

    /// Typing a digit, «@» or «-» during a pick opens the coordinate box.
    fn numeric_input(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let trim_points = matches!(
            self.trim.need(),
            Some(crate::interaction::trim::Need::Point | crate::interaction::trim::Need::Polygon)
        ) || self.detail.picks_points();
        if !self.engine.active() && !trim_points {
            self.engine.numeric = None;
            return;
        }
        if self.engine.numeric.is_none() && !ctx.egui_wants_keyboard_input() {
            // The opening character goes into the box once: take it out of the
            // events so the newly focused field does not type it again.
            let start = ctx.input_mut(|i| {
                let at = i.events.iter().position(|e| {
                    matches!(e, egui::Event::Text(t) if t.starts_with(|c: char| {
                        c.is_ascii_digit()
                            || matches!(c, '@' | '-' | '$' | '!')
                            // Mode letters R, A, G.
                            || matches!(c, 'R' | 'r' | 'A' | 'a' | 'G' | 'g')
                            // Cyrillic letters start a grid node («Б/2»).
                            || ('А'..='я').contains(&c)
                    }))
                })?;
                match i.events.remove(at) {
                    egui::Event::Text(t) => Some(t),
                    _ => None,
                }
            });
            if let Some(text) = start {
                let id = egui::Id::new("numeric location");
                let end = egui::text::CCursor::new(text.chars().count());
                let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(end)));
                state.store(ctx, id);
                self.engine.numeric = Some(text);
                ctx.memory_mut(|m| m.request_focus(id));
                let pass = ctx.cumulative_pass_nr();
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("numeric opened"), pass));
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
                    ui.label("x,y,z или R x,y,z — от предыдущей точки · $x,y,z или A x,y,z — от начала плоскости · !x,y,z или G x,y,z — глобально · d<угол — полярно · d — по курсору · Б/2 или Б/2/+3,600 — узел сетки");
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut text)
                            .id(egui::Id::new("numeric location"))
                            .desired_width(320.),
                    )
                    .labelled_by(label.id);
                // A new window is laid out invisibly on its first frames and
                // may drop the focus request: hold the focus on the box for a
                // few frames after it opens so the next typed keys land in it.
                let opened: u64 = ui
                    .data(|d| d.get_temp(egui::Id::new("numeric opened")))
                    .unwrap_or_default();
                if !edit.lost_focus()
                    && (ui.ctx().cumulative_pass_nr() <= opened + 3
                        || ui.memory(|m| m.focused().is_none()))
                {
                    edit.request_focus();
                }
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
        self.engine.plate_mode = self.editor.draft.plate_mode;
        self.keytips_input(&ctx, &mut requests);
        self.lock_keys(&ctx);
        // Drawing editor: arrows move the selection 1 mm (Shift: 10 mm).
        if self.mode == Mode::Drawings && !ctx.egui_wants_keyboard_input() {
            let mut by = [0., 0.];
            for (key, d) in [
                (egui::Key::ArrowLeft, [-1., 0.]),
                (egui::Key::ArrowRight, [1., 0.]),
                (egui::Key::ArrowUp, [0., 1.]),
                (egui::Key::ArrowDown, [0., -1.]),
            ] {
                for (mods, step) in [(egui::Modifiers::NONE, 1.), (egui::Modifiers::SHIFT, 10.)] {
                    if ctx.input_mut(|i| i.consume_key(mods, key)) {
                        by = [by[0] + d[0] * step, by[1] + d[1] * step];
                    }
                }
            }
            if by != [0., 0.]
                && let Err(e) = self.drawings.nudge_selected(&mut self.editor, by)
            {
                self.drawings.message = Some(e);
            }
        }
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
        self.autosave_tick(std::time::Instant::now());
        self.job_window(&ctx, &mut requests);
        self.recovery_window(&ctx);
        self.drawings.wizard.show(&ctx, &mut self.editor);
        self.follow_plugins();
        let busy = self.busy();
        if let Some(run) = self.plugins.show(
            &ctx,
            &self.tools.path,
            self.editor.document.project(),
            &self.editor.selected,
            busy,
        ) {
            self.start_plugin(run);
        }
        let export_dir = self.drawings.export_dir.clone();
        if let Some(id) = self
            .drawings
            .manager
            .show(&ctx, &mut self.editor, &export_dir)
        {
            self.mode = Mode::Drawings;
            self.drawings.select(self.editor.document.project(), id);
        }
        self.grid_window(&ctx, &mut requests);
        self.components_window(&ctx);
        self.component_editor_window(&ctx);
        self.document_window(&ctx, &mut requests);
        self.inquiry_window(&ctx);
        self.representation_window(&ctx);
        self.control_window(&ctx, &mut requests);
        self.delete_window(&ctx, &mut requests);
        self.special_window(&ctx, &mut requests);
        self.filter_window(&ctx);
        self.chrome(ui, &mut requests);
        self.sync_selection();
        if self.focus_target.take() == Some(FocusArea::Viewport) {
            // Viewport: no widget keeps focus, so keys drive the view and picks.
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        requests
    }

    fn recovery_window(&mut self, ctx: &egui::Context) {
        let Some(offer) = self.tools.autosave.offer.clone() else {
            return;
        };
        let (mut recover, mut decline) = (false, false);
        egui::Window::new("Восстановление проекта")
            .id(egui::Id::new("recovery window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0., 0.))
            .show(ctx, |ui| {
                ui.label(format!(
                    "Найдено автосохранение {} ({}).",
                    offer.snapshot.display(),
                    offer.age()
                ));
                ui.label(if offer.project_missing {
                    "Файла проекта нет: модель не была сохранена.".to_string()
                } else {
                    format!(
                        "Оно новее файла {}: работа после последнего сохранения не записана.",
                        offer.project.display()
                    )
                });
                ui.horizontal(|ui| {
                    recover = ui.button("Восстановить автосохранение").clicked();
                    decline = ui
                        .button("Открыть файл проекта")
                        .on_hover_text("Автосохранение останется на диске")
                        .clicked();
                });
            });
        if recover {
            self.recover_autosave();
        } else if decline {
            self.decline_autosave();
        }
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
