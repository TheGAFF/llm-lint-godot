use super::{
    io,
    report::{Edge, Possible, Report},
    serialized,
};
use crate::{
    features::{
        architecture::helpers::{
            ScriptTrees,
            document_content::{key_files, source_declaration_matches},
            reference_symbols,
        },
        configuration::{
            helpers::inside,
            models::{DynamicLoad, Project},
        },
        parsing::helpers::{
            column,
            resource_paths::{parent, resolve},
            unquote,
        },
    },
    shared::models::{Corpus, LoadCallee, Outcome, Reference, ReferenceOrigin, Source},
};

pub(super) struct Inventory<'a> {
    project: &'a Project,
    corpus: &'a Corpus,
}

pub(super) fn find(
    project: &Project,
    corpus: &Corpus,
    trees: &ScriptTrees,
    report: &mut Report,
) -> Outcome<()> {
    let inventory = Inventory { project, corpus };
    for source in corpus.sources.iter().chain(&corpus.excluded_sources) {
        if let Some(tree) = trees.get(&source.path) {
            inventory.script(source, tree, report);
        } else if !source.text.is_empty() {
            serialized::scan(&inventory, source, report);
        }
    }
    let subject = report.subject.path.clone();
    reference_symbols(
        corpus,
        trees,
        &subject,
        |source, target, node, kind, name| {
            let mut edge = Edge::new(
                &source.path,
                node.start_position().row + 1,
                Some(column(node.start_byte(), &source.text)),
                kind,
                name,
            );
            edge.target = Some(target.into());
            report.direct(edge);
        },
    )?;
    inventory.documents(report);
    for path in corpus
        .paths
        .iter()
        .filter(|path| path.ends_with(".scn") || path.ends_with(".res"))
    {
        if inventory.exists(path) {
            report.gap(
                Edge::new(path, 1, None, "binary_resource", path),
                "Binary Godot scenes and resources have no readable reference locations",
            );
        }
    }
    Ok(())
}

