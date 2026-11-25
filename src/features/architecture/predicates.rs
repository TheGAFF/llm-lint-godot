use super::helpers::file_roles::FileRole;
use crate::features::configuration::{models::Project, rules::matches};

pub(super) fn is_document(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("md"))
}

pub(super) fn is_asset(path: &str, roles: &[FileRole]) -> bool {
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if extension == "md" {
        return false;
    }
    // JSON/CSV/text are fixtures only in testing; tooling manifests and licenses are support files.
    if ["json", "csv", "txt"].contains(&extension.as_str()) {
        return path.split('/').any(|part| part == "testing");
    }
    roles
        .iter()
        .any(|role| role.extensions.contains(&extension))
        || (path.split('/').any(|part| part == "assets")
            && !["import", "uid", "md", "txt", "in"].contains(&extension.as_str()))
}

pub(super) fn is_classified(project: &Project, path: &str) -> bool {
    project
        .settings
        .asset_classifications
        .iter()
        .any(|entry| entry.includes.iter().any(|pattern| matches(pattern, path)))
}
