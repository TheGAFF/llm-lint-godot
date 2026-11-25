mod expressions;

use super::{
    ACCESS, Access, ClassId, Index, Kind, LOCAL, Usage, Value, at, children, text, variable,
};
use crate::features::unused::{Gap, Kind as IssueKind};
use crate::shared::{helpers::snake_case, models::Diagnostic};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

#[derive(Clone, Default)]
struct Scope<'a> {
    locals: BTreeMap<&'a str, Value>,
    local_bindings: BTreeMap<&'a str, usize>,
    assigned: BTreeSet<&'a str>,
    method: Option<&'a str>,
    local_owner: Option<&'a str>,
    static_method: bool,
}

struct Visit<'a, 'b> {
    index: &'b Index<'a>,
    usage: &'b mut Usage<'a>,
    diagnostics: &'b mut Vec<Diagnostic>,
    id: ClassId,
}

pub(super) fn script<'a>(
    index: &Index<'a>,
    usage: &mut Usage<'a>,
    diagnostics: &mut Vec<Diagnostic>,
    id: ClassId,
    class: &super::index::Class<'a>,
) {
    let mut visit = Visit {
        index,
        usage,
        diagnostics,
        id,
    };
    for (name, member) in &class.members {
        let mut scope = Scope {
            static_method: member.static_member,
            ..Scope::default()
        };
        match member.kind {
            Kind::Method => visit.function(member.node, &scope, Some(name)),
            Kind::Variable | Kind::Constant => {
                let children = ["value", "setget"]
                    .into_iter()
                    .filter_map(|field| member.node.child_by_field_name(field));
                for child in children {
                    visit.eval(child, &mut scope);
                }
            }
            Kind::Other => (),
        }
    }
}

