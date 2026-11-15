//! Platform conventions shared by the ribbon, dialogs and shortcut hints.
use std::borrow::Cow;
use std::path::Path;

/// Older instructional strings use Mac symbols. Display readable Windows /
/// Linux key names while keeping the Mac notation on macOS.
pub fn text(value: &str) -> Cow<'_, str> {
    if cfg!(target_os = "macos") {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(
            value
                .replace('⌘', "Ctrl+")
                .replace("⇧ +", "Shift +")
                .replace("⇧ —", "Shift —")
                .replace('⇧', "Shift+")
                .replace('⌥', "Alt")
                .replace('↩', "Enter"),
        )
    }
}

pub fn document_name(path: &str) -> &str {
    // Accept either separator, including files authored on another OS.
    path.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("Новый проект")
}

pub fn document_title(path: &str, modified: bool) -> String {
    format!(
        "{}{} — Open BIM",
        if modified { "* " } else { "" },
        document_name(path)
    )
}

pub fn obim_path(path: &Path) -> std::path::PathBuf {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("obim"))
    {
        path.to_path_buf()
    } else {
        path.with_extension("obim")
    }
}
