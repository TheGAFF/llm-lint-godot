use super::{Access, ClassId, Index, Kind, Usage, Value, quoted};
use crate::shared::models::Corpus;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Scenes<'a>(BTreeMap<&'a str, Scene<'a>>);

#[derive(Default)]
struct Scene<'a> {
    nodes: BTreeMap<String, Attachment<'a>>,
    connections: Vec<(&'a str, &'a str)>,
}

#[derive(Default)]
struct Attachment<'a> {
    script: Option<&'a str>,
    has_script: bool,
    instance: Option<&'a str>,
}

impl<'a> Scenes<'a> {
    pub(super) fn new(corpus: &'a Corpus) -> Self {
        Self(
            corpus
                .sources
                .iter()
                .chain(&corpus.excluded_sources)
                .filter(|source| source.path.ends_with(".tscn"))
                .map(|source| {
                    (
                        source.path.as_str(),
                        Scene::parse(&source.text, &corpus.uids),
                    )
                })
                .collect(),
        )
    }

    pub(super) fn autoloads(&self, corpus: &'a Corpus, index: &mut Index<'a>) {
        for (name, raw) in global_autoloads(corpus) {
            let target = index
                .script_path(raw, "project.godot")
                .or_else(|| self.script(index, raw, ".", &mut BTreeSet::new()));
            if let Some(target) = target {
                index.register(name, Value::Instance(target));
            }
        }
    }

    pub(super) fn expose(&self, index: &Index<'a>, usage: &mut Usage<'a>) {
        self.expose_reachable(index, usage, None);
    }

    pub(super) fn expose_reachable(
        &self,
        index: &Index<'a>,
        usage: &mut Usage<'a>,
        reachable: Option<&BTreeSet<&str>>,
    ) {
        let connections = self
            .0
            .iter()
            .filter(|(path, _)| reachable.is_none_or(|files| files.contains(*path)))
            .flat_map(|(&path, scene)| {
                scene
                    .connections
                    .iter()
                    .map(move |&(node, method)| (path, node, method))
            });
        for (path, node, method) in connections {
            let resource = format!("res://{path}");
            let Some(class) = self.script(index, &resource, node, &mut BTreeSet::new()) else {
                usage.uncertain(path, method, Access::Method);
                continue;
            };
            if let Some((id, declaration)) = index.lookup(Value::Instance(class), method)
                && declaration.kind == Kind::Method
            {
                usage.external.insert(id);
            }
        }
    }

    fn script<'b>(
        &'b self,
        index: &'b Index<'a>,
        resource: &'b str,
        node: &str,
        seen: &mut BTreeSet<(&'b str, String)>,
    ) -> Option<ClassId> {
        let path = index.resource_path(resource)?;
        let scene = self.0.get(path)?;
        if !seen.insert((path, node.into())) {
            return None;
        }
        if let Some(attachment) = scene.nodes.get(node) {
            if attachment.has_script {
                return index.script_path(attachment.script?, path);
            }
            if let Some(instance) = attachment.instance {
                return self.script(index, instance, ".", seen);
            }
        }
        if node == "." {
            return None;
        }
        let mut ancestor = node;
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if let Some(instance) = scene
                .nodes
                .get(parent)
                .and_then(|attachment| attachment.instance)
            {
                return self.script(index, instance, &node[parent.len() + 1..], seen);
            }
            ancestor = parent;
        }
        self.script(index, scene.nodes.get(".")?.instance?, node, seen)
    }
}

impl<'a> Scene<'a> {
    fn parse(source: &'a str, uids: &BTreeMap<String, String>) -> Self {
        let mut scene = Self::default();
        let mut resources = BTreeMap::new();
        let mut current = None;
        for line in source.lines().map(str::trim) {
            if line.starts_with('[') {
                current = None;
            }
            if line.starts_with("[ext_resource ") {
                let id = attribute(line, "id");
                let uid = attribute(line, "uid");
                let path = uid
                    .filter(|uid| uids.contains_key(*uid))
                    .or_else(|| attribute(line, "path"));
                resources.extend(id.zip(path));
            } else if line.starts_with("[node ")
                && let Some(path) = node_path(line)
            {
                let instance = assignment(line, "instance")
                    .and_then(external_id)
                    .and_then(|id| resources.get(id).copied());
                current = Some(scene.nodes.entry(path).or_insert(Attachment {
                    instance,
                    ..Attachment::default()
                }));
            } else if line.starts_with("[connection ") {
                scene
                    .connections
                    .extend(attribute(line, "to").zip(attribute(line, "method")));
            } else if let Some(attachment) = current.as_deref_mut()
                && let Some((name, value)) = line.split_once('=')
                && name.trim() == "script"
            {
                attachment.has_script = true;
                attachment.script =
                    external_id(value.trim()).and_then(|id| resources.get(id).copied());
            }
        }
        scene
    }
}

fn node_path(line: &str) -> Option<String> {
    let name = attribute(line, "name")?;
    Some(match attribute(line, "parent") {
        None => ".".into(),
        Some(".") => name.into(),
        Some(parent) => format!("{parent}/{name}"),
    })
}

fn assignment<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    line.match_indices(name).find_map(|(offset, _)| {
        if !line[..offset].ends_with(char::is_whitespace) {
            return None;
        }
        line[offset + name.len()..].strip_prefix('=')
    })
}

fn attribute<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let value = assignment(line, name)?.strip_prefix('"')?;
    value.split_once('"').map(|(value, _)| value)
}

fn external_id(value: &str) -> Option<&str> {
    quoted(value.strip_prefix("ExtResource(")?.split_once(')')?.0)
}

pub(super) fn global_autoloads(corpus: &Corpus) -> impl Iterator<Item = (&str, &str)> {
    let project = corpus
        .sources
        .iter()
        .chain(&corpus.excluded_sources)
        .find(|source| source.path == "project.godot");
    project.into_iter().flat_map(|project| {
        let lines: Vec<_> = project.text.lines().collect();
        project.references.iter().filter_map(move |reference| {
            let name = reference.item.strip_prefix("autoload:")?;
            let (_, value) = lines.get(reference.line - 1)?.split_once('=')?;
            value
                .trim_start()
                .starts_with("\"*")
                .then_some((name, reference.target.as_str()))
        })
    })
}
