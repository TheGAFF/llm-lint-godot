use super::{LITERAL, Method, Scripts, children, text, tree};
use crate::shared::{
    helpers::issue,
    models::{Diagnostic, Outcome, Source},
};
use std::collections::BTreeMap;
use tree_sitter::Node;

type Environment = BTreeMap<String, String>;

struct Check<'a, 'b> {
    source: &'a Source,
    scripts: &'b Scripts<'a>,
    diagnostics: &'b mut Vec<Diagnostic>,
}

pub(super) fn check<'a>(
    source: &'a Source,
    scripts: &Scripts<'a>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<()> {
    let tree = tree(&source.text)?;
    let mut environment = Environment::new();
    for path in scripts.ancestry(&source.path).iter().rev() {
        if let Some(script) = scripts.scripts.get(path) {
            for (name, field) in &script.fields {
                environment.insert(name.clone(), field.ty.clone());
            }
        }
    }
    environment.insert("self".into(), source.path.clone());
    Check {
        source,
        scripts,
        diagnostics,
    }
    .visit(tree.root_node(), &mut environment, "");
    Ok(())
}

impl Check<'_, '_> {
    fn value(&self, node: Node<'_>) -> &str {
        text(node, &self.source.text)
    }

    fn require(&mut self, node: Node<'_>, ty: &str) {
        if node.kind() == "call"
            && (ty.starts_with("Array[") || ty.starts_with("Dictionary["))
            && let Some(arguments) = node.child_by_field_name("arguments")
            && let Some(value) = arguments.named_child(0)
        {
            self.require(value, ty);
            return;
        }
        if ty == "StringName" && node.kind() == "string" {
            self.diagnostics.push(issue(
                &self.source.path,
                node.start_position().row + 1,
                self.source.text[..node.start_byte()]
                    .rsplit('\n')
                    .next()
                    .map_or(1, |line| line.chars().count() + 1),
                LITERAL,
                "<literal>",
                self.value(node),
                "StringName literal &\"...\"",
            ));
            return;
        }
        if let Some(element) = ty
            .strip_prefix("Array[")
            .and_then(|tail| tail.strip_suffix(']'))
            && node.kind() == "array"
        {
            for child in children(node) {
                self.require(child, element);
            }
            return;
        }
        if let Some(types) = ty
            .strip_prefix("Dictionary[")
            .and_then(|tail| tail.strip_suffix(']'))
            && node.kind() == "dictionary"
            && let Some((key, value)) = types.split_once(',')
        {
            for pair in children(node) {
                self.require_pair(pair, key.trim(), value.trim());
            }
            return;
        }
        if matches!(
            node.kind(),
            "parenthesized_expression" | "conditional_expression" | "pattern_list"
        ) {
            for child in children(node)
                .into_iter()
                .filter(|child| node.child_by_field_name("condition") != Some(*child))
            {
                self.require(child, ty);
            }
        }
    }

    fn require_pair(&mut self, pair: Node<'_>, key: &str, value: &str) {
        if let Some(node) = pair.child_by_field_name("left") {
            self.require(node, key);
        }
        if let Some(node) = pair.child_by_field_name("value") {
            self.require(node, value);
        }
    }

    fn declaration(&mut self, node: Node<'_>, environment: &mut Environment) {
        let ty = node
            .child_by_field_name("type")
            .map(|ty| self.value(ty).to_owned())
            .unwrap_or_default();
        let Some(name) = node
            .child_by_field_name("name")
            .or_else(|| node.named_child(0))
        else {
            return;
        };
        let value = node.child_by_field_name("value");
        if let Some(value) = value {
            self.require(value, &ty);
        }
        let ty = if ty.is_empty() || ty == ":=" {
            value.map_or(String::new(), |value| {
                self.expression_type(value, environment)
            })
        } else {
            ty
        };
        environment.insert(self.value(name).into(), ty);
    }

    fn visit(&mut self, node: Node<'_>, environment: &mut Environment, output: &str) {
        if matches!(node.kind(), "comment" | "string" | "string_name") {
            return;
        }
        if node.kind() == "class_definition" {
            let mut local = Environment::new();
            for child in children(node) {
                self.visit(child, &mut local, "");
            }
            return;
        }
        if matches!(node.kind(), "function_definition" | "lambda") {
            self.function(node, environment);
            return;
        }
        if matches!(
            node.kind(),
            "variable_statement" | "const_statement" | "typed_default_parameter"
        ) {
            self.declaration(node, environment);
        }
        self.context(node, environment, output);
        for child in children(node) {
            if child.kind() == "body" {
                self.visit(child, &mut environment.clone(), output);
            } else {
                self.visit(child, environment, output);
            }
        }
    }

