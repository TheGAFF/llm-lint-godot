use crate::features::parsing::helpers::{
    column, function_unit, resource_loader_alias, tokens, unquote,
};
use crate::shared::{
    helpers::issue,
    models::{LoadCallee, Outcome, Reference, ReferenceOrigin, Source},
};
use std::collections::BTreeSet;
use tree_sitter::{Node, Parser, Tree};

pub fn parse_script(path: &str, text: String) -> Outcome<Source> {
    let tree = parse_tree(&mut script_parser()?, &text)?;
    Ok(source_from_tree(path, text, &tree))
}

pub(crate) fn script_parser() -> Outcome<Parser> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_gdscript::LANGUAGE.into())?;
    Ok(parser)
}

pub(crate) fn parse_tree(parser: &mut Parser, text: &str) -> Outcome<Tree> {
    parser
        .parse(text, None)
        .ok_or_else(|| "GDScript parser returned no tree".into())
}

pub(crate) fn source_from_tree(path: &str, text: String, tree: &Tree) -> Source {
    let root = tree.root_node();
    let mut source = Source {
        path: path.into(),
        text: String::new(),
        owner: String::new(),
        layer: String::new(),
        testing: path.split('/').any(|part| part == "testing"),
        class_name: None,
        base: None,
        functions: vec![],
        references: vec![],
        class_uses: vec![],
        diagnostics: vec![],
        code_lines: 0,
        instance_state: false,
        native_suppressions: vec![],
    };
    let tokens = tokens(root, &text);
    source.code_lines = tokens
        .iter()
        .flat_map(|token| token.line..=token.end_line)
        .collect::<BTreeSet<_>>()
        .len();
    inspect(root, &text, &mut source, "<script>");
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        if node.kind() == "class_name_statement" {
            source.class_name = node
                .child_by_field_name("name")
                .map(|name| value(name, &text).into());
            if let Some(base) = node.child_by_field_name("extends") {
                source.base = Some(
                    value(base, &text)
                        .trim_start_matches("extends ")
                        .trim()
                        .into(),
                );
            }
        }
        if node.kind() == "extends_statement" {
            source.base = Some(
                value(node, &text)
                    .trim_start_matches("extends ")
                    .trim()
                    .into(),
            );
        }
        if node.kind() == "variable_statement" {
            source.instance_state = true;
        }
    }
    source.text = text;
    source
}

fn inspect(node: Node<'_>, text: &str, source: &mut Source, item: &str) {
    if node.kind() == "comment" {
        if value(node, text).contains("gdstyle:ignore") {
            source
                .native_suppressions
                .push((node.start_position().row + 1, value(node, text).into()));
        }
        return;
    }
    let point = node.start_position();
    if node.is_error() || node.is_missing() {
        source.diagnostics.push(issue(
            &source.path,
            point.row + 1,
            column(node.start_byte(), text),
            "architecture/syntax",
            item,
            value(node, text),
            "Valid supported Godot 4 GDScript",
        ));
        return;
    }
    let function_name;
    let current = if matches!(node.kind(), "function_definition" | "lambda") {
        function_name = node.child_by_field_name("name").map_or_else(
            || format!("<lambda:{}>", point.row + 1),
            |name| value(name, text).into(),
        );
        source
            .functions
            .push(function_unit(node, text, function_name.clone()));
        function_name.as_str()
    } else {
        item
    };
    if node.kind() == "call" || node.kind() == "attribute_call" {
        inspect_load(node, text, source, current);
    }
    if node.kind() == "string" {
        let target = unquote(value(node, text));
        let extends = node
            .parent()
            .is_some_and(|parent| parent.kind() == "extends_statement");
        if extends || target.starts_with("res://") || target.starts_with("uid://") {
            source.references.push(Reference {
                origin: if extends {
                    ReferenceOrigin::Extends
                } else {
                    ReferenceOrigin::Literal {
                        offset: node.start_byte(),
                    }
                },
                directory: target.ends_with('/'),
                target,
                item: current.into(),
                line: point.row + 1,
                column: column(node.start_byte(), text),
                dynamic: false,
                required: extends,
            });
        }
        return;
    }
    if node.kind() == "identifier" {
        let name = value(node, text);
        if name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            source
                .class_uses
                .push((name.into(), point.row + 1, column(node.start_byte(), text)));
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        inspect(child, text, source, current);
    }
}

