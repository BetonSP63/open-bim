//! Several engineers on one model through a shared folder: a revision history
//! of whole models, a short save lock, long-lived locks on objects and fields,
//! ID ranges per engineer and a three-way merge on save and on update.
//! Nothing here needs a server; the folder only has to support exclusive file
//! creation and atomic rename (local disks, SMB and NFS shares do).
mod merge;
pub use merge::{Conflict, KEYED_MAPS, Resolution, Unit, changed_units};
pub use open_bim_model::ID_BLOCK;

use open_bim_model::Project;
use open_bim_persistence::PersistenceError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SHARED_MAGIC: &str = "open-bim-shared";
pub const SHARED_FORMAT_VERSION: u32 = 1;
/// A save lock older than this is left by a crashed program and may be taken over.
pub const SAVE_LOCK_STALE: Duration = Duration::from_secs(600);
/// How long a save waits for another engineer's save to finish.
pub const SAVE_LOCK_WAIT: Duration = Duration::from_secs(30);

const MANIFEST: &str = "shared.json";
const HISTORY: &str = "history";
const LOCKS: &str = "locks";
const OBJECT_LOCKS: &str = "locks/objects";
const SAVE_LOCK: &str = "locks/save.lock";
const IDS: &str = "ids";

/// Who holds a lock or made a revision. The same user on the same computer
/// is the same engineer; `pid` and `since` are for messages only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub user: String,
    pub host: String,
    #[serde(default)]
    pub pid: u32,
    /// Seconds since the Unix epoch.
    #[serde(default)]
    pub since: u64,
}

impl Owner {
    pub fn new(user: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            host: host.into(),
            pid: std::process::id(),
            since: now(),
        }
    }
    pub fn same(&self, other: &Owner) -> bool {
        self.user == other.user && self.host == other.host
    }
    fn stamped(&self) -> Self {
        Self {
            pid: std::process::id(),
            since: now(),
            ..self.clone()
        }
    }
}

impl std::fmt::Display for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.user, self.host)
    }
}

/// One saved revision of the shared model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub number: u64,
    pub author: Owner,
    /// Seconds since the Unix epoch.
    pub time: u64,
    pub comment: String,
    /// Objects and fields this revision changed, by lock name.
    #[serde(default)]
    pub changed: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    magic: String,
    format_version: u32,
    head: Revision,
}

/// A lock on an object or field: only its owner may change it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    pub name: String,
    pub owner: Owner,
}

/// IDs `start..end` belong to one engineer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdBlock {
    pub start: u64,
    pub end: u64,
}

impl IdBlock {
    /// Moves the project's ID counter into this block (never backwards).
    pub fn adopt(&self, project: &mut Project) {
        if project.next_id < self.start {
            project.next_id = self.start;
        }
    }
    pub fn exhausted(&self, project: &Project) -> bool {
        project.next_id >= self.end
    }
}

#[derive(Debug)]
pub enum SharingError {
    Io(std::io::Error),
    Persistence(PersistenceError),
    InvalidFolder(String),
    /// Another engineer is saving right now.
    Busy(Owner),
    /// Objects or fields locked by other engineers.
    Locked(Vec<Lock>),
    /// The shared model changed these since the local copy was taken;
    /// get the changes first.
    OutOfDate(Vec<Unit>),
    /// Changed on both sides; choose a side for each and repeat.
    Conflicts(Vec<Conflict>),
    /// The merged model does not hold together (for example a part refers
    /// to a profile someone else deleted).
    InvalidMerge(String),
}

impl From<std::io::Error> for SharingError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<PersistenceError> for SharingError {
    fn from(e: PersistenceError) -> Self {
        Self::Persistence(e)
    }
}

fn list<T>(items: &[T], name: impl Fn(&T) -> String) -> String {
    const SHOWN: usize = 5;
    let mut text = items
        .iter()
        .take(SHOWN)
        .map(name)
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > SHOWN {
        text.push_str(&format!(" и ещё {}", items.len() - SHOWN));
    }
    text
}

impl std::fmt::Display for SharingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Общая папка недоступна: {e}"),
            Self::Persistence(e) => e.fmt(f),
            Self::InvalidFolder(e) => write!(f, "Папка не является общей моделью: {e}"),
            Self::Busy(owner) => write!(f, "Сейчас сохраняет {owner}; повторите позже"),
            Self::Locked(locks) => write!(
                f,
                "Заняты другими: {}",
                list(locks, |l| format!("{} — {}", l.name, l.owner))
            ),
            Self::OutOfDate(units) => write!(
                f,
                "В общей модели есть более новые изменения ({}); сначала получите изменения",
                list(units, Unit::label)
            ),
            Self::Conflicts(conflicts) => write!(
                f,
                "Изменено и у вас, и в общей модели: {}",
                list(conflicts, |c| c.unit.label())
            ),
            Self::InvalidMerge(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for SharingError {}

pub type Result<T> = std::result::Result<T, SharingError>;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn temporary(path: &Path) -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = temporary(path);
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written?;
    if let Some(parent) = path.parent() {
        // Directory sync is not supported by every share; the rename is done.
        let _ = File::open(parent).and_then(|d| d.sync_all());
    }
    Ok(())
}

