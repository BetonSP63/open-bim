//! Local shared-model copy and its offline merge base.
use open_bim_model::{ID_BLOCK, Project};
use open_bim_sharing::{Checkout, Owner, Revision, SharedModel, Updated};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    folder: PathBuf,
    revision: Revision,
    base: Project,
    id_range: u64,
    engineer: String,
}

/// The authoritative merge base travels with the local copy and is available offline.
pub struct SharingSession {
    local_path: PathBuf,
    link: Link,
    pub owner: Owner,
}

fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".shared.json");
    name.into()
}

fn persist_link(path: &Path, link: &Link, new: bool) -> Result<(), String> {
    let path = sidecar(path);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(file.as_file_mut(), link).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    let result = if new {
        file.persist_noclobber(path)
    } else {
        file.persist(path)
    };
    result.map_err(|e| e.to_string())?;
    Ok(())
}

impl SharingSession {
    pub fn open(folder: &Path, local_path: &Path, owner: Owner) -> Result<(Self, Project), String> {
        if local_path.try_exists().map_err(|e| e.to_string())?
            || sidecar(local_path)
                .try_exists()
                .map_err(|e| e.to_string())?
        {
            return Err("Файл локальной копии уже существует".into());
        }
        let folder = folder.canonicalize().map_err(|e| e.to_string())?;
        let shared = SharedModel::open(&folder).map_err(|e| e.to_string())?;
        let base = shared.checkout().map_err(|e| e.to_string())?;
        let ids = shared.claim_id_block(&owner).map_err(|e| e.to_string())?;
        let project = Project {
            next_id: ids.start,
            ..base.project.clone()
        };
        let link = Link {
            folder,
            revision: base.revision,
            base: base.project,
            id_range: ids.start / ID_BLOCK,
            engineer: owner.user.clone(),
        };

        // Publish the first copy without replacing an existing project.
        let parent = local_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let bytes = open_bim_persistence::encode(&project).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist_noclobber(local_path)
            .map_err(|e| e.to_string())?;
        if let Err(error) = persist_link(local_path, &link, true) {
            // Preserve a usable local project if the current directory won't accept the link.
            return Err(format!(
                "Копия сохранена, но связать её с общей моделью не удалось: {error}"
            ));
        }
        Ok((
            Self {
                local_path: local_path.into(),
                link,
                owner,
            },
            project,
        ))
    }

    pub fn resume(local_path: &Path, mut owner: Owner) -> Result<Option<Self>, String> {
        let bytes = match fs::read(sidecar(local_path)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let link: Link = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        link.base.validate().map_err(|e| e.to_string())?;
        owner.user.clone_from(&link.engineer);
        Ok(Some(Self {
            local_path: local_path.into(),
            link,
            owner,
        }))
    }

    pub fn folder(&self) -> &Path {
        &self.link.folder
    }

    pub fn base_revision(&self) -> u64 {
        self.link.revision.number
    }

    pub fn head_revision(&self) -> Result<u64, String> {
        let shared = SharedModel::open(self.folder()).map_err(|e| e.to_string())?;
        shared.head().map(|r| r.number).map_err(|e| e.to_string())
    }

    fn base(&self, mine: &Project) -> Result<Checkout, String> {
        if mine.next_id / ID_BLOCK != self.link.id_range {
            return Err("Локальная копия использует другой диапазон ID".into());
        }
        Ok(Checkout {
            revision: self.link.revision.clone(),
            project: self.link.base.clone(),
        })
    }

    fn replace_base(&mut self, base: Checkout, project: &Project) -> Result<(), String> {
        let link = Link {
            folder: self.link.folder.clone(),
            revision: base.revision,
            base: base.project,
            id_range: self.link.id_range,
            engineer: self.owner.user.clone(),
        };
        open_bim_persistence::save_atomic(&self.local_path, project).map_err(|e| e.to_string())?;
        persist_link(&self.local_path, &link, false)?;
        self.link = link;
        Ok(())
    }

    pub fn update(&mut self, mine: &Project) -> Result<Project, String> {
        let shared = SharedModel::open(self.folder()).map_err(|e| e.to_string())?;
        let Updated { base, project } = shared
            .update(&self.base(mine)?, mine, &BTreeMap::new())
            .map_err(|e| e.to_string())?;
        self.replace_base(base, &project)?;
        Ok(project)
    }

    pub fn publish(&mut self, mine: &Project, comment: &str) -> Result<Project, String> {
        let shared = SharedModel::open(self.folder()).map_err(|e| e.to_string())?;
        let published = shared
            .publish(
                &self.owner,
                &self.base(mine)?,
                mine,
                comment,
                &BTreeMap::new(),
            )
            .map_err(|e| e.to_string())?;
        let project = Project {
            next_id: mine.next_id,
            ..published.base.project.clone()
        };
        self.replace_base(published.base, &project)?;
        Ok(project)
    }

    pub fn release(&self) -> Result<(), String> {
        SharedModel::open(self.folder())
            .and_then(|shared| shared.release_all(&self.owner))
            .map_err(|e| e.to_string())
    }
}
