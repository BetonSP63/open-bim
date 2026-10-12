use crate::{MAX_JOURNAL_BYTES, MAX_PROJECT_BYTES, PersistenceError, decode, encode, read_bounded};
use open_bim_model::Project;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 8] = b"OBIMAUTO";
const COMMIT: &[u8; 8] = b"COMMIT1\n";

#[derive(Debug)]
pub struct Recovery {
    pub project: Project,
    pub checkpoints: usize,
    pub discarded_tail_bytes: usize,
}

/// A complete validated snapshot is appended only after a model transaction commits.
pub fn append_checkpoint(
    path: impl AsRef<Path>,
    project: &Project,
) -> Result<(), PersistenceError> {
    let bytes = encode(project)?;
    let path = path.as_ref();
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)?;
    file.lock()?;
    let current = file.metadata()?.len();
    let record_len = 48 + bytes.len() as u64;
    if current + record_len + 8 > MAX_JOURNAL_BYTES {
        return Err(PersistenceError::LimitExceeded);
    }
    if current == 0 {
        file.write_all(MAGIC)?;
    } else {
        use std::io::{Read, Seek, SeekFrom};
        file.seek(SeekFrom::Start(0))?;
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(PersistenceError::InvalidFormat(
                "Неверная сигнатура автосохранения".into(),
            ));
        }
        if current > 8 && recover_checkpoints(path)?.discarded_tail_bytes > 0 {
            return Err(PersistenceError::InvalidFormat(
                "Журнал имеет незавершённый хвост; восстановите проект и начните новый журнал"
                    .into(),
            ));
        }
    }
    let mut record = Vec::with_capacity(record_len as usize);
    record.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    record.extend_from_slice(&Sha256::digest(&bytes));
    record.extend_from_slice(&bytes);
    record.extend_from_slice(COMMIT);
    file.write_all(&record)?;
    file.sync_all()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn recover_checkpoints(path: impl AsRef<Path>) -> Result<Recovery, PersistenceError> {
    let bytes = read_bounded(path.as_ref(), MAX_JOURNAL_BYTES)?;
    if bytes.len() < 8 || &bytes[..8] != MAGIC {
        return Err(PersistenceError::InvalidFormat(
            "Неверный журнал автосохранения".into(),
        ));
    }
    let mut offset = 8;
    let mut project = None;
    let mut checkpoints = 0;
    while offset < bytes.len() {
        if bytes.len() - offset < 40 {
            break;
        }
        let mut length = [0u8; 8];
        length.copy_from_slice(&bytes[offset..offset + 8]);
        let length = u64::from_le_bytes(length);
        if length > MAX_PROJECT_BYTES {
            return Err(PersistenceError::LimitExceeded);
        }
        let total = 48 + length as usize;
        if bytes.len() - offset < total {
            break;
        }
        let payload = &bytes[offset + 40..offset + 40 + length as usize];
        if &bytes[offset + 40 + length as usize..offset + total] != COMMIT {
            return Err(PersistenceError::InvalidFormat(
                "Повреждён маркер завершения автосохранения".into(),
            ));
        }
        if Sha256::digest(payload).as_slice() != &bytes[offset + 8..offset + 40] {
            return Err(PersistenceError::ChecksumMismatch);
        }
        project = Some(decode(payload)?);
        checkpoints += 1;
        offset += total;
    }
    Ok(Recovery {
        project: project.ok_or_else(|| {
            PersistenceError::InvalidFormat("Нет завершённых контрольных точек".into())
        })?,
        checkpoints,
        discarded_tail_bytes: bytes.len() - offset,
    })
}
