//! A default project belongs to the user, never to the application bundle.
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROJECT_NAME: &str = "Open BIM проект.obim";

fn existing_project(path: &Path) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "путь проекта не является файлом",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Publish a complete example without replacing a project created by another
/// launch. Staging in the destination directory keeps the publish on one volume.
pub fn prepare_project(example: &Path, destination: &Path) -> Result<PathBuf, String> {
    let prepare = || -> io::Result<()> {
        if existing_project(destination)? {
            return Ok(());
        }
        let mut source = File::open(example)?;
        if !source.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "пример не является файлом",
            ));
        }
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut staged = tempfile::Builder::new()
            .prefix(".open-bim-project-")
            .tempfile_in(parent)?;
        io::copy(&mut source, staged.as_file_mut())?;
        staged.as_file_mut().sync_all()?;
        match staged.persist_noclobber(destination) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                // The other process published a complete file first.
                if existing_project(destination)? {
                    Ok(())
                } else {
                    Err(error.error)
                }
            }
            Err(error) => Err(error.error),
        }
    };
    prepare().map_err(|error| {
        format!(
            "Не удалось подготовить проект {}: {error}",
            destination.display()
        )
    })?;
    Ok(destination.to_path_buf())
}

/// An unpackaged development binary without an example can start a new model.
/// An existing user project remains usable even if its bundled example is missing.
pub fn packaged_project(executable: &Path, documents: &Path) -> Result<Option<PathBuf>, String> {
    let destination = documents.join(DEFAULT_PROJECT_NAME);
    if existing_project(&destination)
        .map_err(|error| format!("Не удалось открыть {}: {error}", destination.display()))?
    {
        return Ok(Some(destination));
    }
    let Some(parent) = executable.parent() else {
        return Ok(None);
    };
    let example = parent.join("examples").join("steel-release.obim");
    if !example.try_exists().map_err(|error| error.to_string())? {
        return Ok(None);
    }
    prepare_project(&example, &destination).map(Some)
}
