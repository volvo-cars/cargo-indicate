use std::{fs, path::Path};

/// Ensures the parent directories exists, and if they don't, attempt to create
/// them
pub(crate) fn ensure_parents_exist(path: &Path) -> Result<(), std::io::Error> {
    if let Some(leading_dirs) = path.parent() {
        return fs::create_dir_all(leading_dirs);
    }
    Ok(())
}
