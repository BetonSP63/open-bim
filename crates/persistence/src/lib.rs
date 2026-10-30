//! Versioned projects and committed autosave checkpoints; loads never modify input.
mod autosave;
pub use autosave::{Recovery, append_checkpoint, recover_checkpoints};

use open_bim_model::{ModelError, Project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_PROJECT_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_JOURNAL_BYTES: u64 = 1024 * 1024 * 1024;
pub const PROJECT_MAGIC: &str = "open-bim-project";

#[derive(Debug)]
pub enum PersistenceError {
    Io(std::io::Error),
    Model(ModelError),
    InvalidFormat(String),
    UnsupportedVersion(u32),
    LimitExceeded,
    ChecksumMismatch,
    InjectedFailure,
}
impl From<std::io::Error> for PersistenceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<ModelError> for PersistenceError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Не удалось прочитать или записать файл: {e}"),
            Self::Model(e) => e.fmt(f),
            Self::InvalidFormat(e) => {
                write!(f, "Повреждённый или неподдерживаемый файл проекта: {e}")
            }
            Self::UnsupportedVersion(v) => write!(f, "Версия формата {v} не поддерживается"),
            Self::LimitExceeded => f.write_str("Превышен допустимый размер файла или журнала"),
            Self::ChecksumMismatch => f.write_str("Контрольная сумма не совпадает; файл повреждён"),
            Self::InjectedFailure => f.write_str("Тестовый сбой перед завершением сохранения"),
        }
    }
}
impl std::error::Error for PersistenceError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    magic: String,
    format_version: u32,
    payload_sha256: String,
    project: Project,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEnvelope {
    magic: String,
    format_version: u32,
    project: Project,
}
#[derive(Deserialize)]
struct Header {
    magic: String,
    format_version: u32,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn encode(project: &Project) -> Result<Vec<u8>, PersistenceError> {
    let canonical = project.to_json()?;
    let envelope = Envelope {
        magic: PROJECT_MAGIC.into(),
        format_version: FORMAT_VERSION,
        payload_sha256: digest(canonical.as_bytes()),
        project: project.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| PersistenceError::InvalidFormat(e.to_string()))?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(PersistenceError::LimitExceeded);
    }
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<Project, PersistenceError> {
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(PersistenceError::LimitExceeded);
    }
    let header: Header = serde_json::from_slice(bytes)
        .map_err(|e| PersistenceError::InvalidFormat(e.to_string()))?;
    if header.magic != PROJECT_MAGIC {
        return Err(PersistenceError::InvalidFormat(
            "Неизвестная сигнатура".into(),
        ));
    }
    let project = match header.format_version {
        0 => {
            let legacy: LegacyEnvelope = serde_json::from_slice(bytes)
                .map_err(|e| PersistenceError::InvalidFormat(e.to_string()))?;
            if legacy.magic != PROJECT_MAGIC || legacy.format_version != 0 {
                return Err(PersistenceError::InvalidFormat(
                    "Неверный заголовок версии 0".into(),
                ));
            }
            legacy.project
        }
        FORMAT_VERSION => {
            let envelope: Envelope = serde_json::from_slice(bytes)
                .map_err(|e| PersistenceError::InvalidFormat(e.to_string()))?;
            let canonical = envelope.project.to_json()?;
            if digest(canonical.as_bytes()) != envelope.payload_sha256 {
                return Err(PersistenceError::ChecksumMismatch);
            }
            envelope.project
        }
        version => return Err(PersistenceError::UnsupportedVersion(version)),
    };
    project.validate()?;
    Ok(project)
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, PersistenceError> {
    let file = File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(PersistenceError::LimitExceeded);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(PersistenceError::LimitExceeded);
    }
    Ok(bytes)
}

pub fn load(path: impl AsRef<Path>) -> Result<Project, PersistenceError> {
    decode(&read_bounded(path.as_ref(), MAX_PROJECT_BYTES)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFault {
    None,
    AfterCreate,
    AfterWrite,
    AfterSync,
}

pub fn save_atomic(path: impl AsRef<Path>, project: &Project) -> Result<(), PersistenceError> {
    save_atomic_with_fault(path, project, SaveFault::None)
}

/// Explicit fault hook enables durability tests without changing production I/O.
pub fn save_atomic_with_fault(
    path: impl AsRef<Path>,
    project: &Project,
    fault: SaveFault,
) -> Result<(), PersistenceError> {
    let bytes = encode(project)?;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| PersistenceError::InvalidFormat("Не указан файл назначения".into()))?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let mut temporary_name = name.to_os_string();
    temporary_name.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = parent.join(temporary_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(temporary.clone());
    if fault == SaveFault::AfterCreate {
        return Err(PersistenceError::InjectedFailure);
    }
    file.write_all(&bytes)?;
    if fault == SaveFault::AfterWrite {
        return Err(PersistenceError::InjectedFailure);
    }
    file.sync_all()?;
    if fault == SaveFault::AfterSync {
        return Err(PersistenceError::InjectedFailure);
    }
    drop(file);
    std::fs::rename(&temporary, path)?;
    sync_parent(path)?;
    Ok(())
}

/// Flushes the directory entry of `path` so a rename or a new file survives
/// a crash. Windows cannot open a folder as a file and journals NTFS
/// metadata itself, so there the step is skipped.
pub(crate) fn sync_parent(path: &Path) -> Result<(), PersistenceError> {
    if cfg!(windows) {
        return Ok(());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(parent)?.sync_all()?;
    Ok(())
}
