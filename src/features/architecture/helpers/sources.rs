use super::super::predicates::{is_asset, is_classified, is_document};
use crate::{
    features::{
        configuration::{helpers::inside, models::Project},
        parsing::parsers::{parse_text, parse_tree, script_parser, source_from_tree},
    },
    shared::{
        models::{Corpus, Outcome, Source},
        paths::is_link,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use tree_sitter::Tree;

pub(crate) type ScriptTrees = BTreeMap<String, Tree>;
pub(super) const PRUNED_NAMES: &[&str] = &[
    ".git",
    ".godot",
    ".tools",
    ".build",
    "node_modules",
    "target",
    "export_templates",
];

pub fn corpus(project: &Project) -> Outcome<Corpus> {
    collect_corpus(project, false).map(|(corpus, _)| corpus)
}

pub(crate) fn parsed_corpus(project: &Project) -> Outcome<(Corpus, ScriptTrees)> {
    collect_corpus(project, true)
}

pub(super) fn parse_corpus_scripts(corpus: &Corpus) -> Outcome<ScriptTrees> {
    let mut parser = script_parser()?;
    corpus
        .sources
        .iter()
        .chain(&corpus.excluded_sources)
        .filter(|source| source.path.ends_with(".gd"))
        .map(|source| Ok((source.path.clone(), parse_tree(&mut parser, &source.text)?)))
        .collect()
}

fn collect_corpus(project: &Project, retain_trees: bool) -> Outcome<(Corpus, ScriptTrees)> {
    let (paths, empty_directories) = discover_paths(project)?;
    let mut sources = Vec::new();
    let mut documents = Vec::new();
    let mut excluded_sources = Vec::new();
    let mut uids = BTreeMap::new();
    let mut trees = ScriptTrees::new();
    let mut parser = script_parser()?;
    let file_roles = super::file_roles::catalog()?;
    for path in &paths {
        if !project.root.join(path).is_file() {
            continue;
        }
        let excluded = project
            .settings
            .excluded
            .iter()
            .any(|entry| inside(path, &entry.path));
        if is_document(path) {
            if !excluded && !super::documents::exempt(project, path) {
                let mut document = parse_text(path, String::new());
                document.text = fs::read_to_string(project.root.join(path))?;
                identity(project, &mut document);
                documents.push(document);
            }
            continue;
        }
        if path.ends_with(".uid") {
            let uid = fs::read_to_string(project.root.join(path))?
                .trim()
                .to_owned();
            let script = path.trim_end_matches(".uid").to_owned();
            if uid.starts_with("uid://") && paths.contains(&script) {
                insert_uid(&mut uids, &uid, &script)?;
            }
        }
        if path.ends_with(".import") {
            asset_uid(project, path, &paths, &mut uids)?;
        }
        let extension = path.rsplit('.').next().unwrap_or("");
        if ["uid", "import", "md", "in"].contains(&extension)
            || (extension == "txt" && !path.split('/').any(|part| part == "testing"))
        {
            continue;
        }
        let asset = is_asset(path, &file_roles) || is_classified(project, path);
        let bundle_asset = super::bundles::extra_asset(path)
            && project
                .settings
                .bundles
                .owners
                .iter()
                .any(|owner| inside(path, owner));
        if !asset && !bundle_asset && !["gd", "tscn", "tres", "godot", "cfg"].contains(&extension) {
            continue;
        }
        if extension == "cfg" && !path.ends_with("plugin.cfg") {
            continue;
        }
        let text = if ["gd", "tscn", "tres", "godot", "cfg"].contains(&extension) {
            fs::read_to_string(project.root.join(path))?
        } else {
            String::new()
        };
        let mut source = if extension == "gd" {
            let tree = parse_tree(&mut parser, &text)?;
            let source = source_from_tree(path, text, &tree);
            if retain_trees {
                trees.insert(path.clone(), tree);
            }
            source
        } else {
            parse_text(path, text)
        };
        identity(project, &mut source);
        for line in source.text.lines().filter(|line| line.starts_with("[gd_")) {
            if let Some((_, tail)) = line.split_once("uid=\"")
                && let Some((uid, _)) = tail.split_once('"')
            {
                insert_uid(&mut uids, uid, path)?;
            }
        }
        if excluded {
            excluded_sources.push(source);
        } else {
            sources.push(source);
        }
    }
    Ok((
        Corpus {
            sources,
            documents,
            excluded_sources,
            paths,
            empty_directories,
            uids,
        },
        trees,
    ))
}

fn discover_paths(project: &Project) -> Outcome<(BTreeSet<String>, BTreeSet<String>)> {
    let skipped = project
        .settings
        .excluded
        .iter()
        .filter(|entry| entry.skip_discovery)
        .map(|entry| entry.path.as_str())
        .collect();
    let mut paths = BTreeSet::new();
    let mut empty_directories = BTreeSet::new();
    walk(
        &project.root,
        &project.root,
        &skipped,
        &mut paths,
        &mut empty_directories,
    )?;
    Ok((paths, empty_directories))
}

fn asset_uid(
    project: &Project,
    path: &str,
    paths: &BTreeSet<String>,
    uids: &mut BTreeMap<String, String>,
) -> Outcome<()> {
    let asset = path.trim_end_matches(".import");
    for line in fs::read_to_string(project.root.join(path))?.lines() {
        if let Some(uid) = line
            .trim()
            .strip_prefix("uid=\"")
            .and_then(|tail| tail.strip_suffix('"'))
            && paths.contains(asset)
        {
            insert_uid(uids, uid, asset)?;
        }
    }
    Ok(())
}

fn insert_uid(uids: &mut BTreeMap<String, String>, uid: &str, path: &str) -> Outcome<()> {
    if let Some(previous) = uids.insert(uid.into(), path.into())
        && previous != path
    {
        return Err(format!("Duplicate UID {uid}: {previous} and {path}").into());
    }
    Ok(())
}

fn walk(
    root: &Path,
    directory: &Path,
    skipped: &BTreeSet<&str>,
    paths: &mut BTreeSet<String>,
    empty_directories: &mut BTreeSet<String>,
) -> Outcome<bool> {
    let mut empty = true;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if PRUNED_NAMES.contains(&name.as_str()) {
            empty = false;
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&path)?;
        empty &= metadata.is_file() && [".gitkeep", ".keep"].contains(&name.as_str());
        if relative == super::document_content::RECORD_DIRECTORY
            || skipped.contains(relative.as_str())
        {
            continue;
        }
        if is_link(&metadata) {
            return Err(format!("Symlink inside project corpus: {}", path.display()).into());
        }
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(format!("Expected a regular project file: {}", path.display()).into());
        }
        if metadata.is_dir() && walk(root, &path, skipped, paths, empty_directories)? {
            empty_directories.insert(relative.clone());
        }
        paths.insert(relative);
    }
    Ok(empty)
}

pub fn identity(project: &Project, source: &mut Source) {
    let path = &source.path;
    if path == "project.godot" {
        source.owner = "project".into();
        source.layer = "app".into();
        return;
    }
    for root in &project.settings.source_roots {
        let Some(local) = path.strip_prefix(&format!("{root}/")) else {
            continue;
        };
        let parts: Vec<_> = local.split('/').collect();
        match parts.as_slice() {
            ["features", name, ..] => {
                source.owner = format!("{root}/features/{name}");
                source.layer = "feature".into();
            }
            [layer @ ("shared" | "app"), ..] => {
                source.owner = format!("{root}/{layer}");
                source.layer = (*layer).into();
            }
            _ => {
                source.owner = root.clone();
                source.layer = "unregistered".into();
            }
        }
        return;
    }
    for root in &project.settings.plugin_roots {
        if inside(path, root) {
            source.owner = root.clone();
            source.layer = "editor".into();
            return;
        }
    }
    for root in &project.settings.tooling_roots {
        if inside(path, root) {
            source.owner = root.clone();
            source.layer = "tooling".into();
            return;
        }
    }
    source.layer = "unregistered".into();
}
