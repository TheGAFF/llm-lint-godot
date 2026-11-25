use super::{children, quoted, text, variable};
use crate::shared::models::{Corpus, Outcome, Source};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

pub(super) type ClassId = usize;
pub(super) type MemberId<'a> = (ClassId, &'a str);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Unknown,
    Other,
    Instance(ClassId),
    Script(ClassId),
    Super(ClassId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Method,
    Variable,
    Constant,
    Other,
}

impl Kind {
    pub(super) fn is_checked(self) -> bool {
        matches!(self, Self::Method | Self::Variable)
    }
}

pub(super) struct Member<'a> {
    pub kind: Kind,
    pub node: Node<'a>,
    pub name: Node<'a>,
    pub exposed: bool,
    pub static_member: bool,
}

pub(super) struct Class<'a> {
    pub source: &'a Source,
    pub lint: bool,
    pub body: Node<'a>,
    pub members: BTreeMap<&'a str, Member<'a>>,
    nested: BTreeMap<&'a str, ClassId>,
    outer: Option<ClassId>,
    base_name: &'a str,
}

#[derive(Default)]
enum Base<'a> {
    Script(ClassId),
    Native(&'a str),
    #[default]
    Unknown,
}

#[derive(serde::Deserialize)]
struct Engine {
    builtins: BTreeSet<String>,
    classes: BTreeMap<String, (String, Vec<String>)>,
}

pub(super) struct Index<'a> {
    pub classes: Vec<Class<'a>>,
    files: BTreeMap<&'a str, ClassId>,
    globals: BTreeMap<&'a str, Option<Value>>,
    bases: Vec<Base<'a>>,
    uids: &'a BTreeMap<String, String>,
    engine: Engine,
}

impl<'a> Index<'a> {
    pub(super) fn build(
        corpus: &'a Corpus,
        trees: &'a crate::features::architecture::helpers::sources::ScriptTrees,
        scenes: &super::scenes::Scenes<'a>,
    ) -> Outcome<Self> {
        let mut index = Self {
            classes: Vec::new(),
            files: BTreeMap::new(),
            globals: BTreeMap::new(),
            bases: Vec::new(),
            uids: &corpus.uids,
            engine: serde_json::from_str(include_str!(
                "../../../../../../policy/engine-virtuals.json"
            ))?,
        };
        let sources = corpus
            .sources
            .iter()
            .map(|source| (source, true))
            .chain(corpus.excluded_sources.iter().map(|source| (source, false)));
        for (source, lint) in sources {
            let Some(tree) = trees.get(source.path.as_str()) else {
                continue;
            };
            let id = index.add_class(
                source,
                lint,
                tree.root_node(),
                root_base(tree.root_node(), &source.text),
                None,
            );
            index.files.insert(&source.path, id);
            if let Some(name) = &source.class_name {
                index.register(name, Value::Script(id));
            }
        }
        scenes.autoloads(corpus, &mut index);
        index.bases = (0..index.classes.len()).map(|id| index.base(id)).collect();
        Ok(index)
    }

    fn add_class(
        &mut self,
        source: &'a Source,
        lint: bool,
        body: Node<'a>,
        base_name: &'a str,
        outer: Option<ClassId>,
    ) -> ClassId {
        let id = self.classes.len();
        self.classes.push(Class {
            source,
            lint,
            body,
            members: BTreeMap::new(),
            nested: BTreeMap::new(),
            outer,
            base_name,
        });
        let mut annotations = Vec::new();
        for node in children(body) {
            match node.kind() {
                "annotation" => {
                    annotations.push(node);
                    continue;
                }
                "comment" | "region" | "end_region" => continue,
                _ => (),
            }
            if node.kind() == "class_definition"
                && let Some(name) = node.child_by_field_name("name")
                && let Some(body) = node.child_by_field_name("body")
            {
                let base = node
                    .child_by_field_name("extends")
                    .and_then(|extends| extends.named_child(0))
                    .map_or_else(
                        || root_base(body, &source.text),
                        |base| text(base, &source.text),
                    );
                let nested = self.add_class(source, lint, body, base, Some(id));
                self.classes[id]
                    .nested
                    .insert(text(name, &source.text), nested);
            } else if let Some(member) = member(node, &source.text, &annotations) {
                self.classes[id]
                    .members
                    .insert(text(member.name, &source.text), member);
            }
            annotations.clear();
        }
        id
    }