impl Inventory<'_> {
    pub fn resolve(&self, file: &str, written: &str) -> Option<String> {
        resolve(&self.corpus.uids, parent(file), written)
    }

    pub fn exists(&self, path: &str) -> bool {
        (path.is_empty() || self.corpus.paths.contains(path))
            && self.project.root.join(path).is_file()
    }

    pub fn resource(&self, edge: Edge, directory: bool, report: &mut Report) {
        self.resource_with_bounds(edge, directory, None, report);
    }

    fn resource_with_bounds(
        &self,
        mut edge: Edge,
        directory: bool,
        item: Option<&str>,
        report: &mut Report,
    ) {
        let Some(target) = self
            .resolve(&edge.source, &edge.written)
            .filter(|_| !edge.written.is_empty())
        else {
            report.unresolved(edge, "Could not resolve path or UID");
            return;
        };
        let outbound = edge.source == report.subject.path;
        let maybe_inbound = inside(&report.subject.path, &target);
        if !outbound && !maybe_inbound {
            return;
        }
        if (target.is_empty() || self.corpus.paths.contains(&target))
            && self.project.root.join(&target).is_dir()
        {
            let root = format!("res://{target}");
            let bounds = item
                .and_then(|item| self.bounds(&edge.source, item))
                .filter(|bounds| bounds.roots.iter().any(|allowed| inside(&root, allowed)));
            let inbound = maybe_inbound
                && bounds.is_none_or(|bounds| bounds.matches_extension(&report.subject.path));
            let extensions = bounds.map_or_else(Vec::new, |bounds| bounds.extensions.clone());
            possible(report, edge, vec![root], extensions, inbound);
        } else if !directory && self.exists(&target) {
            edge.target = Some(target);
            report.direct(edge);
        } else {
            edge.target = Some(target);
            report.unresolved(
                edge,
                if directory {
                    "Directory is missing from the inventory"
                } else {
                    "File is missing from the inventory"
                },
            );
        }
    }

    fn script(&self, source: &Source, tree: &tree_sitter::Tree, report: &mut Report) {
        let accesses = io::accesses(tree, &source.text);
        let location = |node: tree_sitter::Node<'_>| {
            (
                node.start_position().row + 1,
                column(node.start_byte(), &source.text),
            )
        };
        let arguments: Vec<_> = source
            .references
            .iter()
            .filter_map(|reference| match reference.origin {
                ReferenceOrigin::Load { argument, .. } => Some(argument.0..argument.1),
                _ => None,
            })
            .chain(accesses.iter().map(|access| access.argument.byte_range()))
            .collect();
        for reference in &source.references {
            let (kind, dynamic, literal) = match reference.origin {
                ReferenceOrigin::Load {
                    callee, literal, ..
                } => {
                    let kind = match callee {
                        LoadCallee::Load => "load",
                        LoadCallee::Preload => "preload",
                        LoadCallee::ResourceLoader => "resource_loader",
                    };
                    let written = literal.map(|(start, end)| unquote(&source.text[start..end]));
                    let dynamic = written
                        .as_deref()
                        .map_or(reference.dynamic, external_scheme);
                    (kind, dynamic, written)
                }
                ReferenceOrigin::Literal { offset }
                    if arguments.iter().any(|span| span.contains(&offset)) =>
                {
                    continue;
                }
                ReferenceOrigin::Literal { .. } => ("resource_literal", false, None),
                ReferenceOrigin::Extends => ("extends", false, None),
                ReferenceOrigin::Serialized => continue,
            };
            let mut edge = entry(source, reference, kind);
            if let Some(written) = literal {
                edge.written = written;
            }
            if dynamic {
                self.dynamic(&reference.item, edge, report);
            } else {
                let directory = reference.directory || edge.written.ends_with('/');
                self.resource_with_bounds(edge, directory, Some(&reference.item), report);
            }
        }
        for access in accesses {
            let (line, column) = location(access.site);
            self.access(source, &access, line, column, report);
        }
        for diagnostic in &source.diagnostics {
            if diagnostic.rule == "architecture/syntax" {
                report.gap(
                    Edge::new(
                        &source.path,
                        diagnostic.line,
                        Some(diagnostic.column),
                        "script_syntax",
                        &diagnostic.actual,
                    ),
                    "GDScript parser could not scan this syntax",
                );
            }
        }
    }

    fn access(
        &self,
        source: &Source,
        access: &io::ResourceAccess<'_>,
        line: usize,
        column: usize,
        report: &mut Report,
    ) {
        let literal = access
            .literal
            .map(|node| unquote(&source.text[node.byte_range()]));
        let targets = literal
            .as_ref()
            .map_or(access.targets.as_slice(), std::slice::from_ref);
        for target in targets {
            let edge = Edge::new(&source.path, line, Some(column), access.kind, target);
            if target.starts_with("res://")
                || target.starts_with("uid://")
                || (access.literal.is_some() && !target.contains("://"))
            {
                self.resource_with_bounds(edge, access.directory, Some(access.item), report);
            } else {
                self.dynamic(access.item, edge, report);
            }
        }
    }

    fn bounds(&self, source: &str, item: &str) -> Option<&DynamicLoad> {
        self.project
            .settings
            .dynamic_loads
            .iter()
            .find(|bounds| bounds.path == source && bounds.item == item)
    }

    fn dynamic(&self, item: &str, edge: Edge, report: &mut Report) {
        let Some(bounds) = self.bounds(&edge.source, item) else {
            report.gap(edge, "Computed load has no configured roots");
            return;
        };
        let inbound = !edge.written.starts_with("user://")
            && bounds.matches_extension(&report.subject.path)
            && bounds
                .roots
                .iter()
                .filter_map(|root| root.strip_prefix("res://"))
                .any(|root| inside(&report.subject.path, root.trim_end_matches('/')));
        possible(
            report,
            edge,
            bounds.roots.clone(),
            bounds.extensions.clone(),
            inbound,
        );
    }

    fn documents(&self, report: &mut Report) {
        for document in &self.corpus.documents {
            for declaration in key_files(document) {
                self.document_row(&document.path, &declaration.path, declaration.line, report);
            }
        }
    }

    fn document_row(&self, document: &str, pattern: &str, line: usize, report: &mut Report) {
        let glob = pattern.contains(['*', '?']);
        let kind = if glob { "key_files_glob" } else { "key_files" };
        let edge = Edge::new(document, line, None, kind, pattern);
        let applies = |path: &str| source_declaration_matches(pattern, path, document);
        if document != report.subject.path {
            if applies(&report.subject.path) {
                report.direct(Edge {
                    target: Some(report.subject.path.clone()),
                    ..edge
                });
            }
            return;
        }
        let mut matches = if glob {
            self.corpus
                .paths
                .iter()
                .map(String::as_str)
                .filter(|path| applies(path))
                .collect()
        } else {
            vec![pattern]
        };
        matches.retain(|path| applies(path) && self.exists(path));
        if matches.is_empty() {
            report.unresolved(edge, "Key Files declaration has no indexed file matches");
        } else {
            for path in matches {
                report.direct(Edge {
                    target: Some(path.into()),
                    ..edge.clone()
                });
            }
        }
    }
}

pub(super) fn entry(source: &Source, reference: &Reference, kind: &'static str) -> Edge {
    Edge::new(
        &source.path,
        reference.line,
        Some(reference.column),
        kind,
        &reference.target,
    )
}

fn possible(
    report: &mut Report,
    edge: Edge,
    mut roots: Vec<String>,
    mut extensions: Vec<String>,
    inbound: bool,
) {
    roots.sort_unstable();
    extensions.sort_unstable();
    if inbound {
        let mut candidate = edge.clone();
        candidate.target = Some(report.subject.path.clone());
        report.possible.push(Possible {
            edge: candidate,
            direction: "incoming",
            roots: roots.clone(),
            extensions: extensions.clone(),
        });
    }
    if edge.source == report.subject.path {
        report.possible.push(Possible {
            edge,
            direction: "outgoing",
            roots,
            extensions,
        });
    }
}

fn external_scheme(path: &str) -> bool {
    path.contains("://") && !path.starts_with("res://") && !path.starts_with("uid://")
}