fn to_json<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec_pretty(value).expect("plain data serialises")
}

/// Creates `path` only if it does not exist; `Ok(false)` if it does.
fn create_exclusive(path: &Path, bytes: &[u8]) -> Result<bool> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            let written = file.write_all(bytes).and_then(|_| file.sync_all());
            if let Err(e) = written {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(e.into());
            }
            Ok(true)
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn read_owner(path: &Path) -> Result<Option<Owner>> {
    // A lock is created empty and filled right after; give the writer a moment.
    let mut attempts = 0;
    loop {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(owner) => return Ok(Some(owner)),
                Err(_) if attempts < 50 => attempts += 1,
                Err(e) => {
                    return Err(SharingError::InvalidFolder(format!(
                        "{}: {e}",
                        path.display()
                    )));
                }
            },
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Removes a lock file only while it still belongs to `owner`.
fn remove_owned(path: &Path, owner: &Owner) -> Result<()> {
    if read_owner(path)?.is_some_and(|o| o.same(owner)) {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

/// The model's base a local copy started from, as received from the folder.
#[derive(Clone, Debug)]
pub struct Checkout {
    pub revision: Revision,
    pub project: Project,
}

/// Result of getting the shared changes into a local copy.
#[derive(Clone, Debug)]
pub struct Updated {
    /// New base: the shared head that was merged.
    pub base: Checkout,
    /// Local copy with the shared changes and the local edits.
    pub project: Project,
}

/// Result of a save into the shared folder.
#[derive(Clone, Debug)]
pub struct Published {
    /// New base; its project is the saved model. When nothing local
    /// differed from the shared head, this is the head and nothing was written.
    pub base: Checkout,
    pub written: bool,
}

/// A shared model folder.
#[derive(Clone, Debug)]
pub struct SharedModel {
    root: PathBuf,
}

/// Holds the save lock while alive.
struct SaveGuard<'a> {
    path: PathBuf,
    owner: &'a Owner,
}
impl Drop for SaveGuard<'_> {
    fn drop(&mut self) {
        let _ = remove_owned(&self.path, self.owner);
    }
}

impl SharedModel {
    /// Makes `root` (missing or empty) a shared model starting from `project`.
    /// The project's own IDs stay in the first range, so it must use fewer
    /// than `ID_BLOCK` IDs.
    pub fn create(
        root: impl AsRef<Path>,
        project: &Project,
        author: &Owner,
        comment: &str,
    ) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
            return Err(SharingError::InvalidFolder(
                "папка для новой общей модели должна быть пустой".into(),
            ));
        }
        if project.next_id >= ID_BLOCK {
            return Err(SharingError::InvalidFolder(
                "в модели слишком много ID для общей работы".into(),
            ));
        }
        project
            .validate()
            .map_err(|e| SharingError::InvalidMerge(e.to_string()))?;
        for dir in [HISTORY, OBJECT_LOCKS, IDS] {
            std::fs::create_dir_all(root.join(dir))?;
        }
        let model = Self { root };
        create_exclusive(&model.root.join(IDS).join("0.json"), &to_json(author))?;
        let revision = Revision {
            number: 1,
            author: author.stamped(),
            time: now(),
            comment: comment.into(),
            changed: Vec::new(),
        };
        model.write_revision(&revision, project)?;
        Ok(model)
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let model = Self {
            root: root.as_ref().to_path_buf(),
        };
        model.head()?;
        Ok(model)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn history_path(&self, number: u64, extension: &str) -> PathBuf {
        self.root
            .join(HISTORY)
            .join(format!("{number:06}.{extension}"))
    }

    fn lock_path(&self, name: &str) -> PathBuf {
        self.root.join(OBJECT_LOCKS).join(format!("{name}.lock"))
    }

    /// History files are written once and never changed; the manifest
    /// is replaced last, so a reader always sees a complete revision.
    fn write_revision(&self, revision: &Revision, project: &Project) -> Result<()> {
        let model = self.history_path(revision.number, "obim");
        if model.exists() {
            return Err(SharingError::InvalidFolder(format!(
                "ревизия {} уже записана",
                revision.number
            )));
        }
        open_bim_persistence::save_atomic(&model, project)?;
        write_atomic(
            &self.history_path(revision.number, "json"),
            &to_json(revision),
        )?;
        write_atomic(
            &self.root.join(MANIFEST),
            &to_json(&Manifest {
                magic: SHARED_MAGIC.into(),
                format_version: SHARED_FORMAT_VERSION,
                head: revision.clone(),
            }),
        )
    }

    /// The latest revision.
    pub fn head(&self) -> Result<Revision> {
        let bytes = std::fs::read(self.root.join(MANIFEST))?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|e| SharingError::InvalidFolder(e.to_string()))?;
        if manifest.magic != SHARED_MAGIC {
            return Err(SharingError::InvalidFolder("неизвестная сигнатура".into()));
        }
        if manifest.format_version != SHARED_FORMAT_VERSION {
            return Err(SharingError::InvalidFolder(format!(
                "версия {} не поддерживается",
                manifest.format_version
            )));
        }
        Ok(manifest.head)
    }

    /// The model of one revision.
    pub fn load(&self, number: u64) -> Result<Project> {
        Ok(open_bim_persistence::load(
            self.history_path(number, "obim"),
        )?)
    }

    /// The latest revision with its model: the base for a local copy.
    pub fn checkout(&self) -> Result<Checkout> {
        let revision = self.head()?;
        let project = self.load(revision.number)?;
        Ok(Checkout { revision, project })
    }

    /// All revisions, oldest first.
    pub fn history(&self) -> Result<Vec<Revision>> {
        let mut revisions = Vec::new();
        for entry in std::fs::read_dir(self.root.join(HISTORY))? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let bytes = std::fs::read(&path)?;
                revisions.push(
                    serde_json::from_slice::<Revision>(&bytes)
                        .map_err(|e| SharingError::InvalidFolder(e.to_string()))?,
                );
            }
        }
        revisions.sort_by_key(|r| r.number);
        Ok(revisions)
    }

    /// Reserves a fresh ID range for `owner`; ranges are never reused.
    pub fn claim_id_block(&self, owner: &Owner) -> Result<IdBlock> {
        for index in 1..u64::MAX / ID_BLOCK {
            let path = self.root.join(IDS).join(format!("{index}.json"));
            if create_exclusive(&path, &to_json(&owner.stamped()))? {
                return Ok(IdBlock {
                    start: index * ID_BLOCK,
                    end: (index + 1) * ID_BLOCK,
                });
            }
        }
        Err(SharingError::InvalidFolder("исчерпаны диапазоны ID".into()))
    }

    /// Every lock in the folder.
    pub fn locks(&self) -> Result<Vec<Lock>> {
        let mut locks = Vec::new();
        for entry in std::fs::read_dir(self.root.join(OBJECT_LOCKS))? {
            let path = entry?.path();
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".lock"))
            else {
                continue;
            };
            if let Some(owner) = read_owner(&path)? {
                locks.push(Lock {
                    name: name.into(),
                    owner,
                });
            }
        }
        locks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(locks)
    }

    /// Locks other engineers hold on these units: changing them is refused.
    pub fn blocked(&self, owner: &Owner, units: &BTreeSet<Unit>) -> Result<Vec<Lock>> {
        let names: BTreeSet<String> = units.iter().map(Unit::lock_name).collect();
        let mut blocked = Vec::new();
        for name in names {
            if let Some(other) = read_owner(&self.lock_path(&name))?
                && !other.same(owner)
            {
                blocked.push(Lock { name, owner: other });
            }
        }
        Ok(blocked)
    }

    /// Takes locks on all `units` or on none. Refused when another engineer
    /// holds any of them, or when the shared model changed any of them after
    /// `base` (the local copy would overwrite that change).
    pub fn acquire(&self, owner: &Owner, base: &Checkout, units: &BTreeSet<Unit>) -> Result<()> {
        let head = self.head()?;
        if head.number != base.revision.number {
            let theirs =
                merge::units(&self.load(head.number)?).map_err(SharingError::InvalidMerge)?;
            let mine = merge::units(&base.project).map_err(SharingError::InvalidMerge)?;
            let stale: Vec<Unit> = units
                .iter()
                .filter(|u| mine.get(*u) != theirs.get(*u))
                .cloned()
                .collect();
            if !stale.is_empty() {
                return Err(SharingError::OutOfDate(stale));
            }
        }
        let names: BTreeSet<String> = units.iter().map(Unit::lock_name).collect();
        let stamped = to_json(&owner.stamped());
        let mut taken = Vec::new();
        let mut refused = Vec::new();
        for name in names {
            let path = self.lock_path(&name);
            if create_exclusive(&path, &stamped)? {
                taken.push(path);
            } else if let Some(other) = read_owner(&path)?
                && !other.same(owner)
            {
                refused.push(Lock { name, owner: other });
            }
        }
        if !refused.is_empty() {
            for path in taken {
                remove_owned(&path, owner)?;
            }
            return Err(SharingError::Locked(refused));
        }
        Ok(())
    }

    /// Gives back the owner's locks on these units.
    pub fn release(&self, owner: &Owner, units: &BTreeSet<Unit>) -> Result<()> {
        let names: BTreeSet<String> = units.iter().map(Unit::lock_name).collect();
        for name in names {
            remove_owned(&self.lock_path(&name), owner)?;
        }
        Ok(())
    }

    /// Gives back every lock the owner holds.
    pub fn release_all(&self, owner: &Owner) -> Result<()> {
        for lock in self.locks()? {
            if lock.owner.same(owner) {
                remove_owned(&self.lock_path(&lock.name), owner)?;
            }
        }
        Ok(())
    }

    /// Removes someone else's lock (an engineer who left without giving it
    /// back). The program asks for confirmation before calling this.
    pub fn force_release(&self, name: &str) -> Result<()> {
        match std::fs::remove_file(self.lock_path(name)) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    fn save_lock<'a>(&self, owner: &'a Owner, wait: Duration) -> Result<SaveGuard<'a>> {
        std::fs::create_dir_all(self.root.join(LOCKS))?;
        let path = self.root.join(SAVE_LOCK);
        let started = Instant::now();
        loop {
            if create_exclusive(&path, &to_json(&owner.stamped()))? {
                return Ok(SaveGuard { path, owner });
            }
            if let Some(other) = read_owner(&path)? {
                if now().saturating_sub(other.since) > SAVE_LOCK_STALE.as_secs() {
                    // Renaming first makes only one engineer win the takeover.
                    let stale = temporary(&path);
                    if std::fs::rename(&path, &stale).is_ok() {
                        let _ = std::fs::remove_file(&stale);
                    }
                    continue;
                }
                if started.elapsed() >= wait {
                    return Err(SharingError::Busy(other));
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Brings the shared changes into the local copy `mine`, edited from `base`.
    pub fn update(
        &self,
        base: &Checkout,
        mine: &Project,
        resolutions: &BTreeMap<Unit, Resolution>,
    ) -> Result<Updated> {
        let head = self.checkout()?;
        let merged = merge::merge(&base.project, mine, &head.project, resolutions)
            .map_err(SharingError::InvalidMerge)?
            .map_err(SharingError::Conflicts)?;
        let project = if merged.from_mine.is_empty() {
            Project {
                next_id: mine.next_id,
                ..head.project.clone()
            }
        } else {
            merged.project
        };
        Ok(Updated {
            base: head,
            project,
        })
    }

    /// Saves the local copy `mine`, edited from `base`, as a new revision.
    /// Changes of other engineers saved since `base` are kept; objects
    /// locked by others must not have changed locally.
    pub fn publish(
        &self,
        owner: &Owner,
        base: &Checkout,
        mine: &Project,
        comment: &str,
        resolutions: &BTreeMap<Unit, Resolution>,
    ) -> Result<Published> {
        self.publish_waiting(owner, base, mine, comment, resolutions, SAVE_LOCK_WAIT)
    }

    /// `publish` with a chosen wait for another engineer's save.
    pub fn publish_waiting(
        &self,
        owner: &Owner,
        base: &Checkout,
        mine: &Project,
        comment: &str,
        resolutions: &BTreeMap<Unit, Resolution>,
        wait: Duration,
    ) -> Result<Published> {
        let _guard = self.save_lock(owner, wait)?;
        let head = self.checkout()?;
        let merged = merge::merge(&base.project, mine, &head.project, resolutions)
            .map_err(SharingError::InvalidMerge)?
            .map_err(SharingError::Conflicts)?;
        if merged.from_mine.is_empty() {
            return Ok(Published {
                base: head,
                written: false,
            });
        }
        let blocked = self.blocked(owner, &merged.from_mine)?;
        if !blocked.is_empty() {
            return Err(SharingError::Locked(blocked));
        }
        let changed: BTreeSet<String> = merged.from_mine.iter().map(Unit::lock_name).collect();
        let revision = Revision {
            number: head.revision.number + 1,
            author: owner.stamped(),
            time: now(),
            comment: comment.into(),
            changed: changed.into_iter().collect(),
        };
        self.write_revision(&revision, &merged.project)?;
        Ok(Published {
            base: Checkout {
                revision,
                project: merged.project,
            },
            written: true,
        })
    }
}
