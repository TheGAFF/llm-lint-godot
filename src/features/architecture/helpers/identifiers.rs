mod literals;
mod resources;
mod visibility;
pub(crate) use visibility::{audit_symbols, reference_symbols};

use crate::shared::models::{Corpus, Diagnostic, Outcome, Source};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Tree};

pub const NAME: &str = "architecture/resource-id-name";
pub const TYPE: &str = "architecture/resource-id-type";
pub const LITERAL: &str = "architecture/stringname-literal";

#[derive(Clone, Default)]
struct Field {
    ty: String,
    value: String,
    exported: bool,
    line: usize,
    column: usize,
}

#[derive(Clone, Default)]
struct Method {
    parameters: Vec<String>,
    output: String,
}

#[derive(Default)]
struct Script {
    base: String,
    fields: BTreeMap<String, Field>,
    methods: BTreeMap<String, Method>,
}

struct Scripts<'a> {
    scripts: BTreeMap<String, Script>,
    classes: BTreeMap<String, String>,
    sources: BTreeMap<String, &'a Source>,
    engine: BTreeMap<String, String>,
}

pub(super) fn check(
    corpus: &Corpus,
    trees: &super::sources::ScriptTrees,
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<()> {
    #[derive(serde::Deserialize)]
    struct Engine {
        role_bases: BTreeMap<String, String>,
    }
    let engine: Engine =
        serde_json::from_str(include_str!("../../../../policy/engine-classes.json"))?;
    let mut scripts = Scripts {
        scripts: BTreeMap::new(),
        classes: corpus.uids.clone(),
        sources: BTreeMap::new(),
        engine: engine.role_bases,
    };
    for source in corpus.sources.iter().chain(&corpus.excluded_sources) {
        if !source.path.ends_with(".gd") {
            continue;
        }
        let tree = trees
            .get(&source.path)
            .ok_or("Missing GDScript syntax tree")?;
        let script = metadata(source, tree.root_node());
        if let Some(class) = &source.class_name {
            scripts.classes.insert(class.clone(), source.path.clone());
        }
        scripts.sources.insert(source.path.clone(), source);
        scripts.scripts.insert(source.path.clone(), script);
    }
    for source in &corpus.sources {
        if source.path.ends_with(".gd") {
            resources::check_declaration(source, &scripts, diagnostics);
            literals::check(source, &scripts, diagnostics)?;
        }
    }
    resources::check(corpus, &scripts, diagnostics);
    visibility::check(corpus, trees, diagnostics)?;
    Ok(())
}

pub(crate) fn tree(text: &str) -> Outcome<Tree> {
    use crate::features::parsing::parsers::{parse_tree, script_parser};
    parse_tree(&mut script_parser()?, text)
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn metadata(source: &Source, root: Node<'_>) -> Script {
    let mut script = Script {
        base: source.base.clone().unwrap_or_default(),
        ..Script::default()
    };
    for node in children(root) {
        if matches!(node.kind(), "variable_statement" | "const_statement") {
            if let Some(name) = node.child_by_field_name("name") {
                script.fields.insert(
                    text(name, &source.text).into(),
                    Field {
                        ty: field_type(node, &source.text),
                        value: node
                            .child_by_field_name("value")
                            .map_or("", |value| text(value, &source.text))
                            .into(),
                        exported: text(node, &source.text).contains("@export"),
                        line: node.start_position().row + 1,
                        column: node.start_position().column + 1,
                    },
                );
            }
        } else if node.kind() == "function_definition"
            && let Some(name) = node.child_by_field_name("name")
        {
            script.methods.insert(
                text(name, &source.text).into(),
                Method {
                    parameters: parameter_types(node, &source.text),
                    output: node
                        .child_by_field_name("return_type")
                        .map_or("", |ty| text(ty, &source.text))
                        .into(),
                },
            );
        }
    }
    script
}

impl Scripts<'_> {
    fn path<'a>(&'a self, name: &'a str) -> &'a str {
        self.classes.get(name).map_or(name, String::as_str)
    }

    fn ancestry(&self, name: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = self.path(name).to_owned();
        while seen.insert(current.clone()) {
            result.push(current.clone());
            if !self.scripts.contains_key(&current) {
                current = self.engine.get(&current).cloned().unwrap_or_default();
                continue;
            }
            let Some(script) = self.scripts.get(&current) else {
                break;
            };
            let base = script.base.trim_matches('"');
            current = if base.ends_with(".gd") && !base.starts_with("res://") {
                let parent = current.rsplit_once('/').map_or("", |(parent, _)| parent);
                format!("{parent}/{base}")
            } else {
                self.path(base.trim_start_matches("res://")).to_owned()
            };
        }
        result
    }

    fn field(&self, class: &str, name: &str) -> Option<&Field> {
        self.ancestry(class)
            .iter()
            .find_map(|path| self.scripts.get(path)?.fields.get(name))
    }

    fn method(&self, class: &str, name: &str) -> Option<&Method> {
        self.ancestry(class)
            .iter()
            .find_map(|path| self.scripts.get(path)?.methods.get(name))
    }
}

fn parameter_types(node: Node<'_>, source: &str) -> Vec<String> {
    node.child_by_field_name("parameters")
        .map_or_else(Vec::new, |parameters| {
            children(parameters)
                .iter()
                .map(|parameter| {
                    parameter
                        .child_by_field_name("type")
                        .map_or("", |ty| text(ty, source))
                        .into()
                })
                .collect()
        })
}

pub(super) fn comments(source: &Source) -> Outcome<Vec<(usize, String)>> {
    let tree = tree(&source.text)?;
    let mut result = Vec::new();
    collect_comments(tree.root_node(), &source.text, &mut result);
    Ok(result)
}

fn collect_comments(node: Node<'_>, source: &str, result: &mut Vec<(usize, String)>) {
    if node.kind() == "comment" {
        result.push((
            node.start_position().row,
            text(node, source).trim_start_matches('#').trim().into(),
        ));
        return;
    }
    for child in children(node) {
        collect_comments(child, source, result);
    }
}

fn field_type(node: Node<'_>, source: &str) -> String {
    let annotation = node
        .child_by_field_name("type")
        .map_or("", |ty| text(ty, source));
    if !annotation.is_empty() && annotation != ":=" {
        return annotation.into();
    }
    let value = node
        .child_by_field_name("value")
        .map_or("", |value| text(value, source));
    if value.starts_with("&\"") {
        return "StringName".into();
    }
    if value.starts_with('"') {
        return "String".into();
    }
    if value.parse::<i64>().is_ok() {
        return "int".into();
    }
    if value.parse::<f64>().is_ok() {
        return "float".into();
    }
    String::new()
}
