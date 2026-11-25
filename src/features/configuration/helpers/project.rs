use super::super::{
    models::{NativeTool, Project, Settings},
    validators::validate_duplicate_exceptions,
};
use crate::shared::{models::Outcome, paths::guarded_path};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub fn discover_project(start: &Path, explicit: Option<&Path>) -> Outcome<Project> {
    let Project { root, mut settings } = discover_settings(start, explicit)?;
    settings.collection_max_code_lines = super::super::rules::threshold(
        &settings.rules,
        "architecture/collection-size",
        "max",
        settings.collection_max_code_lines,
    );
    native_options(&mut settings)?;
    Ok(Project { root, settings })
}

pub(crate) fn discover_settings(start: &Path, explicit: Option<&Path>) -> Outcome<Project> {
    let (root, config) = project_config(start, explicit)?;
    let settings = toml_edit::de::from_str(&fs::read_to_string(config)?)?;
    configuration_validity(&root, &settings)?;
    Ok(Project { root, settings })
}

pub(crate) fn project_config(start: &Path, explicit: Option<&Path>) -> Outcome<(PathBuf, PathBuf)> {
    let config = if let Some(config) = explicit {
        start.join(config)
    } else {
        start
            .ancestors()
            .map(|root| root.join("llm-lint.godot.toml"))
            .find(|file| file.is_file())
            .ok_or("No llm-lint.godot.toml found")?
    };
    let root = fs::canonicalize(config.parent().ok_or("Configuration has no parent")?)?;
    let config = guarded_path(
        &root,
        Path::new(config.file_name().ok_or("Missing configuration filename")?),
    )?;
    if !guarded_path(&root, Path::new("project.godot"))?.is_file() {
        return Err("Configuration must be beside project.godot".into());
    }
    Ok((root, config))
}

pub(in crate::features::configuration) fn configuration_validity(
    root: &Path,
    settings: &Settings,
) -> Outcome<()> {
    super::super::rules::rules_validity(&settings.rules, &settings.overrides)?;
    crate::features::architecture::helpers::file_roles::validity(&settings.asset_classifications)?;
    crate::features::architecture::helpers::documents::validity(
        &settings.documentation_exclusions,
    )?;
    if settings.version != 1 || settings.collection_max_code_lines == 0 {
        return Err("Unsupported policy version or zero collection limit".into());
    }
    let roots: Vec<_> = settings
        .source_roots
        .iter()
        .chain(&settings.plugin_roots)
        .chain(&settings.tooling_roots)
        .collect();
    if roots.is_empty() {
        return Err("At least one authored source root is required".into());
    }
    for (index, value) in roots.iter().enumerate() {
        safe_path(value)?;
        if !guarded_path(root, Path::new(value))?.is_dir() {
            return Err(format!("Missing source root: {value}").into());
        }
        for other in &roots[..index] {
            if inside(value, other) || inside(other, value) {
                return Err("Authored roots must not overlap".into());
            }
        }
    }
    crate::features::architecture::helpers::bundles::validity(root, settings)?;
    for (path, reason, optional) in settings
        .excluded
        .iter()
        .map(|entry| (&entry.path, &entry.reason, entry.optional))
        .chain(
            settings
                .entrypoints
                .iter()
                .map(|entry| (&entry.path, &entry.reason, false)),
        )
    {
        safe_path(path)?;
        if reason.trim().is_empty() || (!optional && !guarded_path(root, Path::new(path))?.exists())
        {
            return Err(format!("Missing path or reason: {path}").into());
        }
    }
    validate_dynamic_loads(root, settings)?;
    let mut exceptions = BTreeSet::new();
    for exception in &settings.exceptions {
        safe_path(&exception.path)?;
        if !exception.rule.starts_with("architecture/")
            || crate::features::architecture::helpers::bundles::is_layout_rule(&exception.rule)
            || exception.item.is_empty()
            || exception.reason.trim().is_empty()
            || !exceptions.insert((&exception.rule, &exception.path, &exception.item))
        {
            return Err("Invalid or repeated exact exception".into());
        }
    }
    let duplicates = &settings.duplicates;
    validate_duplicate_exceptions(settings)?;
    validate_unused(root, settings)?;
    if duplicates.warning_tokens == 0
        || duplicates.warning_lines == 0
        || duplicates.error_tokens < duplicates.warning_tokens
        || duplicates.error_lines < duplicates.warning_lines
        || !(0.0..1.0).contains(&duplicates.similarity)
        || duplicates.similarity == 0.0
    {
        return Err("Invalid duplicate thresholds".into());
    }
    validate_native(settings)
}

