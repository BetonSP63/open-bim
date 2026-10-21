//! Autosave next to the project file and recovery after a crash.
//!
//! A snapshot goes to «<файл>.autosave» every few minutes when the model
//! differs both from the saved file and from the previous snapshot. A manual
//! save keeps the previous file as «<файл>.bak» and removes the snapshot, so
//! a snapshot left on disk always means unsaved work. Opening a project whose
//! snapshot is newer than the file offers to restore it.
use open_bim_model::Project;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_MINUTES: u32 = 5;

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// «project.obim» → «project.obim.autosave».
pub fn autosave_path(project: &Path) -> PathBuf {
    with_suffix(project, ".autosave")
}

/// «project.obim» → «project.obim.bak»: the file as it was before the last save.
pub fn backup_path(project: &Path) -> PathBuf {
    with_suffix(project, ".bak")
}

fn hash(project: &Project) -> Result<String, String> {
    open_bim_commands::project_hash(project).map_err(|e| e.to_string())
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A snapshot newer than its project file, found when opening the project.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub project: PathBuf,
    pub snapshot: PathBuf,
    /// The project file is missing: the model was never saved there.
    pub project_missing: bool,
    pub snapshot_time: Option<SystemTime>,
}

impl Offer {
    /// «5 мин назад» for the dialog; exact local time needs no time zone data.
    pub fn age(&self) -> String {
        let Some(time) = self.snapshot_time else {
            return "время неизвестно".into();
        };
        let seconds = SystemTime::now()
            .duration_since(time)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        match seconds {
            0..60 => "меньше минуты назад".into(),
            60..3600 => format!("{} мин назад", seconds / 60),
            3600..86400 => format!("{} ч {} мин назад", seconds / 3600, seconds / 60 % 60),
            _ => format!("{} дн назад", seconds / 86400),
        }
    }
}

#[derive(Debug)]
pub struct Autosave {
    pub enabled: bool,
    /// Interval in minutes (1–60).
    pub minutes: u32,
    last_check: Instant,
    /// Model hash in the project file and in the last snapshot.
    saved: Option<String>,
    snapshot: Option<String>,
    /// When the last snapshot was written (status bar).
    pub last_snapshot: Option<Instant>,
    /// Recovery waiting for the user's answer.
    pub offer: Option<Offer>,
    /// The user chose the project file over this snapshot (path, time).
    declined: Option<(PathBuf, Option<SystemTime>)>,
}

impl Default for Autosave {
    fn default() -> Self {
        Self {
            enabled: true,
            minutes: DEFAULT_MINUTES,
            last_check: Instant::now(),
            saved: None,
            snapshot: None,
            last_snapshot: None,
            offer: None,
            declined: None,
        }
    }
}

impl Autosave {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.minutes.clamp(1, 60)) * 60)
    }

    /// The model now matches its project file (opened, created or saved).
    pub fn mark_saved(&mut self, project: &Project) {
        self.saved = hash(project).ok();
        self.snapshot = None;
        self.last_check = Instant::now();
    }

    /// The model came from a snapshot: unsaved, but already on disk there.
    pub fn mark_recovered(&mut self, project: &Project) {
        self.saved = None;
        self.snapshot = hash(project).ok();
        self.last_check = Instant::now();
    }

    /// The model differs from the project file.
    pub fn modified(&self, project: &Project) -> bool {
        hash(project).ok() != self.saved
    }

    /// Write a snapshot if the interval has passed and the model changed.
    /// Returns the snapshot path when one was written.
    pub fn tick(
        &mut self,
        now: Instant,
        path: &Path,
        project: &Project,
    ) -> Result<Option<PathBuf>, String> {
        if !self.enabled || now.saturating_duration_since(self.last_check) < self.interval() {
            return Ok(None);
        }
        self.last_check = now;
        self.snapshot_now(now, path, project)
    }

    /// Write a snapshot now if the model differs from the file and the last
    /// snapshot (used by the timer and before risky operations).
    pub fn snapshot_now(
        &mut self,
        now: Instant,
        path: &Path,
        project: &Project,
    ) -> Result<Option<PathBuf>, String> {
        let current = hash(project)?;
        if self.saved.as_ref() == Some(&current) || self.snapshot.as_ref() == Some(&current) {
            return Ok(None);
        }
        let target = autosave_path(path);
        open_bim_persistence::save_atomic(&target, project)
            .map_err(|e| format!("Автосохранение не записано: {e}"))?;
        self.snapshot = Some(current);
        self.last_snapshot = Some(now);
        Ok(Some(target))
    }

    /// Manual save: the previous file becomes «.bak», the snapshot is removed.
    pub fn save(&mut self, path: &Path, project: &Project) -> Result<String, String> {
        let mut note = String::new();
        if path.is_file() {
            std::fs::copy(path, backup_path(path))
                .map_err(|e| format!("Не удалось сохранить резервную копию: {e}"))?;
            note = format!(" · прежняя версия: {}", backup_path(path).display());
        }
        open_bim_persistence::save_atomic(path, project).map_err(|e| e.to_string())?;
        let snapshot = autosave_path(path);
        if snapshot.is_file() {
            // The snapshot is now older than the file; a failure only leaves a
            // stale snapshot that the next opening reports as older.
            let _ = std::fs::remove_file(&snapshot);
        }
        self.mark_saved(project);
        self.declined = None;
        Ok(format!("Проект сохранён: {}{note}", path.display()))
    }

    /// A snapshot of `path` newer than the file that the user has not declined.
    pub fn recovery(&self, path: &Path) -> Option<Offer> {
        let snapshot = autosave_path(path);
        let snapshot_time = modified(&snapshot);
        if !snapshot.is_file() {
            return None;
        }
        if self.declined.as_ref() == Some(&(snapshot.clone(), snapshot_time)) {
            return None;
        }
        let project_time = modified(path);
        let newer = match (snapshot_time, project_time) {
            (_, None) => true,
            (Some(s), Some(p)) => s > p,
            (None, Some(_)) => false,
        };
        newer.then(|| Offer {
            project: path.to_path_buf(),
            snapshot,
            project_missing: !path.is_file(),
            snapshot_time,
        })
    }

    /// Open the project file and keep the snapshot on disk; this snapshot is
    /// not offered again in this session.
    pub fn decline(&mut self, offer: &Offer) {
        self.declined = Some((offer.snapshot.clone(), offer.snapshot_time));
        self.offer = None;
    }
}