    pub(super) fn register(&mut self, name: &'a str, value: Value) {
        self.globals
            .entry(name)
            .and_modify(|value| *value = None)
            .or_insert(Some(value));
    }

    pub(super) fn script_path(&self, raw: &str, from: &str) -> Option<ClassId> {
        if let Some(path) = self.resource_path(raw) {
            return self.files.get(path).copied();
        }
        if raw.contains("://") {
            return None;
        }
        let parent = from.rsplit_once('/').map_or("", |(parent, _)| parent);
        let mut parts = parent
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        for part in raw.split('/') {
            match part {
                "" | "." => (),
                ".." => {
                    parts.pop()?;
                }
                _ => parts.push(part),
            }
        }
        self.files.get(parts.join("/").as_str()).copied()
    }

    pub(super) fn resource_path<'b>(&'b self, raw: &'b str) -> Option<&'b str> {
        raw.strip_prefix("res://")
            .or_else(|| self.uids.get(raw).map(String::as_str))
    }

    fn base(&self, id: ClassId) -> Base<'a> {
        let class = &self.classes[id];
        let name = class.base_name;
        if let Some(path) = quoted(name) {
            return self
                .script_path(path, &class.source.path)
                .map_or(Base::Unknown, Base::Script);
        }
        if let Value::Script(base) = self.named(id, name, 0) {
            return Base::Script(base);
        }
        if self.engine.classes.contains_key(name) {
            return Base::Native(name);
        }
        Base::Unknown
    }

    fn parent(&self, id: ClassId) -> Option<ClassId> {
        match self.bases.get(id)? {
            Base::Script(base) => Some(*base),
            _ => None,
        }
    }

    fn ancestors(&self, id: ClassId) -> impl Iterator<Item = ClassId> {
        let mut seen = BTreeSet::new();
        std::iter::successors(Some(id), |id| self.parent(*id))
            .take_while(move |id| seen.insert(*id))
    }

    pub(super) fn lookup(
        &self,
        receiver: Value,
        name: &str,
    ) -> Option<(MemberId<'a>, &Member<'a>)> {
        let id = match receiver {
            Value::Script(id) | Value::Instance(id) => id,
            Value::Super(id) => self.parent(id)?,
            _ => return None,
        };
        let (owner, name, member) = self.ancestors(id).find_map(|owner| {
            let (name, member) = self.classes[owner].members.get_key_value(name)?;
            Some((owner, *name, member))
        })?;
        if matches!(receiver, Value::Script(_)) && !member.static_member {
            return None;
        }
        Some(((owner, name), member))
    }

    pub(super) fn expose_overrides(&self, id: ClassId, exposed: &mut BTreeSet<MemberId<'a>>) {
        let class = &self.classes[id];
        for (name, member) in &class.members {
            if member.kind != Kind::Method || member.static_member || name.starts_with('_') {
                continue;
            }
            let parents = self.ancestors(id).skip(1).filter(|base| {
                let parent = &self.classes[*base];
                parent.source.path != class.source.path
                    && parent
                        .members
                        .get(name)
                        .is_some_and(|method| method.kind == Kind::Method && !method.static_member)
            });
            for base in parents {
                exposed.extend([(base, *name), (id, *name)]);
            }
        }
    }

    pub(super) fn audit_overrides(&self, id: ClassId, used: &mut BTreeSet<MemberId<'a>>) {
        for (&name, member) in &self.classes[id].members {
            if member.kind != Kind::Method || member.static_member {
                continue;
            }
            let inherited = self.ancestors(id).skip(1).filter(|&parent| {
                self.classes[parent]
                    .members
                    .get(name)
                    .is_some_and(|base| base.kind == Kind::Method && !base.static_member)
            });
            for parent in inherited {
                used.extend([(parent, name), (id, name)]);
            }
        }
    }

    pub(super) fn qualified(&self, mut id: ClassId, name: &str) -> String {
        let mut parts = vec![name];
        while let Some(parent) = self.classes[id].outer {
            if let Some((&name, _)) = self.classes[parent]
                .nested
                .iter()
                .find(|&(_, &child)| child == id)
            {
                parts.push(name);
            }
            id = parent;
        }
        parts.reverse();
        parts.join(".")
    }

    pub(super) fn inherited_hook(&self, caller: ClassId, name: &str) -> bool {
        self.classes[caller]
            .members
            .get(name)
            .is_some_and(|member| member.kind == Kind::Method && !member.static_member)
            || self.engine_callback(caller, name)
    }

    pub(super) fn engine_callback(&self, id: ClassId, name: &str) -> bool {
        if matches!(name, "_init" | "_static_init") {
            return true;
        }
        let Some(Base::Native(native)) =
            self.ancestors(id).last().and_then(|id| self.bases.get(id))
        else {
            return false;
        };
        std::iter::successors(Some(*native), |native| {
            let (parent, _) = self.engine.classes.get(*native)?;
            (!parent.is_empty()).then_some(parent.as_str())
        })
        .any(|native| {
            self.engine.classes.get(native).is_some_and(|(_, methods)| {
                methods
                    .binary_search_by(|method| method.as_str().cmp(name))
                    .is_ok()
            })
        })
    }

    pub(super) fn nested(&self, id: ClassId, name: &str) -> Option<ClassId> {
        self.ancestors(id)
            .find_map(|id| self.classes[id].nested.get(name).copied())
    }

    pub(super) fn named(&self, id: ClassId, name: &str, depth: usize) -> Value {
        if depth >= 24 {
            return Value::Unknown;
        }
        let mut parts = name.split('.').map(str::trim);
        let first = parts.next().unwrap_or("");
        parts.fold(
            self.simple_name(id, first, depth),
            |value, part| match value {
                Value::Script(id) => self.nested(id, part).map_or(Value::Unknown, Value::Script),
                _ => Value::Unknown,
            },
        )
    }

    fn simple_name(&self, id: ClassId, name: &str, depth: usize) -> Value {
        self.lexical_name(id, name, depth).unwrap_or_else(|| {
            self.globals
                .get(name)
                .copied()
                .flatten()
                .unwrap_or(Value::Unknown)
        })
    }

    pub(super) fn lexical_name(&self, id: ClassId, name: &str, depth: usize) -> Option<Value> {
        for scope in std::iter::successors(Some(id), |id| self.classes[*id].outer) {
            if let Some(nested) = self.nested(scope, name) {
                return Some(Value::Script(nested));
            }
            if let Some(((owner, _), member)) = self.lookup(Value::Script(scope), name)
                && member.kind == Kind::Constant
            {
                return Some(self.member_value(owner, member, false, depth + 1));
            }
        }
        None
    }

    pub(super) fn type_value(&self, id: ClassId, ty: &str, depth: usize) -> Value {
        match self.named(id, ty, depth) {
            Value::Script(id) => Value::Instance(id),
            _ if self
                .engine
                .builtins
                .contains(ty.split('[').next().unwrap_or(ty))
                && ty != "Variant" =>
            {
                Value::Other
            }
            _ => Value::Unknown,
        }
    }

    pub(super) fn builtin(&self, name: &str) -> bool {
        self.engine.builtins.contains(name) || self.engine.classes.contains_key(name)
    }

    pub(super) fn member_value(
        &self,
        id: ClassId,
        member: &Member<'a>,
        invoked: bool,
        depth: usize,
    ) -> Value {
        if depth >= 24 {
            return Value::Unknown;
        }
        if member.kind == Kind::Other {
            return Value::Other;
        }
        let source = &self.classes[id].source.text;
        if member.kind == Kind::Method {
            if !invoked {
                return Value::Other;
            }
            return member
                .node
                .child_by_field_name("return_type")
                .map_or(Value::Unknown, |ty| {
                    self.type_value(id, text(ty, source), depth)
                });
        }
        if let Some(ty) = member.node.child_by_field_name("type")
            && ty.kind() != "inferred_type"
        {
            return self.type_value(id, text(ty, source), depth);
        }
        if member.kind == Kind::Constant || member.node.child_by_field_name("type").is_some() {
            return member
                .node
                .child_by_field_name("value")
                .map_or(Value::Unknown, |value| self.initializer(id, value, depth));
        }
        Value::Unknown
    }

    fn initializer(&self, id: ClassId, value: Node<'a>, depth: usize) -> Value {
        if depth >= 24 {
            return Value::Unknown;
        }
        let source = &self.classes[id].source;
        let raw = text(value, &source.text);
        match value.kind() {
            "identifier" => self.named(id, raw, depth),
            "call" => self.load_script(id, value),
            "attribute" => self.attribute_initializer(id, value, depth),
            "binary_operator"
                if value
                    .child_by_field_name("op")
                    .is_some_and(|op| op.kind() == "as") =>
            {
                value
                    .child_by_field_name("right")
                    .map_or(Value::Unknown, |ty| {
                        self.type_value(id, text(ty, &source.text), depth)
                    })
            }
            "array" | "dictionary" | "string" | "string_name" | "integer" | "float" | "true"
            | "false" => Value::Other,
            _ => Value::Unknown,
        }
    }

    fn attribute_initializer(&self, id: ClassId, node: Node<'a>, depth: usize) -> Value {
        let source = &self.classes[id].source.text;
        let parts = children(node);
        let Some((first, rest)) = parts.split_first() else {
            return Value::Unknown;
        };
        let initial = self.initializer(id, *first, depth + 1);
        rest.iter().fold(initial, |receiver, part| {
            let name = part.named_child(0).unwrap_or(*part);
            let name = text(name, source);
            if let Value::Script(class) = receiver
                && part.kind() == "attribute_call"
                && name == "new"
            {
                return Value::Instance(class);
            }
            if let Value::Script(class) = receiver
                && part.kind() == "identifier"
                && let Some(nested) = self.nested(class, name)
            {
                return Value::Script(nested);
            }
            self.lookup(receiver, name)
                .map_or(Value::Unknown, |((owner, _), member)| {
                    self.member_value(owner, member, part.kind() == "attribute_call", depth + 1)
                })
        })
    }

    pub(super) fn load_script(&self, id: ClassId, node: Node<'a>) -> Value {
        let source = &self.classes[id].source;
        let script = node
            .named_child(0)
            .filter(|name| matches!(text(*name, &source.text), "preload" | "load"))
            .and_then(|_| node.child_by_field_name("arguments"))
            .and_then(|args| args.named_child(0))
            .filter(|path| path.kind() == "string")
            .and_then(|path| quoted(text(path, &source.text)))
            .and_then(|path| self.script_path(path, &source.path));
        script.map_or(Value::Unknown, Value::Script)
    }
}

