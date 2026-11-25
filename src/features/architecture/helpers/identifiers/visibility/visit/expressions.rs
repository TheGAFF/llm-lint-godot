use super::{Access, Scope, Value, Visit, children};
use crate::features::architecture::helpers::identifiers::visibility::quoted;
use tree_sitter::Node;

impl<'a> Visit<'a, '_> {
    pub(super) fn call(&mut self, node: Node<'a>, scope: &mut Scope<'a>) -> Value {
        let Some(callee) = node.named_child(0) else {
            return Value::Unknown;
        };
        let (arguments, values) = self.arguments(node, scope);
        if callee.kind() != "identifier" {
            self.eval(callee, scope);
            return Value::Unknown;
        }
        let name = self.raw(callee);
        if name == "super"
            && let Some(method) = scope.method
        {
            return self
                .reference(Value::Super(self.id), callee, method, Access::Call)
                .1;
        }
        let (found, value) = self.bare(callee, scope, Access::Call);
        if found {
            return value;
        }
        match name {
            "load" | "preload" => self.index.load_script(self.id, node),
            "Callable" => {
                if let (Some(receiver), Some(name)) = (values.first(), arguments.get(1)) {
                    self.audit_computed(*receiver, *name, Access::Method);
                    self.literal_reference(*receiver, *name, Access::Method);
                }
                Value::Other
            }
            "call" | "call_deferred" | "callv" | "get" | "set" => {
                self.reflect(self.receiver(scope), name, &arguments)
            }
            _ if self.usage.audit.is_some() => {
                self.audit_reflect(self.receiver(scope), name, &arguments, &values);
                if self.index.builtin(name) {
                    Value::Other
                } else {
                    Value::Unknown
                }
            }
            name if self.index.builtin(name) => Value::Other,
            _ => Value::Unknown,
        }
    }

    pub(super) fn attribute(&mut self, node: Node<'a>, scope: &mut Scope<'a>) -> Value {
        let mut parts = children(node)
            .into_iter()
            .filter(|part| part.kind() != "comment");
        let Some(first) = parts.next() else {
            return Value::Unknown;
        };
        let mut value = if first.kind() == "identifier" {
            let (found, value) = self.bare(first, scope, Access::Any);
            if !found && self.index.builtin(self.raw(first)) {
                Value::Other
            } else {
                value
            }
        } else {
            self.eval(first, scope)
        };
        for part in parts {
            value = self.attribute_part(value, part, scope);
        }
        value
    }

    fn attribute_part(&mut self, receiver: Value, part: Node<'a>, scope: &mut Scope<'a>) -> Value {
        let (arguments, values) = self.arguments(part, scope);
        let (name, access) = match part.kind() {
            "identifier" => (part, Access::Any),
            "attribute_call" => {
                let Some(name) = part.named_child(0) else {
                    return Value::Unknown;
                };
                (name, Access::Call)
            }
            "attribute_subscript" => {
                let Some(name) = part.named_child(0) else {
                    return Value::Unknown;
                };
                let (_, value) = self.reference(receiver, name, self.raw(name), Access::Any);
                return self.indexed(value, &arguments);
            }
            _ => return Value::Unknown,
        };
        let raw = self.raw(name);
        if let Value::Script(class) = receiver {
            if access == Access::Call && raw == "new" {
                return Value::Instance(class);
            }
            if access == Access::Any
                && let Some(nested) = self.index.nested(class, raw)
            {
                return Value::Script(nested);
            }
        }
        let (found, value) = self.reference(receiver, name, raw, access);
        if !found && access == Access::Call {
            if self.usage.audit.is_some() {
                self.audit_reflect(receiver, raw, &arguments, &values);
            }
            return self.reflect(receiver, raw, &arguments);
        }
        value
    }

    pub(super) fn binary(&mut self, node: Node<'a>, scope: &mut Scope<'a>) -> Value {
        if let Some(left) = node.child_by_field_name("left") {
            self.eval(left, scope);
        }
        let Some(right) = node.child_by_field_name("right") else {
            return Value::Unknown;
        };
        let operator = node.child_by_field_name("op").map_or("", |op| self.raw(op));
        if self.usage.symbols.is_some() && matches!(operator, "as" | "is" | "is not") {
            self.eval(right, scope);
        }
        if operator == "as" {
            return self.index.type_value(self.id, self.raw(right), 0);
        }
        if !operator.starts_with("is") {
            self.eval(right, scope);
        }
        Value::Unknown
    }

    pub(super) fn subscript(&mut self, node: Node<'a>, scope: &mut Scope<'a>) -> Value {
        let receiver = node
            .named_child(0)
            .map_or(Value::Unknown, |base| self.eval(base, scope));
        let (arguments, _) = self.arguments(node, scope);
        self.indexed(receiver, &arguments)
    }

    fn indexed(&mut self, receiver: Value, arguments: &[Node<'a>]) -> Value {
        arguments.first().map_or(Value::Unknown, |argument| {
            self.literal_reference(receiver, *argument, Access::Variable)
        })
    }

    fn arguments(&mut self, node: Node<'a>, scope: &mut Scope<'a>) -> (Vec<Node<'a>>, Vec<Value>) {
        let arguments: Vec<_> = node
            .child_by_field_name("arguments")
            .into_iter()
            .flat_map(children)
            .filter(|argument| argument.kind() != "comment")
            .collect();
        let values = arguments
            .iter()
            .map(|node| self.eval(*node, scope))
            .collect();
        (arguments, values)
    }

    fn reflect(&mut self, receiver: Value, name: &str, arguments: &[Node<'a>]) -> Value {
        let access = match name {
            "call" | "call_deferred" | "callv" => Access::MethodCall,
            "get" | "set" => Access::Variable,
            _ => return Value::Unknown,
        };
        arguments.first().map_or(Value::Unknown, |argument| {
            self.audit_computed(receiver, *argument, access);
            let result = self.literal_reference(receiver, *argument, access);
            if matches!(name, "call_deferred" | "set") {
                Value::Unknown
            } else {
                result
            }
        })
    }

    fn audit_computed(&mut self, receiver: Value, argument: Node<'a>, access: Access) {
        if !matches!(argument.kind(), "string" | "string_name" | "node_path")
            || quoted(self.raw(argument).trim_start_matches('^')).is_none()
        {
            self.dynamic_member(receiver, argument, access);
        }
    }

    fn audit_reflect(
        &mut self,
        receiver: Value,
        name: &str,
        arguments: &[Node<'a>],
        values: &[Value],
    ) {
        let (access, slot, target) = match name {
            "has_method" | "rpc" => (Access::Method, 0, receiver),
            "rpc_id" => (Access::Method, 1, receiver),
            "call_group" => (Access::Method, 1, Value::Unknown),
            "call_group_flags" => (Access::Method, 2, Value::Unknown),
            "set_group" => (Access::Variable, 1, Value::Unknown),
            "set_group_flags" => (Access::Variable, 2, Value::Unknown),
            "get_indexed" | "set_indexed" | "set_deferred" => (Access::Variable, 0, receiver),
            "tween_property" | "interpolate_property" => (
                Access::Variable,
                1,
                values.first().copied().unwrap_or(Value::Unknown),
            ),
            _ => return,
        };
        let Some(&argument) = arguments.get(slot) else {
            return;
        };
        if let Some(name) = quoted(self.raw(argument).trim_start_matches('^'))
            .filter(|_| matches!(argument.kind(), "string" | "string_name" | "node_path"))
        {
            let base = name.split(':').next().unwrap_or(name);
            self.reference(target, argument, base, access);
        } else {
            self.dynamic_member(target, argument, access);
        }
    }

    fn literal_reference(&mut self, receiver: Value, node: Node<'a>, access: Access) -> Value {
        if !matches!(node.kind(), "string" | "string_name") {
            return Value::Unknown;
        }
        quoted(self.raw(node)).map_or(Value::Unknown, |name| {
            self.reference(receiver, node, name, access).1
        })
    }
}
