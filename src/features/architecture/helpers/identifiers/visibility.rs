mod audit;
mod index;
mod scenes;
mod symbols;
mod visit;
pub(crate) use audit::audit_symbols;
pub(crate) use symbols::reference_symbols;

use super::{children, text};
use crate::{
    features::parsing::helpers::column,
    shared::{
        helpers::issue,
        models::{Corpus, Diagnostic, Outcome, Source},
    },
};
use index::{ClassId, Index, Kind, MemberId, Value};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

const REQUIRED: &str = "architecture/member-underscore-required";
const ACCESS: &str = "architecture/private-member-access";
const LOCAL: &str = "architecture/local-variable-snake-case";

pub(super) fn check(
    corpus: &Corpus,
    trees: &crate::features::architecture::helpers::sources::ScriptTrees,
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<()> {
    let scenes = scenes::Scenes::new(corpus);
    let index = Index::build(corpus, trees, &scenes)?;
    let mut usage = Usage::default();
    for (id, class) in index.classes.iter().enumerate() {
        visit::script(&index, &mut usage, diagnostics, id, class);
        index.expose_overrides(id, &mut usage.external);
    }
    scenes.expose(&index, &mut usage);
    for id in usage.internal.difference(&usage.external) {
        let class = &index.classes[id.0];
        let member = &class.members[id.1];
        if !class.lint || class.body.has_error() || member.exposed || id.1.starts_with('_') {
            continue;
        }
        if usage.possibly_external(class.source.path.as_str(), id.1, member.kind) {
            continue;
        }
        diagnostics.push(at(
            class.source,
            member.name,
            REQUIRED,
            id.1,
            id.1,
            &format!("Same-file member should be named _{}", id.1),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Access {
    Any,
    Call,
    Method,
    MethodCall,
    Variable,
}

impl Access {
    fn matches(self, kind: Kind) -> bool {
        match self {
            Self::Any => kind.is_checked(),
            Self::Call => matches!(kind, Kind::Method | Kind::Variable),
            Self::Method | Self::MethodCall => kind == Kind::Method,
            Self::Variable => kind == Kind::Variable,
        }
    }

    fn audit_matches(self, kind: Kind) -> bool {
        self.matches(kind) || (kind == Kind::Constant && matches!(self, Self::Any | Self::Variable))
    }
}

#[derive(Default)]
struct Usage<'a> {
    internal: BTreeSet<MemberId<'a>>,
    external: BTreeSet<MemberId<'a>>,
    uncertain: BTreeMap<&'a str, BTreeSet<(&'a str, Access)>>,
    symbols: Option<symbols::Symbols<'a>>,
    audit: Option<audit::Audit<'a>>,
}

impl<'a> Usage<'a> {
    fn uncertain(&mut self, file: &'a str, name: &'a str, access: Access) {
        if let Some(audit) = &mut self.audit {
            audit.named(name, access);
        }
        if !name.starts_with('_') {
            self.uncertain
                .entry(name)
                .or_default()
                .insert((file, access));
        }
    }

    fn possibly_external(&self, file: &str, name: &str, kind: Kind) -> bool {
        self.uncertain.get(name).is_some_and(|uses| {
            uses.iter()
                .any(|(path, access)| *path != file && access.matches(kind))
        })
    }
}

fn at(
    source: &Source,
    node: Node<'_>,
    rule: &str,
    item: &str,
    actual: &str,
    expected: &str,
) -> Diagnostic {
    issue(
        &source.path,
        node.start_position().row + 1,
        column(node.start_byte(), &source.text),
        rule,
        item,
        actual,
        expected,
    )
}

fn quoted(raw: &str) -> Option<&str> {
    let raw = raw.strip_prefix('&').unwrap_or(raw);
    let quote = raw.chars().next().filter(|ch| matches!(ch, '"' | '\''))?;
    let value = raw.strip_prefix(quote)?.strip_suffix(quote)?;
    (!value.contains(['\\', quote, '\r', '\n'])).then_some(value)
}

fn variable(kind: &str) -> bool {
    matches!(
        kind,
        "variable_statement" | "export_variable_statement" | "onready_variable_statement"
    )
}
