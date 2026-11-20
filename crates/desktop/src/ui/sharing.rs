use super::{Request, Workbench};
use crate::sharing_session::SharingSession;
use eframe::egui;
use open_bim_model::Project;
use open_bim_sharing::{Owner, SharedModel};
use std::path::{Path, PathBuf};

pub(super) struct SharingUi {
    open: bool,
    session: Option<SharingSession>,
    folder: String,
    local: String,
    engineer: String,
    comment: String,
    head: Option<u64>,
}

impl Default for SharingUi {
    fn default() -> Self {
        Self {
            open: false,
            session: None,
            folder: String::new(),
            local: String::new(),
            engineer: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "Инженер".into()),
            comment: String::new(),
            head: None,
        }
    }
}

impl SharingUi {
    fn owner(&self) -> Owner {
        let host = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
            .or_else(|| {
                let output = std::process::Command::new("hostname").output().ok()?;
                if !output.status.success() {
                    return None;
                }
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().into())
            })
            .unwrap_or_else(|| "компьютер".into());
        Owner::new(self.engineer.trim(), host)
    }

    pub(super) fn status(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let name = session.folder().file_name()?.to_string_lossy();
        let base = session.base_revision();
        Some(match self.head {
            Some(head) if head > base => format!("{name} · {base}/{head} · есть новые ревизии"),
            Some(head) => format!("{name} · {base}/{head}"),
            None => format!("{name} · ревизия {base}"),
        })
    }

    pub(super) fn resume(&mut self, path: &Path) -> Result<(), String> {
        self.session = None;
        self.head = None;
        if let Some(session) = SharingSession::resume(path, self.owner())? {
            self.folder = session.folder().to_string_lossy().into();
            self.local = path.to_string_lossy().into();
            self.engineer.clone_from(&session.owner.user);
            self.session = Some(session);
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Action {
    Create,
    Open,
    Publish,
    Update,
    Check,
    Release,
}

impl Workbench {
    pub(super) fn toggle_sharing(&mut self) {
        self.sharing.open = !self.sharing.open;
        self.message(if self.sharing.open {
            "Совместная работа: выберите общую папку и локальную копию"
        } else {
            "Окно совместной работы закрыто"
        });
    }

    fn shared_loaded(&mut self, project: Project, label: &str) -> Result<Request, String> {
        let scene = self.editor.replace(project, label)?;
        self.tools
            .autosave
            .mark_saved(self.editor.document.project());
        self.message(label);
        Ok(Request::Loaded(scene))
    }

    fn sharing_action(&mut self, action: Action) -> Result<Option<Request>, String> {
        if self.busy() || self.project_change_pending() || self.close_pending() {
            return Err("Завершите текущую операцию с проектом".into());
        }
        let owner = self.sharing.owner();
        if owner.user.is_empty() {
            return Err("Укажите имя инженера".into());
        }
        if matches!(action, Action::Create | Action::Open) {
            let root = PathBuf::from(self.sharing.folder.trim());
            let local = PathBuf::from(self.sharing.local.trim());
            if root.as_os_str().is_empty() || local.as_os_str().is_empty() {
                return Err("Укажите общую папку и путь локальной копии".into());
            }
            if matches!(action, Action::Open) && self.modified() {
                return Err("Сохраните текущий проект перед открытием общей модели".into());
            }
            if local.try_exists().map_err(|error| error.to_string())? {
                return Err("Выберите новый файл локальной копии".into());
            }
            if matches!(action, Action::Create) {
                SharedModel::create(
                    &root,
                    self.editor.document.project(),
                    &owner,
                    "Создание общей модели",
                )
                .map_err(|error| error.to_string())?;
            }
            let (session, project) = SharingSession::open(&root, &local, owner)?;
            let request = self.shared_loaded(project, "Открыта локальная копия общей модели")?;
            self.set_project_path(local);
            self.sharing.head = Some(session.base_revision());
            self.sharing.session = Some(session);
            return Ok(Some(request));
        }
        let session = self
            .sharing
            .session
            .as_mut()
            .ok_or("Откройте общую модель")?;
        session.owner = owner;
        match action {
            Action::Publish | Action::Update => {
                let mine = self.editor.document.project();
                let (project, label) = if matches!(action, Action::Publish) {
                    (
                        session.publish(mine, &self.sharing.comment)?,
                        "Сохранено в общую модель",
                    )
                } else {
                    (session.update(mine)?, "Изменения получены из общей модели")
                };
                self.sharing.head = Some(session.base_revision());
                self.shared_loaded(project, label).map(Some)
            }
            Action::Check => {
                self.sharing.head = Some(session.head_revision()?);
                Ok(None)
            }
            Action::Release => {
                session.release()?;
                self.message("Блокировки освобождены");
                Ok(None)
            }
            Action::Create | Action::Open => unreachable!(),
        }
    }

    pub(super) fn publish_shared(&mut self) -> Option<Request> {
        if self.sharing.session.is_none() || self.sharing.comment.trim().is_empty() {
            self.sharing.open = true;
            self.message("Укажите комментарий к общей ревизии");
            return None;
        }
        match self.sharing_action(Action::Publish) {
            Ok(request) => request,
            Err(error) => {
                self.message(error);
                None
            }
        }
    }

    pub(super) fn sharing_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let mut open = self.sharing.open;
        let mut action = None;
        egui::Window::new("Совместная работа").open(&mut open).show(ctx, |ui| {
            let fields = &mut self.sharing;
            ui.label("Используйте локальную папку или SMB/NFS. Облачная синхронизация не поддерживается.");
            ui.horizontal(|ui| {
                ui.label("Имя инженера");
                ui.text_edit_singleline(&mut fields.engineer);
            });
            ui.horizontal(|ui| {
                ui.label("Общая папка");
                ui.text_edit_singleline(&mut fields.folder);
                if ui.button("Выбрать папку").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                { fields.folder = path.to_string_lossy().into(); }
            });
            ui.horizontal(|ui| {
                ui.label("Локальная копия");
                ui.text_edit_singleline(&mut fields.local);
                if ui.button("Выбрать файл копии").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Проект Open BIM", &["obim"]).save_file()
                { fields.local = super::platform::obim_path(&path).to_string_lossy().into(); }
            });
            ui.horizontal(|ui| {
                if ui.button("Создать общую модель").clicked() { action = Some(Action::Create); }
                if ui.button("Открыть общую модель").clicked() { action = Some(Action::Open); }
            });
            ui.add_enabled_ui(fields.session.is_some(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Комментарий");
                    ui.text_edit_singleline(&mut fields.comment);
                });
                if ui.add_enabled(!fields.comment.trim().is_empty(), egui::Button::new("Сохранить в общую модель")).clicked() {
                    action = Some(Action::Publish);
                }
                ui.horizontal(|ui| {
                    if ui.button("Получить изменения").clicked() { action = Some(Action::Update); }
                    if ui.button("Проверить ревизии").clicked() { action = Some(Action::Check); }
                    if ui.button("Освободить").clicked() { action = Some(Action::Release); }
                });
            });
        });
        self.sharing.open = open;
        if let Some(action) = action {
            match self.sharing_action(action) {
                Ok(Some(request)) => requests.push(request),
                Ok(None) => {}
                Err(error) => self.message(error),
            }
        }
    }
}