    fn function(&mut self, node: Node<'_>, environment: &Environment) {
        let mut local = environment.clone();
        for parameter in node
            .child_by_field_name("parameters")
            .into_iter()
            .flat_map(children)
        {
            let Some(name) = parameter.named_child(0) else {
                continue;
            };
            let ty = parameter
                .child_by_field_name("type")
                .map_or("", |ty| self.value(ty));
            local.insert(self.value(name).into(), ty.into());
            self.visit(parameter, &mut local, "");
        }
        let output = node
            .child_by_field_name("return_type")
            .map_or("", |ty| self.value(ty))
            .to_owned();
        if let Some(body) = node.child_by_field_name("body") {
            self.visit(body, &mut local, &output);
        }
    }

    fn context(&mut self, node: Node<'_>, environment: &mut Environment, output: &str) {
        match node.kind() {
            "assignment" => {
                if let (Some(left), Some(right)) = (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ) {
                    self.require(right, &self.expression_type(left, environment));
                }
            }
            "return_statement" => {
                if let Some(value) = node.named_child(0) {
                    self.require(value, output);
                }
            }
            "binary_operator" => self.comparison(node, environment),
            "match_statement" => self.patterns(node, environment),
            "for_statement" => self.iteration(node, environment),
            "call" | "attribute" => self.arguments(node, environment),
            "subscript" => self.subscript(node, environment),
            _ => {}
        }
    }

    fn subscript(&mut self, node: Node<'_>, environment: &Environment) {
        let Some(base) = node.named_child(0) else {
            return;
        };
        let ty = self.expression_type(base, environment);
        let Some(types) = ty
            .strip_prefix("Dictionary[")
            .and_then(|tail| tail.strip_suffix(']'))
        else {
            return;
        };
        let Some((key, _)) = types.split_once(',') else {
            return;
        };
        if let Some(arguments) = node
            .child_by_field_name("arguments")
            .or_else(|| node.named_child(1))
        {
            for index in children(arguments) {
                self.require(index, key.trim());
            }
        }
    }

    fn patterns(&mut self, node: Node<'_>, environment: &Environment) {
        let (Some(value), Some(body)) = (
            node.child_by_field_name("value"),
            node.child_by_field_name("body"),
        ) else {
            return;
        };
        let ty = self.expression_type(value, environment);
        for section in children(body) {
            for pattern in children(section)
                .into_iter()
                .filter(|child| child.kind() != "body")
            {
                self.require(pattern, &ty);
            }
        }
    }

    fn iteration(&mut self, node: Node<'_>, environment: &mut Environment) {
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        let ty = node
            .child_by_field_name("type")
            .map_or("", |ty| self.value(ty))
            .to_owned();
        if let Some(right) = node.child_by_field_name("right") {
            self.require(right, &format!("Array[{ty}]"));
        }
        environment.insert(self.value(left).into(), ty);
    }

    fn arguments(&mut self, node: Node<'_>, environment: &Environment) {
        let Some((method, arguments)) = self.call(node, environment) else {
            return;
        };
        for (ty, argument) in method.parameters.iter().zip(children(arguments)) {
            self.require(argument, ty);
        }
    }

    fn comparison(&mut self, node: Node<'_>, environment: &Environment) {
        let (Some(left), Some(right), Some(op)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
            node.child_by_field_name("op"),
        ) else {
            return;
        };
        let op = self.value(op).to_owned();
        if ["==", "!=", "in", "not in"].contains(&op.as_str()) {
            let left_ty = self.expression_type(left, environment);
            self.require(
                right,
                &if op.contains("in") {
                    format!("Array[{left_ty}]")
                } else {
                    left_ty
                },
            );
            if !op.contains("in") {
                self.require(left, &self.expression_type(right, environment));
            }
        }
    }

