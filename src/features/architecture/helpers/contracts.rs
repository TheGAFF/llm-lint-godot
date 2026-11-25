use crate::{
    features::configuration::{
        helpers::contracts::{Options, matches_output, missing_companions, options, output_name},
        models::Project,
    },
    shared::{
        helpers::issue,
        models::{Corpus, Diagnostic, Outcome, Source},
    },
};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

#[derive(Default)]
struct Script {
    classes: BTreeSet<String>,
    aliases: BTreeMap<String, String>,
    functions: Vec<Mapper>,
}
struct Mapper {
    name: String,
    scope: String,
    output: String,
    line: usize,
    column: usize,
    annotated: bool,
}

fn value<'a>(node: Node<'_>, text: &'a str) -> &'a str {
    &text[node.byte_range()]
}

fn inspect(node: Node<'_>, text: &str, scope: &str, script: &mut Script) {
    let mut scope = scope.to_owned();
    if node.kind() == "class_definition"
        && let Some(name) = node.child_by_field_name("name")
    {
        scope = format!("{scope}{}.", value(name, text));
        script.classes.insert(scope.trim_end_matches('.').into());
    }
    if node.kind() == "const_statement"
        && let (Some(name), Some(initializer)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        )
    {
        script.aliases.insert(
            format!("{scope}{}", value(name, text)),
            value(initializer, text).trim().into(),
        );
    }
    if node.kind() == "function_definition" {
        if let Some(mapper) = mapper(node, text, &scope) {
            script.functions.push(mapper);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, text, &scope, script);
    }
}

fn mapper(node: Node<'_>, text: &str, scope: &str) -> Option<Mapper> {
    let name = value(node.child_by_field_name("name")?, text);
    if !name.starts_with("map_") {
        return None;
    }
    let annotation = node.child_by_field_name("return_type");
    let output = annotation
        .map(|ty| value(ty, text).to_owned())
        .or_else(|| inferred_constructor(node, text))?;
    Some(Mapper {
        name: name.into(),
        scope: scope.into(),
        output,
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        annotated: annotation.is_some(),
    })
}

fn inferred_constructor(node: Node<'_>, text: &str) -> Option<String> {
    if node.kind() == "return_statement" {
        let raw = value(node, text).strip_prefix("return")?.trim();
        return raw.split_once(".new(").map(|(name, _)| name.trim().into());
    }
    if node.kind() == "lambda" {
        return None;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find_map(|child| inferred_constructor(child, text))
}

struct Index<'a> {
    scripts: BTreeMap<&'a str, Script>,
    globals: BTreeMap<&'a str, &'a str>,
    engines: BTreeSet<String>,
}

impl Index<'_> {
    fn resolve(
        &self,
        file: &str,
        scope: &str,
        name: &str,
        seen: &mut BTreeSet<String>,
    ) -> Option<String> {
        if !seen.insert(format!("{file}:{scope}:{name}")) {
            return None;
        }
        let script = self.scripts.get(file)?;
        let (head, tail) = name
            .split_once('.')
            .map_or((name, ""), |(head, tail)| (head, tail));
        let mut current = scope.trim_end_matches('.').to_owned();
        loop {
            let prefix = if current.is_empty() {
                String::new()
            } else {
                format!("{current}.")
            };
            let local = format!("{prefix}{name}");
            if script.classes.contains(&local) {
                return local.rsplit('.').next().map(str::to_owned);
            }
            if let Some(alias) = script.aliases.get(&format!("{prefix}{head}")) {
                return self.resolve_alias(file, &prefix, head, tail, alias, seen);
            }
            if current.is_empty() {
                break;
            }
            current = current
                .rsplit_once('.')
                .map_or("", |(parent, _)| parent)
                .into();
        }
        if let Some(target) = self.globals.get(head) {
            return if tail.is_empty() {
                Some(head.into())
            } else {
                self.resolve(target, "", tail, seen)
            };
        }
        self.engines.contains(name).then(|| name.to_owned())
    }
    fn resolve_alias(
        &self,
        file: &str,
        prefix: &str,
        head: &str,
        tail: &str,
        alias: &str,
        seen: &mut BTreeSet<String>,
    ) -> Option<String> {
        if let Some(preload) = alias.strip_prefix("preload(") {
            let (literal, suffix) = preload.split_once(')')?;
            let path = literal.trim().trim_matches(['\'', '"']);
            let target = if let Some(path) = path.strip_prefix("res://") {
                path.to_owned()
            } else {
                normalize(file, path)?
            };
            let qualified = [suffix.trim_start_matches('.'), tail]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(".");
            if qualified.is_empty() {
                let global = self
                    .globals
                    .iter()
                    .find_map(|(name, path)| (*path == target).then(|| (*name).to_owned()));
                let local = self
                    .scripts
                    .contains_key(target.as_str())
                    .then(|| head.to_owned());
                return global.or(local);
            }
            return self.resolve(&target, "", &qualified, seen);
        }
        let qualified = if tail.is_empty() {
            alias.to_owned()
        } else {
            format!("{alias}.{tail}")
        };
        self.resolve(file, prefix, &qualified, seen)
    }
}