impl<'a> Visit<'a, '_> {
    fn class(&self) -> &super::index::Class<'a> {
        &self.index.classes[self.id]
    }

    fn raw(&self, node: Node<'a>) -> &'a str {
        text(node, &self.class().source.text)
    }

    fn receiver(&self, scope: &Scope<'_>) -> Value {
        if scope.static_method {
            Value::Script(self.id)
        } else {
            Value::Instance(self.id)
        }
    }

    fn reference(
        &mut self,
        receiver: Value,
        node: Node<'a>,
        name: &'a str,
        access: Access,
    ) -> (bool, Value) {
        let Some((id, member)) = self.index.lookup(receiver, name) else {
            if receiver == Value::Unknown {
                self.usage
                    .uncertain(&self.class().source.path, name, access);
            }
            return (false, Value::Unknown);
        };
        let owner = &self.index.classes[id.0];
        let class = self.class();
        let source = class.source;
        let lint = class.lint && owner.lint;
        let check = access.matches(member.kind);
        if access.audit_matches(member.kind)
            && let Some(audit) = &mut self.usage.audit
        {
            audit.used.insert(id);
        }
        let inherited = matches!(receiver, Value::Super(_))
            && access == Access::Call
            && member.kind == Kind::Method
            && self.index.inherited_hook(self.id, name);
        if check && owner.source.path == source.path {
            self.usage.internal.insert(id);
        } else if check {
            self.usage.external.insert(id);
            if name.starts_with('_') && lint && !inherited {
                self.diagnostics.push(at(
                    source,
                    node,
                    ACCESS,
                    name,
                    &format!("{name} is private to {}", owner.source.path),
                    "Use a public member across script files",
                ));
            }
        }
        let value = if member.kind.is_checked() && !check {
            Value::Unknown
        } else {
            self.index.member_value(
                id.0,
                member,
                matches!(access, Access::Call | Access::MethodCall),
                0,
            )
        };
        (true, value)
    }

    fn bare(&mut self, node: Node<'a>, scope: &Scope<'a>, access: Access) -> (bool, Value) {
        let name = self.raw(node);
        if let Some(value) = scope.locals.get(name) {
            if let Some(audit) = &mut self.usage.audit
                && let Some(offset) = scope.local_bindings.get(name)
            {
                audit
                    .locals
                    .remove(&(self.index.classes[self.id].source.path.as_str(), *offset));
            }
            return (true, *value);
        }
        match name {
            "self" => return (true, self.receiver(scope)),
            "super" => return (true, Value::Super(self.id)),
            _ => (),
        }
        let (found, value) = self.reference(self.receiver(scope), node, name, access);
        if found {
            return (true, value);
        }
        if let Some(symbols) = &mut self.usage.symbols {
            symbols.record(self.index, self.id, node, false, false);
        }
        let value = self.index.named(self.id, name, 0);
        (value != Value::Unknown, value)
    }

    fn local_name(&mut self, node: Node<'a>) {
        let name = self.raw(node);
        let class = self.class();
        if class.lint && !snake_case(name) {
            self.diagnostics.push(at(
                class.source,
                node,
                LOCAL,
                name,
                name,
                "Local variable name in snake_case without a leading underscore",
            ));
        }
    }

    fn declared_type(&self, node: Node<'a>) -> Option<Value> {
        let ty = node
            .child_by_field_name("type")
            .filter(|ty| ty.kind() != "inferred_type")?;
        Some(self.index.type_value(self.id, self.raw(ty), 0))
    }

    fn track_local(&mut self, node: Node<'a>, scope: &mut Scope<'a>) {
        let name = self.raw(node);
        let class = &self.index.classes[self.id];
        let Some(audit) = &mut self.usage.audit else {
            return;
        };
        scope.local_bindings.remove(name);
        if !class.lint || name.starts_with('_') {
            return;
        }
        let offset = node.start_byte();
        let item = scope
            .local_owner
            .map_or_else(|| name.into(), |owner| format!("{owner}.{name}"));
        let item = self.index.qualified(self.id, &item);
        scope.local_bindings.insert(name, offset);
        audit.locals.insert(
            (class.source.path.as_str(), offset),
            super::audit::at(class.source, node, IssueKind::Variable, item),
        );
    }

    fn dynamic_member(&mut self, receiver: Value, node: Node<'_>, access: Access) {
        if receiver == Value::Other {
            return;
        }
        let source = self.index.classes[self.id].source;
        if let Some(audit) = &mut self.usage.audit {
            let kinds: &'static [IssueKind] = match access {
                Access::Method | Access::MethodCall => &[IssueKind::Function],
                Access::Variable => &[IssueKind::Variable],
                _ => crate::features::unused::SYMBOL_KINDS,
            };
            audit.gaps.push(Gap {
                path: source.path.clone(),
                line: node.start_position().row + 1,
                message: "Computed member name could not be resolved.".into(),
                kinds,
            });
        }
    }

    fn declaration(&mut self, node: Node<'a>, scope: &mut Scope<'a>) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        if variable(node.kind()) {
            self.local_name(name);
        }
        let initial = node
            .child_by_field_name("value")
            .map_or(Value::Unknown, |node| self.eval(node, scope));
        self.track_local(name, scope);
        let name = self.raw(name);
        let value = self.declared_type(node).unwrap_or_else(|| {
            if node.child_by_field_name("type").is_some()
                || node.kind() == "const_statement"
                || !scope.assigned.contains(name)
            {
                initial
            } else {
                Value::Unknown
            }
        });
        scope.locals.insert(name, value);
    }

    fn function(&mut self, node: Node<'a>, parent: &Scope<'a>, name: Option<&'a str>) {
        let mut scope = parent.clone();
        scope.method = name;
        scope.local_owner = name.or(parent.local_owner);
        collect_assignments(node, &self.class().source.text, &mut scope.assigned);
        let parameters = node.child_by_field_name("parameters").or_else(|| {
            children(node)
                .into_iter()
                .find(|child| child.kind() == "parameters")
        });
        for parameter in parameters.into_iter().flat_map(children) {
            if let Some(default) = parameter.child_by_field_name("value") {
                self.eval(default, &mut scope);
            }
            let name = if parameter.kind() == "identifier" {
                Some(parameter)
            } else {
                parameter
                    .named_child(0)
                    .filter(|name| name.kind() == "identifier")
            };
            if let Some(name) = name {
                scope.local_bindings.remove(self.raw(name));
                scope.locals.insert(
                    self.raw(name),
                    self.declared_type(parameter).unwrap_or(Value::Unknown),
                );
            }
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.eval(body, &mut scope);
        }
    }

    fn for_loop(&mut self, node: Node<'a>, scope: &mut Scope<'a>) {
        if let Some(iterable) = node.child_by_field_name("right") {
            self.eval(iterable, scope);
        }
        let mut local = scope.clone();
        if let Some(name) = node.child_by_field_name("left") {
            self.local_name(name);
            self.track_local(name, &mut local);
            local.locals.insert(
                self.raw(name),
                self.declared_type(node).unwrap_or(Value::Unknown),
            );
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.eval(body, &mut local);
        }
    }

    fn pattern_section(&mut self, node: Node<'a>, scope: &mut Scope<'a>) {
        let mut local = scope.clone();
        for child in children(node) {
            if matches!(child.kind(), "body" | "pattern_guard") {
                self.eval(child, &mut local);
            } else {
                self.pattern(child, scope, &mut local);
            }
        }
    }

    fn pattern(&mut self, node: Node<'a>, scope: &mut Scope<'a>, local: &mut Scope<'a>) {
        match node.kind() {
            "pattern_binding" => {
                if let Some(name) = node.named_child(0) {
                    self.local_name(name);
                    self.track_local(name, local);
                    local.locals.insert(self.raw(name), Value::Unknown);
                }
            }
            "array" | "dictionary" | "pair" => {
                for child in children(node) {
                    self.pattern(child, scope, local);
                }
            }
            "identifier" if self.raw(node) == "_" => (),
            _ => {
                self.eval(node, scope);
            }
        }
    }

    fn eval(&mut self, node: Node<'a>, scope: &mut Scope<'a>) -> Value {
        match node.kind() {
            "comment" | "type" | "annotations" | "annotation" | "ERROR" => (),
            "identifier" | "name" => return self.bare(node, scope, Access::Any).1,
            "call" => return self.call(node, scope),
            "attribute" => return self.attribute(node, scope),
            "binary_operator" => return self.binary(node, scope),
            "subscript" => return self.subscript(node, scope),
            "parenthesized_expression" => {
                return node
                    .named_child(0)
                    .map_or(Value::Unknown, |node| self.eval(node, scope));
            }
            "array" | "dictionary" => {
                self.walk(node, scope);
                return Value::Other;
            }
            "string" | "string_name" | "integer" | "float" | "true" | "false" => {
                return Value::Other;
            }
            "lambda" | "set_body" | "get_body" => self.function(node, scope, None),
            "for_statement" => self.for_loop(node, scope),
            "pattern_section" => self.pattern_section(node, scope),
            "body" => self.walk(node, &mut scope.clone()),
            "setter" | "getter" => {
                self.bare(node, scope, Access::Method);
            }
            "pair"
                if node
                    .child(1)
                    .is_some_and(|separator| separator.kind() == "=") =>
            {
                if let Some(value) = node.child_by_field_name("value") {
                    self.eval(value, scope);
                }
            }
            kind if variable(kind) || kind == "const_statement" => self.declaration(node, scope),
            _ => self.walk(node, scope),
        }
        Value::Unknown
    }

    fn walk(&mut self, node: Node<'a>, scope: &mut Scope<'a>) {
        for child in children(node) {
            self.eval(child, scope);
        }
    }
}

fn collect_assignments<'a>(node: Node<'_>, source: &'a str, assigned: &mut BTreeSet<&'a str>) {
    if matches!(node.kind(), "assignment" | "augmented_assignment")
        && let Some(name) = node
            .child_by_field_name("left")
            .filter(|left| left.kind() == "identifier")
    {
        assigned.insert(text(name, source));
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_assignments(child, source, assigned);
    }
}
