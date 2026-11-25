use super::sources::Snapshot;
use crate::{
    features::configuration::models::Project,
    shared::{
        models::Outcome,
        paths::{guarded_path, replace_file},
    },
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) const DIRECTORY: &str = ".llm-lint/doc-reviews";

fn path(project: &Project, document: &str) -> Outcome<PathBuf> {
    guarded_path(
        &project.root,
        Path::new(&format!("{DIRECTORY}/{document}.json")),
    )
}

pub(super) fn read(project: &Project, document: &str) -> Outcome<Option<Snapshot>> {
    match fs::read(path(project, document)?) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn write(project: &Project, document: &str, record: &Snapshot) -> Outcome<bool> {
    let path = path(project, document)?;
    let mut bytes = serde_json::to_vec_pretty(record)?;
    bytes.push(b'\n');
    if fs::read(&path).is_ok_and(|existing| existing == bytes) {
        return Ok(false);
    }
    let parent = path.parent().ok_or("Missing review record directory")?;
    fs::create_dir_all(parent)?;
    replace_file(&path, &bytes)?;
    Ok(true)
}