fn normalize(file: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<_> = file.split('/').collect();
    parts.pop();
    for part in relative.split('/') {
        match part {
            ".." => {
                parts.pop()?;
            }
            "." => {}
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

fn output(raw: &str) -> Option<(&str, bool)> {
    let raw = raw.trim();
    if let Some(inner) = raw
        .strip_prefix("Array[")
        .and_then(|value| value.strip_suffix(']'))
    {
        return output(inner).map(|(name, _)| (name, true));
    }
    if raw.contains('[')
        || matches!(
            raw,
            "void"
                | "Variant"
                | "bool"
                | "int"
                | "float"
                | "String"
                | "StringName"
                | "NodePath"
                | "Array"
                | "Dictionary"
                | "Vector2"
                | "Vector2i"
                | "Vector3"
                | "Vector3i"
                | "Vector4"
                | "Vector4i"
                | "Rect2"
                | "Rect2i"
                | "Transform2D"
                | "Transform3D"
                | "Plane"
                | "Quaternion"
                | "AABB"
                | "Basis"
                | "Projection"
                | "Color"
                | "RID"
                | "Callable"
                | "Signal"
                | "PackedByteArray"
                | "PackedInt32Array"
                | "PackedInt64Array"
                | "PackedFloat32Array"
                | "PackedFloat64Array"
                | "PackedStringArray"
                | "PackedVector2Array"
                | "PackedVector3Array"
                | "PackedVector4Array"
                | "PackedColorArray"
        )
    {
        return None;
    }
    Some((raw, false))
}

pub(super) fn check_contracts(
    project: &Project,
    corpus: &Corpus,
    trees: &super::sources::ScriptTrees,
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<()> {
    let mut index = Index {
        scripts: BTreeMap::new(),
        globals: BTreeMap::new(),
        engines: serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../../policy/engine-classes.json"
        ))?["classes"]
            .as_array()
            .ok_or("Invalid engine class catalog")?
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
    };
    for source in corpus
        .sources
        .iter()
        .chain(&corpus.excluded_sources)
        .filter(|source| source.path.ends_with(".gd"))
    {
        let tree = trees
            .get(&source.path)
            .ok_or("Missing GDScript syntax tree")?;
        let mut script = Script::default();
        inspect(tree.root_node(), &source.text, "", &mut script);
        index.scripts.insert(&source.path, script);
        if let Some(name) = &source.class_name {
            index.globals.insert(name, &source.path);
        }
    }
    for source in &corpus.sources {
        if source.testing
            || project
                .settings
                .entrypoints
                .iter()
                .any(|entry| entry.path == source.path)
        {
            continue;
        }
        let settings = options(
            &project.settings.rules,
            &project.settings.overrides,
            "architecture/companion-files",
            &source.path,
        );
        for (id, target) in missing_companions(&project.root, &source.path, &settings)? {
            diagnostics.push(issue(
                &source.path,
                1,
                1,
                "architecture/companion-files",
                "<script>",
                &format!("Missing {id}: {target}"),
                "Create the required companion file",
            ));
        }
        let Some(script) = index.scripts.get(source.path.as_str()) else {
            continue;
        };
        let settings = options(
            &project.settings.rules,
            &project.settings.overrides,
            "architecture/mapper-return-type",
            &source.path,
        );
        let declarations = options(
            &project.settings.rules,
            &project.settings.overrides,
            "architecture/mapper-output-declaration",
            &source.path,
        );
        for mapper in &script.functions {
            check_mapper(
                &index,
                source,
                mapper,
                &settings,
                &declarations,
                diagnostics,
            );
        }
    }
    Ok(())
}

fn check_mapper(
    index: &Index<'_>,
    source: &Source,
    mapper: &Mapper,
    settings: &Options,
    declarations: &Options,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((raw, array)) = output(&mapper.output) else {
        return;
    };
    let resolved = index.resolve(&source.path, &mapper.scope, raw, &mut BTreeSet::new());
    let name = resolved
        .as_deref()
        .unwrap_or_else(|| raw.rsplit('.').next().unwrap_or(raw));
    if !mapper.annotated || !matches_output(&mapper.name, name, array, settings) {
        let expected = output_name(name, array, settings);
        diagnostics.push(issue(
            &source.path,
            mapper.line,
            mapper.column,
            "architecture/mapper-return-type",
            &mapper.name,
            &mapper.output,
            &format!("Explicit output annotation and map_{expected} or map_<input>_to_{expected}"),
        ));
    }
    if resolved.is_none() && !declarations.external_types.contains(&raw.to_owned()) {
        diagnostics.push(issue(
            &source.path,
            mapper.line,
            mapper.column,
            "architecture/mapper-output-declaration",
            &mapper.name,
            raw,
            "Declare the output class locally/shared or register an exact external type",
        ));
    }
}