    fn expression_type(&self, node: Node<'_>, environment: &Environment) -> String {
        match node.kind() {
            "identifier" | "name" => environment
                .get(self.value(node))
                .cloned()
                .unwrap_or_default(),
            "string_name" => "StringName".into(),
            "string" => "String".into(),
            "attribute" => self.attribute_type(node, environment),
            "call" => self
                .call(node, environment)
                .map_or(String::new(), |(method, _)| method.output),
            "subscript" => node
                .named_child(0)
                .map(|base| self.expression_type(base, environment))
                .and_then(|ty| {
                    ty.strip_prefix("Array[")
                        .and_then(|tail| tail.strip_suffix(']'))
                        .map(str::to_owned)
                })
                .unwrap_or_default(),
            "parenthesized_expression" => node.named_child(0).map_or(String::new(), |child| {
                self.expression_type(child, environment)
            }),
            _ => String::new(),
        }
    }

    fn attribute_type(&self, node: Node<'_>, environment: &Environment) -> String {
        let nodes = children(node);
        let Some(first) = nodes.first() else {
            return String::new();
        };
        let mut ty = self.expression_type(*first, environment);
        if ty.is_empty() && self.scripts.classes.contains_key(self.value(*first)) {
            ty = self.value(*first).into();
        }
        for child in nodes.iter().skip(1) {
            ty = self.member_type(*child, ty);
        }
        ty
    }

    fn member_type(&self, child: Node<'_>, ty: String) -> String {
        if child.kind() != "attribute_call" {
            return self
                .scripts
                .field(&ty, self.value(child))
                .map_or(String::new(), |field| field.ty.clone());
        }
        let name = child.named_child(0).map_or("", |name| self.value(name));
        if name == "new" {
            return ty;
        }
        self.scripts
            .method(&ty, name)
            .map_or(String::new(), |method| method.output.clone())
    }

    fn call<'a>(&self, node: Node<'a>, environment: &Environment) -> Option<(Method, Node<'a>)> {
        if node.kind() == "call" {
            let name = self.value(node.named_child(0)?);
            let arguments = node.child_by_field_name("arguments")?;
            if name == "StringName" {
                return Some((
                    Method {
                        parameters: vec!["StringName".into()],
                        output: "StringName".into(),
                    },
                    arguments,
                ));
            }
            return Some((
                self.scripts.method(&self.source.path, name)?.clone(),
                arguments,
            ));
        }
        let nodes = children(node);
        let call = *nodes.last()?;
        if call.kind() != "attribute_call" {
            return None;
        }
        let mut ty = self.expression_type(nodes[0], environment);
        if ty.is_empty() && self.scripts.classes.contains_key(self.value(nodes[0])) {
            ty = self.value(nodes[0]).into();
        }
        for part in nodes.iter().skip(1).take(nodes.len().saturating_sub(2)) {
            ty = self.scripts.field(&ty, self.value(*part))?.ty.clone();
        }
        let name = self.value(call.named_child(0)?);
        let arguments = call.child_by_field_name("arguments")?;
        if let Some(types) = ty
            .strip_prefix("Dictionary[")
            .and_then(|tail| tail.strip_suffix(']'))
            && let Some((key, value)) = types.split_once(',')
            && ["get", "set", "has", "erase", "get_or_add"].contains(&name)
        {
            return Some((
                Method {
                    parameters: vec![key.trim().into(), value.trim().into()],
                    output: value.trim().into(),
                },
                arguments,
            ));
        }
        if let Some(element) = ty
            .strip_prefix("Array[")
            .and_then(|tail| tail.strip_suffix(']'))
            && ["append", "push_back", "push_front", "has", "erase", "find"].contains(&name)
        {
            return Some((
                Method {
                    parameters: vec![element.into()],
                    ..Method::default()
                },
                arguments,
            ));
        }
        Some((self.scripts.method(&ty, name)?.clone(), arguments))
    }
}

pub(super) fn resource_literals(
    source: &Source,
    scripts: &Scripts<'_>,
    ty: &str,
    value: &str,
    line: usize,
    column: usize,
) -> Vec<Diagnostic> {
    if !ty.contains("StringName") {
        return Vec::new();
    }
    let prefix = format!("var value: {ty} = ");
    let Ok(synthetic) =
        crate::features::parsing::parsers::parse_script(&source.path, format!("{prefix}{value}\n"))
    else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    let _result = check(&synthetic, scripts, &mut diagnostics);
    for diagnostic in &mut diagnostics {
        diagnostic.path = source.path.clone();
        diagnostic.line += line - 1;
        if diagnostic.line == line {
            diagnostic.column = diagnostic.column.saturating_sub(prefix.len()) + column - 1;
        }
    }
    diagnostics
}
