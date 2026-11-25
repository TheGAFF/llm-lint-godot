//! Semantic duplicate evidence, independent of formatting and source line numbers.
use super::{tokens, value};
use crate::shared::models::Token;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tree_sitter::Node;

type Scope<'a> = BTreeMap<&'a str, usize>;

pub(super) fn describe(
    node: Node<'_>,
    text: &str,
    body: &mut [Token],
) -> (String, Vec<String>, Vec<String>, bool) {
    let mut bindings = Bindings::default();
    bindings.visit(node, text, &mut Scope::new());
    for token in body.iter_mut() {
        token.binding = bindings.offsets.get(&token.start).copied();
    }
    let mut digest = Sha256::new();
    for token in tokens(node, text) {
        for component in [&token.kind, &token.text] {
            digest.update((component.len() as u64).to_le_bytes());
            digest.update(component.as_bytes());
        }
    }
    let mut local_ids = BTreeMap::new();
    let semantic = body
        .iter()
        .filter(|t| !["(", ")", "[", "]", "{", "}", ",", ":", ".", "\\"].contains(&t.text.as_str()))
        .map(|t| {
            let next = local_ids.len();
            t.binding.map_or_else(
                || format!("{}:{}", t.kind, t.text),
                |b| format!("local:{}", local_ids.entry(b).or_insert(next)),
            )
        })
        .collect();
    let operators = body
        .iter()
        .filter(|t| is_operator(&t.text))
        .map(|t| t.text.clone())
        .collect();
    let data_only = body.first().is_some_and(|t| t.text == "return")
        && body.iter().all(|t| {
            ["integer", "float", "string"].contains(&t.kind.as_str())
                || [
                    "return", "true", "false", "null", "[", "]", "{", "}", ",", ":", "-", "+", "&",
                ]
                .contains(&t.text.as_str())
        });
    (
        format!("{:x}", digest.finalize()),
        semantic,
        operators,
        data_only,
    )
}

fn is_operator(text: &str) -> bool {
    [
        "+", "-", "*", "/", "%", "**", "=", "+=", "-=", "*=", "/=", "%=", "==", "!=", "<", ">",
        "<=", ">=", "and", "or", "not", "in", "is", "&", "|", "^", "~", "<<", ">>",
    ]
    .contains(&text)
}

#[derive(Default)]
struct Bindings {
    next: usize,
    offsets: BTreeMap<usize, usize>,
}

impl Bindings {
    fn declare<'a>(&mut self, node: Node<'_>, text: &'a str, scope: &mut Scope<'a>) {
        self.offsets.insert(node.start_byte(), self.next);
        scope.insert(value(node, text), self.next);
        self.next += 1;
    }

    fn visit<'a>(&mut self, node: Node<'_>, text: &'a str, scope: &mut Scope<'a>) {
        match node.kind() {
            "function_definition" | "lambda" => self.function(node, text, scope),
            "type" | "return_type" | "string" | "comment" => (),
            "variable_statement" | "const_statement" => {
                if let Some(initializer) = node.child_by_field_name("value") {
                    self.visit(initializer, text, scope);
                }
                if let Some(name) = node.child_by_field_name("name") {
                    self.declare(name, text, scope);
                }
            }
            "for_statement" => {
                if let Some(iterable) = node.child_by_field_name("right") {
                    self.visit(iterable, text, scope);
                }
                let mut inner = scope.clone();
                if let Some(name) = node.child_by_field_name("left") {
                    self.declare(name, text, &mut inner);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, text, &mut inner);
                }
            }
            "attribute_call" | "attribute_subscript" => {
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit(arguments, text, scope);
                }
            }
            "pair"
                if node
                    .child(1)
                    .is_some_and(|separator| separator.kind() == "=") =>
            {
                if let Some(value_node) = node.child_by_field_name("value") {
                    self.visit(value_node, text, scope);
                }
            }
            "attribute" => self.attribute(node, text, scope),
            "identifier" | "name" => {
                if let Some(binding) = scope.get(value(node, text)) {
                    self.offsets.insert(node.start_byte(), *binding);
                }
            }
            "body" => self.children(node, text, &mut scope.clone()),
            _ => self.children(node, text, scope),
        }
    }

    fn children<'a>(&mut self, node: Node<'_>, text: &'a str, scope: &mut Scope<'a>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, text, scope);
        }
    }

    fn function<'a>(&mut self, node: Node<'_>, text: &'a str, scope: &Scope<'a>) {
        let mut inner = scope.clone();
        if let Some(parameters) = node.child_by_field_name("parameters") {
            let mut cursor = parameters.walk();
            for parameter in parameters.named_children(&mut cursor) {
                self.parameter(parameter, text, &mut inner);
            }
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit(body, text, &mut inner);
        }
    }

    fn attribute<'a>(&mut self, node: Node<'_>, text: &'a str, scope: &mut Scope<'a>) {
        let mut cursor = node.walk();
        for (_, child) in node
            .named_children(&mut cursor)
            .enumerate()
            .filter(|(index, child)| *index == 0 || child.kind() != "identifier")
        {
            self.visit(child, text, scope);
        }
    }

    fn parameter<'a>(&mut self, node: Node<'_>, text: &'a str, scope: &mut Scope<'a>) {
        if let Some(default) = node.child_by_field_name("value") {
            self.visit(default, text, scope);
        }
        let name = if node.kind() == "identifier" {
            Some(node)
        } else {
            node.named_child(0).filter(|n| n.kind() == "identifier")
        };
        if let Some(name) = name {
            self.declare(name, text, scope);
        }
    }
}
