//! Application shell shared by the native app and headless UI tests: menus,
//! panels, command registry, palette, help and background jobs. The 3D
//! viewport (GPU) is drawn by the binary inside the central panel.
pub mod chrome;
pub mod commands;
pub mod icons;
pub mod jobs;
pub mod keytips;
pub mod palette;
pub mod property_pane;
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
    /// Zoom to the selected objects (Tekla Shift+Space).
    FitSelected,
    /// Switch the camera and view depth to a model view.
    ApplyView(Box<views::ModelView>),
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
    /// Bolt (two points) and weld (two parts) pick commands.
    pub detail: crate::interaction::tools::DetailPicker,
    /// Open grid dialog («Сетка осей…»).
    pub grid: Option<crate::interaction::tools::GridSpec>,
    /// Fitting, cutting and construction commands in progress.
    pub trim: crate::interaction::trim::TrimPicker,
    /// Part display mode and transparency (Tekla «Отображение»).
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
    /// F6 / Ctrl+F: area of the window that takes keyboard focus next frame.
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
    /// Tekla direct modification (D): drag handles of selected parts.
    pub direct_modification: bool,
    /// Delete waiting for confirmation: ids and the total number removed.
    pub delete_confirm: Option<(BTreeSet<ObjectId>, usize)>,
    /// Tab / Shift+Tab: index into the snap candidates under the cursor.
    pub snap_cycle: usize,
    /// Property pane edit buffer.
    pub pane: property_pane::PropertyPane,
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
            detail: Default::default(),
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
            delete_confirm: None,
            snap_cycle: 0,
            pane: Default::default(),
            views: vec![views::ModelView::model_3d()],
            active_view: 0,
            clip: [None, None],
            contextual: true,
            context_at: None,
            measure: None,
            measured: None,
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
        self.job = Some(ActiveJob::Open(Box::new(job), path));
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
                self.editor.begin(kind);
                self.engine.start(kind);
                self.message(self.engine.prompt());
            }
            Cmd::ToolBolt | Cmd::ToolWeld => {
                self.engine.cancel();
                self.editor.cancel();
                self.mode = Mode::Model;
                let tool = if cmd == Cmd::ToolBolt {
                    crate::interaction::tools::DetailTool::Bolt
                } else {
                    crate::interaction::tools::DetailTool::Weld
                };
                self.detail = crate::interaction::tools::DetailPicker::start(tool);
                // The command's properties show in the side pane (Tekla dialog).
                self.side_panel = Some(chrome::SidePanel::Properties);
                self.message(self.prompt());
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
                // A selected part is taken as the first part, like Tekla.
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
                // An existing grid opens prefilled for editing (Tekla: double-click a grid).
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
                } else if self.trim.cancel() || self.detail.cancel() || self.edit_pick.cancel() {
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
                    // Dependants (holes, bolts, welds, connections): ask first, like Tekla.
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
            Cmd::CycleFocus => {
                self.focus_area = self.focus_area.next();
                self.focus_target = Some(self.focus_area);
                self.message(format!("Фокус: {}", self.focus_area.label()));
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
                all.retain(|id| !self.hidden.contains(id));
                self.message(format!("Выбрано объектов: {}", all.len()));
                self.editor.selected = all;
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
        // The command stays armed for the next object (Tekla repeats it).
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
        if self.trim.need() == Some(crate::interaction::trim::Need::Part) {
            self.trim.parts.push(id);
            if self.trim.complete() {
                return self.finish_trim();
            }
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
    /// switch camera, view depth and work plane (Tekla keeps them per view).
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
        if let Some(node) = crate::interaction::grid_edit::grid_node(
            self.editor.document.project(),
            text,
            self.engine.workplane.origin.z,
        ) {
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
                    ui.label("x,y,z — от предыдущей точки · $x,y,z — от начала плоскости · !x,y,z — глобально · d<угол — полярно · d — по курсору · Б/2 или Б/2/+3,600 — узел сетки");
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
        self.job_window(&ctx, &mut requests);
        self.grid_window(&ctx, &mut requests);
        self.document_window(&ctx, &mut requests);
        self.delete_window(&ctx, &mut requests);
        self.chrome(ui, &mut requests);
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