fn inspect_load(node: Node<'_>, text: &str, source: &mut Source, item: &str) {
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    let start = if node.kind() == "attribute_call" {
        node.parent()
            .filter(|parent| parent.kind() == "attribute")
            .map_or(node.start_byte(), |parent| parent.start_byte())
    } else {
        node.start_byte()
    };
    let callee = text[start..arguments.start_byte()].trim();
    let direct = [
        "load",
        "preload",
        "ResourceLoader.load",
        "ResourceLoader.load_threaded_request",
        "ResourceLoader.load_threaded_get",
    ]
    .contains(&callee);
    let alias = callee.rsplit_once('.').is_some_and(|(receiver, method)| {
        ["load", "load_threaded_request", "load_threaded_get"].contains(&method)
            && resource_loader_alias(node, text, receiver)
    });
    if !direct && !alias {
        return;
    }
    let mut cursor = arguments.walk();
    let Some(argument) = arguments
        .named_children(&mut cursor)
        .find(|argument| !argument.is_extra())
    else {
        return;
    };
    let span = (argument.start_byte(), argument.end_byte());
    let literal = literal_argument(argument).map(|node| (node.start_byte(), node.end_byte()));
    let callee = match callee {
        "load" => LoadCallee::Load,
        "preload" => LoadCallee::Preload,
        _ => LoadCallee::ResourceLoader,
    };
    for target in path_arguments(argument, node, text) {
        let dynamic = !target.starts_with("res://") && !target.starts_with("uid://");
        source.references.push(Reference {
            origin: ReferenceOrigin::Load {
                argument: span,
                literal,
                callee,
            },
            target,
            item: item.into(),
            line: node.start_position().row + 1,
            column: column(node.start_byte(), text),
            dynamic,
            directory: false,
            required: true,
        });
    }
}

pub(crate) fn path_arguments(argument: Node<'_>, call: Node<'_>, text: &str) -> Vec<String> {
    if argument.kind() == "string" {
        return vec![unquote(value(argument, text))];
    }
    let mut targets = Vec::new();
    known_targets(argument, call, text, &mut targets);
    if targets.is_empty() {
        targets.push(value(argument, text).into());
    }
    targets
}

fn known_targets(argument: Node<'_>, node: Node<'_>, text: &str, targets: &mut Vec<String>) {
    let identifier = value(argument, text);
    let mut ancestor = node.parent();
    let mut iterable = identifier;
    while let Some(parent) = ancestor {
        if parent.kind() == "for_statement" {
            let header = value(parent, text)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches(':');
            if let Some((variable, collection)) =
                header.trim_start_matches("for ").split_once(" in ")
                && variable.split(':').next().unwrap_or("").trim() == identifier
            {
                iterable = collection.trim();
                break;
            }
        }
        ancestor = parent.parent();
    }
    let root = std::iter::successors(Some(node), tree_sitter::Node::parent).last();
    if let Some(root) = root {
        let mut cursor = root.walk();
        for declaration in root.named_children(&mut cursor) {
            if (declaration.kind() == "const_statement"
                || declaration.kind() == "constant_statement")
                && declaration
                    .child_by_field_name("name")
                    .is_some_and(|name| value(name, text) == iterable)
            {
                strings(declaration, text, targets);
            }
        }
    }
}

fn strings(node: Node<'_>, text: &str, targets: &mut Vec<String>) {
    if node.kind() == "string" {
        targets.push(unquote(value(node, text)));
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        strings(child, text, targets);
    }
}

fn value<'a>(node: Node<'_>, text: &'a str) -> &'a str {
    &text[node.byte_range()]
}

pub(crate) fn io_call<'a>(
    node: Node<'a>,
    text: &str,
    directories: bool,
) -> Option<(&'static str, Node<'a>)> {
    if !matches!(node.kind(), "attribute_call" | "call") {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    let start = node
        .parent()
        .filter(|parent| parent.kind() == "attribute")
        .map_or(node.start_byte(), |parent| parent.start_byte());
    let callee = text[start..args.start_byte()].trim();
    let kind = match callee {
        "FileAccess.open"
        | "FileAccess.get_file_as_string"
        | "FileAccess.get_file_as_bytes"
        | "Image.load_from_file" => "file_read",
        "ProjectSettings.load_resource_pack" => "resource_pack",
        "ResourceUID.get_id_path" | "ResourceUID.uid_to_path" => "uid_lookup",
        "DirAccess.open"
        | "DirAccess.get_files_at"
        | "DirAccess.get_directories_at"
        | "DirAccess.dir_exists_absolute"
            if directories =>
        {
            "directory_access"
        }
        _ if callee.rsplit('.').next() == Some("change_scene_to_file") => "scene_load",
        _ => return None,
    };
    let mut cursor = args.walk();
    let mut values = args
        .named_children(&mut cursor)
        .filter(|arg| arg.kind() != "comment");
    let target = values.next()?;
    if callee == "FileAccess.open"
        && values.next().is_some_and(|mode| {
            matches!(
                text[mode.byte_range()].trim(),
                "FileAccess.WRITE" | "FileAccess.WRITE_READ"
            )
        })
    {
        return None;
    }
    Some((kind, target))
}

pub(crate) fn literal_argument(mut node: Node<'_>) -> Option<Node<'_>> {
    while node.kind() == "parenthesized_expression" {
        let mut cursor = node.walk();
        node = node
            .named_children(&mut cursor)
            .find(|child| !child.is_extra())?;
    }
    (node.kind() == "string").then_some(node)
}
