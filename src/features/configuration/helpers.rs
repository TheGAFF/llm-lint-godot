pub mod native;
mod project;
pub mod rules;
pub(in crate::features::configuration) use project::{
    configuration_validity, validate_native_tool,
};
pub use project::{discover_project, inside, safe_path, target_paths};
pub(crate) use project::{discover_settings, project_config};

pub mod contracts;
