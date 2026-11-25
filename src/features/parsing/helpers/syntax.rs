use crate::shared::models::{Function, Token};
use std::collections::BTreeSet;
use tree_sitter::Node;

mod duplicate_units;

pub fn resource_loader_alias(node: Node<'_>, text: &str, receiver: &str) -> bool {
    let scopes: Vec<_> = std::iter::successors(Some(node), tree_sitter::Node::parent)
        .filter(|parent| matches!(parent.kind(), "function_definition" | "source"))
        .collect();
    scopes
        .iter()
        .any(|scope| loader_binding(*scope, text, receiver))
}

fn loader_binding(node: Node<'_>, text: &str, receiver: &str) -> bool {
    if matches!(
        node.kind(),
        "variable_statement" | "const_statement" | "constant_statement"
    ) && node
        .child_by_field_name("name")
        .is_some_and(|name| value(name, text) == receiver)
        && node
            .child_by_field_name("value")
            .is_some_and(|value_node| value(value_node, text) == "ResourceLoader")
    {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !matches!(child.kind(), "function_definition" | "lambda"))
        .any(|child| loader_binding(child, text, receiver))
}

pub fn tokens(node: Node<'_>, text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    leaves(node, text, &mut tokens);
    tokens
}

pub fn function_unit(node: Node<'_>, text: &str, name: String) -> Function {
    let body = node.child_by_field_name("body");
    let mut tokens = body.map_or_else(Vec::new, |body| tokens(body, text));
    let (fingerprint, semantic, operators, data_only) =
        duplicate_units::describe(node, text, &mut tokens);
    let mut shape = Vec::new();
    if let Some(body) = body {
        structure(body, &mut shape);
    }
    let mut blockers = BTreeSet::new();
    for token in &tokens {
        if token.kind == "identifier"
            && token
                .text
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            blockers.insert(format!(
                "References type or captured constant {}",
                token.text
            ));
        }
        if [
            "self",
            "super",
            "await",
            "emit_signal",
            "emit",
            "get_node",
            "FileAccess",
            "ResourceLoader",
            "ResourceSaver",
            "DirAccess",
        ]
        .contains(&token.text.as_str())
        {
            blockers.insert(format!("Depends on {}", token.text));
        }
    }
    let mut cursor = node.walk();
    let static_method = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "static_keyword");
    if !static_method {
        blockers.insert("Instance method; inspect state and node dependencies".into());
    }
    Function {
        fingerprint,
        semantic,
        operators,
        data_only,
        name,
        static_method,
        line: node.start_position().row + 1,
        column: column(node.start_byte(), text),
        tokens,
        shape,
        blockers: blockers.into_iter().collect(),
    }
}

fn structure(node: Node<'_>, shape: &mut Vec<String>) {
    if matches!(node.kind(), "comment" | "region" | "end_region") {
        return;
    }
    if node.is_named() {
        shape.push(node.kind().into());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        structure(child, shape);
    }
}

fn leaves(node: Node<'_>, text: &str, tokens: &mut Vec<Token>) {
    if matches!(node.kind(), "comment" | "region" | "end_region")
        || node.start_byte() == node.end_byte()
    {
        return;
    }
    if node.child_count() == 0 || node.kind() == "string" {
        let value = value(node, text);
        if value.trim().is_empty() {
            return;
        }
        tokens.push(Token {
            binding: None,
            text: value.into(),
            kind: node.kind().into(),
            start: node.start_byte(),
            end: node.end_byte(),
            line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            column: column(node.start_byte(), text),
        });
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        leaves(child, text, tokens);
    }
}

pub fn column(offset: usize, text: &str) -> usize {
    text[..offset]
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .count()
        + 1
}

fn value<'a>(node: Node<'_>, text: &'a str) -> &'a str {
    &text[node.byte_range()]
}

pub fn unquote(value: &str) -> String {
    value
        .trim_start_matches('&')
        .trim_start_matches('^')
        .trim_matches(['\'', '"'])
        .replace("\\\"", "\"")
        .replace("\\'", "'")
}
