use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub(super) struct Subject {
    pub path: String,
    pub owner: Option<String>,
    pub uids: Vec<String>,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) struct Location {
    pub file: String,
    pub line: usize,
    pub column: Option<usize>,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) struct Edge {
    pub location: Location,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub target: Option<String>,
    pub source: String,
    pub written: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

impl Edge {
    pub fn new(
        file: &str,
        line: usize,
        column: Option<usize>,
        kind: &'static str,
        written: &str,
    ) -> Self {
        Self {
            location: Location {
                file: file.into(),
                line,
                column,
            },
            kind,
            target: None,
            source: file.into(),
            written: written.into(),
            aliases: Vec::new(),
            reason: None,
        }
    }

    fn site(&self) -> String {
        let Location { file, line, column } = &self.location;
        column.map_or_else(
            || format!("{file}:{line}"),
            |column| format!("{file}:{line}:{column}"),
        )
    }

    fn print(&self) {
        let target = self.target.as_deref().unwrap_or(&self.written);
        print!("  {} [{}] -> {target}", self.site(), self.kind);
        if target != self.written && !self.written.is_empty() {
            print!(" ({})", self.written);
        }
        if !self.aliases.is_empty() {
            print!(" (also {})", self.aliases.join(", "));
        }
        if let Some(reason) = self.reason {
            print!(" | {reason}");
        }
        println!();
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) struct Possible {
    #[serde(flatten)]
    pub edge: Edge,
    pub direction: &'static str,
    pub roots: Vec<String>,
    pub extensions: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct Report {
    version: u8,
    pub subject: Subject,
    pub incoming: Vec<Edge>,
    pub outgoing: Vec<Edge>,
    pub possible: Vec<Possible>,
    pub unresolved: Vec<Edge>,
    pub coverage_gaps: Vec<Edge>,
}

impl Report {
    pub fn new(subject: Subject) -> Self {
        Self {
            version: 1,
            subject,
            incoming: vec![],
            outgoing: vec![],
            possible: vec![],
            unresolved: vec![],
            coverage_gaps: vec![],
        }
    }

    pub fn direct(&mut self, edge: Edge) {
        if edge.target.as_deref() == Some(&self.subject.path) {
            self.incoming.push(edge.clone());
        }
        if edge.source == self.subject.path {
            self.outgoing.push(edge);
        }
    }

    pub fn unresolved(&mut self, mut edge: Edge, reason: &'static str) {
        if edge.source == self.subject.path {
            edge.reason = Some(reason);
            self.unresolved.push(edge);
        }
    }

    pub fn gap(&mut self, mut edge: Edge, reason: &'static str) {
        edge.reason = Some(reason);
        self.coverage_gaps.push(edge);
    }

    pub fn sort(&mut self) {
        self.incoming.sort_unstable();
        self.incoming.dedup();
        self.outgoing.sort_unstable();
        self.outgoing.dedup();
        self.possible.sort_unstable();
        self.possible.dedup();
        self.unresolved.sort_unstable();
        self.unresolved.dedup();
        self.coverage_gaps.sort_unstable();
        self.coverage_gaps.dedup();
    }

    pub fn print(&self) {
        println!("{}", self.subject.path);
        if let Some(owner) = &self.subject.owner {
            println!("Owner: {owner}");
        }
        if !self.subject.uids.is_empty() {
            println!("UIDs: {}", self.subject.uids.join(", "));
        }
        println!("Incoming ({})", self.incoming.len());
        self.incoming.iter().for_each(Edge::print);
        self.print_outgoing();
        println!("Possible ({})", self.possible.len());
        for possible in &self.possible {
            let edge = &possible.edge;
            let extensions = if possible.extensions.is_empty() {
                "any".into()
            } else {
                possible.extensions.join(", ")
            };
            println!(
                "  {} [{} {}] {} (roots: {}; extensions: {extensions})",
                edge.site(),
                possible.direction,
                edge.kind,
                edge.written,
                possible.roots.join(", ")
            );
        }
        println!("Unresolved ({})", self.unresolved.len());
        self.unresolved.iter().for_each(Edge::print);
        println!("Coverage gaps ({})", self.coverage_gaps.len());
        self.coverage_gaps.iter().for_each(Edge::print);
    }

    fn print_outgoing(&self) {
        println!("Outgoing ({})", self.outgoing.len());
        let mut counts = BTreeMap::new();
        for edge in &self.outgoing {
            if edge.kind == "key_files_glob" {
                *counts.entry((&edge.location, &edge.written)).or_insert(0) += 1;
            }
        }
        for edge in &self.outgoing {
            if edge.kind != "key_files_glob" {
                edge.print();
            } else if let Some(count) = counts.remove(&(&edge.location, &edge.written)) {
                println!(
                    "  {} [{}] {} ({count} matches)",
                    edge.site(),
                    edge.kind,
                    edge.written
                );
            }
        }
    }
}
