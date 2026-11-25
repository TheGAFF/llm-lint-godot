use super::models::Outcome;
use std::{
    fs::{self, Metadata},
    io,
    path::{Component, Path, PathBuf},
};

pub(crate) fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.is_symlink()
}

// The root is already resolved. Missing descendants are valid destinations.
pub(crate) fn guarded_path(root: &Path, relative: &Path) -> Outcome<PathBuf> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err(format!("Expected a path inside {}", root.display()).into());
        };
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if is_link(&metadata) => {
                return Err(format!("Symlink or reparse point: {}", path.display()).into());
            }
            Ok(metadata) if !metadata.is_file() && !metadata.is_dir() => {
                return Err(
                    format!("Expected a regular file or directory: {}", path.display()).into(),
                );
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

pub(crate) fn replace_file(path: &Path, bytes: &[u8]) -> Outcome<()> {
    use io::Write as _;
    let parent = path.parent().ok_or("Missing file directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.persist(path)?;
    Ok(())
}