fn validate_dynamic_loads(root: &Path, settings: &Settings) -> Outcome<()> {
    let mut keys = BTreeSet::new();
    for load in &settings.dynamic_loads {
        safe_path(&load.path)?;
        if !guarded_path(root, Path::new(&load.path))?.is_file()
            || load.item.is_empty()
            || load.roots.is_empty()
            || load.reason.trim().is_empty()
            || !keys.insert((&load.path, &load.item))
        {
            return Err("Invalid or repeated dynamic load declaration".into());
        }
        let mut extensions = BTreeSet::new();
        for extension in &load.extensions {
            if extension.is_empty()
                || !extension
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                || !extensions.insert(extension)
            {
                return Err(
                    format!("Invalid or repeated dynamic load extension: {extension}").into(),
                );
            }
        }
        for target in &load.roots {
            let local = target
                .strip_prefix("res://")
                .or_else(|| target.strip_prefix("user://"))
                .ok_or("Dynamic roots must start with res:// or user://")?;
            safe_path(local)?;
            if !target.starts_with("res://") {
                continue;
            }
            let path = guarded_path(root, Path::new(local))?;
            if !path.exists() {
                return Err(format!("Missing dynamic resource root: {target}").into());
            }
            if path.is_file() && !load.matches_extension(local) {
                return Err(format!(
                    "Dynamic resource path is excluded by its extension filter: {target}"
                )
                .into());
            }
        }
    }
    Ok(())
}

fn validate_unused(root: &Path, settings: &Settings) -> Outcome<()> {
    let mut references = BTreeSet::new();
    for path in &settings.unused.reference_files {
        safe_path(path)?;
        if !guarded_path(root, Path::new(path))?.is_file() || !references.insert(path) {
            return Err(format!("Invalid or repeated unused reference file: {path}").into());
        }
    }
    let mut manifests = BTreeSet::new();
    for path in &settings.unused.release_exclusion_files {
        safe_path(path)?;
        if !path.ends_with(".json")
            || !guarded_path(root, Path::new(path))?.is_file()
            || !manifests.insert(path)
        {
            return Err(
                format!("Invalid or repeated unused release exclusion file: {path}").into(),
            );
        }
    }
    let mut keeps = BTreeSet::new();
    for entry in &settings.unused.keep {
        safe_path(&entry.path)?;
        let path = guarded_path(root, Path::new(&entry.path))?;
        let valid_path = entry.item.as_ref().map_or_else(
            || path.exists(),
            |name| !name.trim().is_empty() && path.is_file() && entry.path.ends_with(".gd"),
        );
        if !valid_path
            || entry.reason.trim().is_empty()
            || !keeps.insert((&entry.path, &entry.item))
        {
            return Err(format!("Invalid or repeated unused keep: {}", entry.path).into());
        }
    }
    Ok(())
}

fn validate_native(settings: &Settings) -> Outcome<()> {
    let supported = [
        "max_line_length",
        "max_function_length",
        "max_file_length",
        "max_parameters",
        "max_returns",
        "max_nesting_depth",
        "max_local_variables",
        "max_branches",
        "max_class_variables",
        "max_public_methods",
        "max_inner_classes",
        "large_number_threshold",
    ];
    if settings
        .style
        .iter()
        .any(|(key, value)| !supported.contains(&key.as_str()) || *value == 0)
    {
        return Err("Unknown style setting or zero threshold".into());
    }
    if let Some(tool) = &settings.gdstyle {
        if tool.version == super::native::LEGACY_VERSION {
            return Err(format!(
                "Run godot-llm-lint init to upgrade gdstyle from {} to {}",
                tool.version,
                super::native::VERSION
            )
            .into());
        }
        validate_native_tool(tool, super::native::VERSION)?;
    }
    Ok(())
}

pub(in crate::features::configuration) fn validate_native_tool(
    tool: &NativeTool,
    version: &str,
) -> Outcome<()> {
    if tool.version != version {
        return Err(format!("gdstyle requires version {version}").into());
    }
    if tool.auto {
        if !tool.executable.is_empty() || !tool.sha256.is_empty() {
            return Err("Automatic gdstyle configuration omits executable/sha256".into());
        }
    } else if tool.executable.is_empty() || !super::native::valid_digest(&tool.sha256) {
        return Err("Explicit gdstyle configuration requires an executable and SHA256".into());
    }
    Ok(())
}

pub fn safe_path(path: &str) -> Outcome<()> {
    if path.is_empty()
        || path.contains(['\\', ':', '*', '?'])
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("Expected a project-relative slash path: {path}").into());
    }
    Ok(())
}

pub fn inside(path: &str, root: &str) -> bool {
    root.is_empty()
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn target_paths(project: &Project, targets: &[String]) -> Outcome<Vec<String>> {
    targets
        .iter()
        .map(|target| {
            let candidate = project.root.join(target);
            let resolved = fs::canonicalize(&candidate)?;
            let anchor = candidate.strip_prefix(&project.root).or_else(|_| {
                let root = candidate
                    .ancestors()
                    .find(|path| fs::canonicalize(path).is_ok_and(|path| path == project.root))
                    .ok_or("Target is outside the consumer project")?;
                candidate
                    .strip_prefix(root)
                    .map_err(|_| "Invalid target path")
            })?;
            guarded_path(&project.root, anchor)?;
            let relative = resolved
                .strip_prefix(&project.root)
                .map_err(|_| "Target is outside the consumer project")?;
            Ok(relative.to_string_lossy().replace('\\', "/"))
        })
        .collect()
}

fn native_options(settings: &mut Settings) -> Outcome<()> {
    for (id, setting) in &settings.rules {
        if !id.starts_with("gdstyle/") {
            continue;
        }
        let super::super::rules::RuleSetting::Detailed(value) = setting else {
            continue;
        };
        for (key, number) in &value.options {
            settings.style.insert(
                key.clone(),
                usize::try_from(number.as_u64().ok_or("Invalid native threshold")?)?,
            );
        }
    }
    Ok(())
}
