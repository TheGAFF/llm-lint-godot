use super::{ClassId, Index, Usage, children, scenes, text, visit};
use crate::features::architecture::helpers::sources::ScriptTrees;
use crate::features::parsing::helpers::resource_paths::resolve;
use crate::shared::models::{Corpus, Outcome, Source};
use std::collections::BTreeMap;
use tree_sitter::Node;

#[derive(Clone, Copy)]
struct Binding<'a> {
    path: &'a str,
    kind: &'static str,
}

pub(super) struct Symbols<'a> {
    bindings: BTreeMap<&'a str, Option<Binding<'a>>>,
    uses: Vec<(&'a Source, &'a str, Node<'a>, &'static str, &'a str)>,
    subject: &'a str,
}

pub(crate) fn reference_symbols(
    corpus: &Corpus,
    trees: &ScriptTrees,
    subject: &str,
    mut each: impl FnMut(&Source, &str, Node<'_>, &'static str, &str),
) -> Outcome<()> {
    let scenes = scenes::Scenes::new(corpus);
    let index = Index::build(corpus, trees, &scenes)?;
    let mut usage = Usage {
        symbols: Some(Symbols::new(corpus, subject)),
        ..Usage::default()
    };
    let mut diagnostics = Vec::new();
    for (id, class) in index.classes.iter().enumerate() {
        visit::script(&index, &mut usage, &mut diagnostics, id, class);
        diagnostics.clear();
        if let Some(symbols) = &mut usage.symbols {
            symbols.types(&index, id, class.body);
            if let Some(base) = class
                .body
                .parent()
                .and_then(|node| node.child_by_field_name("extends"))
            {
                symbols.types(&index, id, base);
            }
        }
    }
    if let Some(symbols) = usage.symbols {
        for (source, path, node, kind, name) in symbols.uses {
            each(source, path, node, kind, name);
        }
    }
    Ok(())
}

impl<'a> Symbols<'a> {
    fn new(corpus: &'a Corpus, subject: &'a str) -> Self {
        let mut symbols = Self {
            bindings: BTreeMap::new(),
            uses: vec![],
            subject,
        };
        for source in corpus.sources.iter().chain(&corpus.excluded_sources) {
            if let Some(name) = &source.class_name {
                symbols.insert(name, &source.path, "global_class");
            }
        }
        for (name, raw) in scenes::global_autoloads(corpus) {
            if let Some(path) = resolve(&corpus.uids, "", raw)
                && let Some(path) = corpus.paths.get(&path)
            {
                symbols.insert(name, path, "autoload_use");
            }
        }
        symbols
    }

    fn insert(&mut self, name: &'a str, path: &'a str, kind: &'static str) {
        self.bindings
            .entry(name)
            .and_modify(|entry| *entry = None)
            .or_insert(Some(Binding { path, kind }));
    }

    pub(super) fn record(
        &mut self,
        index: &Index<'a>,
        id: ClassId,
        node: Node<'a>,
        extends: bool,
        type_only: bool,
    ) {
        let source = index.classes[id].source;
        let name = text(node, &source.text);
        let Some(binding) = self.bindings.get(name).copied().flatten() else {
            return;
        };
        if (source.path != self.subject && binding.path != self.subject)
            || source.path == binding.path
            || (type_only && binding.kind == "autoload_use")
            || index.lexical_name(id, name, 0).is_some()
        {
            return;
        }
        let kind = if extends { "extends" } else { binding.kind };
        self.uses.push((source, binding.path, node, kind, name));
    }

    fn types(&mut self, index: &Index<'a>, id: ClassId, node: Node<'a>) {
        match node.kind() {
            "class_definition" | "ERROR" => (),
            "extends_statement" => {
                if let Some(ty) = node.named_child(0).filter(|node| node.kind() == "type") {
                    self.type_expression(index, id, ty, true);
                }
            }
            "type" => self.type_expression(index, id, node, false),
            _ => {
                for child in children(node) {
                    self.types(index, id, child);
                }
            }
        }
    }

    fn type_expression(&mut self, index: &Index<'a>, id: ClassId, node: Node<'a>, extends: bool) {
        match node.kind() {
            "identifier" => self.record(index, id, node, extends, true),
            "attribute" => {
                if let Some(base) = node.named_child(0) {
                    self.type_expression(index, id, base, extends);
                }
            }
            _ => {
                for child in children(node) {
                    self.type_expression(index, id, child, extends);
                }
            }
        }
    }
}