fn root_base<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    children(node)
        .into_iter()
        .find_map(|node| {
            match node.kind() {
                "extends_statement" => Some(node),
                "class_name_statement" => node.child_by_field_name("extends"),
                _ => None,
            }?
            .named_child(0)
        })
        .map_or("RefCounted", |base| text(base, source))
}

fn member<'a>(node: Node<'a>, source: &str, pending: &[Node<'_>]) -> Option<Member<'a>> {
    let mut cursor = node.walk();
    let (kind, name) = match node.kind() {
        "function_definition" => (Kind::Method, node.child_by_field_name("name")),
        "constructor_definition" => (
            Kind::Method,
            node.children(&mut cursor)
                .find(|child| child.kind() == "_init"),
        ),
        "const_statement" => (Kind::Constant, node.child_by_field_name("name")),
        "signal_statement" | "enum_definition" => (Kind::Other, node.child_by_field_name("name")),
        kind if variable(kind) => (Kind::Variable, node.child_by_field_name("name")),
        _ => return None,
    };
    let annotations = children(node)
        .into_iter()
        .filter(|child| child.kind() == "annotations")
        .flat_map(children)
        .chain(pending.iter().copied())
        .filter_map(|annotation| annotation.named_child(0))
        .map(|name| text(name, source));
    let exposed = node.kind() == "export_variable_statement"
        || annotations.into_iter().any(|name| {
            (kind == Kind::Variable
                && (name == "export" || name.starts_with("export_"))
                && !matches!(name, "export_group" | "export_subgroup" | "export_category"))
                || (kind == Kind::Method && matches!(name, "rpc" | "abstract"))
        });
    Some(Member {
        kind,
        node,
        name: name.filter(|name| !name.is_missing())?,
        exposed: exposed || (kind == Kind::Method && node.child_by_field_name("body").is_none()),
        static_member: kind == Kind::Constant
            || children(node)
                .iter()
                .any(|child| child.kind() == "static_keyword"),
    })
}
