use super::{
    collect::{Inventory, entry},
    report::{Edge, Report},
};
use crate::{features::parsing::helpers::resource_paths::attribute, shared::models::Source};

pub(super) fn scan(inventory: &Inventory<'_>, source: &Source, report: &mut Report) {
    if source.path.ends_with("plugin.cfg") {
        plugin(inventory, source, report);
        return;
    }
    for reference in &source.references {
        let kind = match reference.item.as_str() {
            section if section.starts_with("[gd_scene") || section.starts_with("[gd_resource") => {
                // The header's UID identifies the file.
                if reference.target.starts_with("uid://") {
                    continue;
                }
                "serialized_resource"
            }
            section if section.starts_with("[ext_resource ") => continue,
            section if section.starts_with("autoload:") => "autoload_registration",
            "[editor_plugins]" if source.path == "project.godot" => "plugin_registration",
            _ if source.path == "project.godot" => "project_setting",
            _ => "serialized_resource",
        };
        inventory.resource(entry(source, reference, kind), reference.directory, report);
    }
    for (row, line) in source.text.lines().enumerate() {
        if line.trim_start().starts_with("[ext_resource ") {
            external(inventory, source, row + 1, line, report);
        }
    }
}

fn plugin(inventory: &Inventory<'_>, source: &Source, report: &mut Report) {
    use crate::features::parsing::helpers::resource_paths::strings;

    let mut is_plugin = false;
    for (row, line) in source.text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            is_plugin = trimmed == "[plugin]";
        }
        if is_plugin
            && let Some((key, value)) = trimmed.split_once('=')
            && key.trim() == "script"
            && value.trim_start().starts_with('"')
            && let Some(path) = strings(value).next()
        {
            let edge = attribute_edge(source, row + 1, line, "plugin_script", path);
            inventory.resource(edge, false, report);
        }
    }
}

fn attribute_edge(source: &Source, row: usize, line: &str, kind: &'static str, raw: &str) -> Edge {
    let start = raw.as_ptr() as usize - line.as_ptr() as usize;
    Edge::new(
        &source.path,
        row,
        Some(line[..start].chars().count() + 1),
        kind,
        raw,
    )
}

fn external(
    inventory: &Inventory<'_>,
    source: &Source,
    row: usize,
    line: &str,
    report: &mut Report,
) {
    let uid = attribute(line, "uid");
    let saved = attribute(line, "path");
    let resolved_uid = uid.and_then(|raw| inventory.resolve(&source.path, raw));
    let resolved_saved = saved.and_then(|raw| inventory.resolve(&source.path, raw));
    let subject = report.subject.path.as_str();
    if source.path != subject
        && resolved_uid.as_deref() != Some(subject)
        && resolved_saved.as_deref() != Some(subject)
        && (uid.is_some() || saved.is_some())
    {
        return;
    }
    if let Some(uid) = uid
        && resolved_uid
            .as_deref()
            .is_some_and(|path| inventory.exists(path))
    {
        let mut edge = attribute_edge(source, row, line, "external_resource", uid);
        edge.target = resolved_uid.clone();
        if let Some(saved) = saved {
            if resolved_saved == resolved_uid {
                edge.aliases.push(saved.into());
            } else {
                inventory.resource(
                    attribute_edge(source, row, line, "external_resource_saved_path", saved),
                    false,
                    report,
                );
            }
        }
        report.direct(edge);
    } else if let Some(saved) = saved {
        let mut edge = attribute_edge(source, row, line, "external_resource", saved);
        if let Some(uid) = uid {
            edge.aliases.push(uid.into());
            edge.reason = Some("UID is unknown; using the saved path");
        }
        inventory.resource(edge, false, report);
    } else if let Some(uid) = uid {
        inventory.resource(
            attribute_edge(source, row, line, "external_resource", uid),
            false,
            report,
        );
    } else {
        report.gap(
            Edge::new(&source.path, row, None, "external_resource", line.trim()),
            "External resource has no readable UID or path",
        );
    }
}
