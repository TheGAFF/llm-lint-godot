use super::{Document, Kind, Node, expected_sections, rows, section};
use crate::shared::models::Diagnostic;

impl Document<'_> {
    pub(in crate::features::architecture::helpers::document_content) fn validate_markdown_structure(
        &self,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        blueprint(self, diagnostics);
        nesting(self, &self.markdown.nodes, 0, false, diagnostics);
        for name in expected_sections(self.role) {
            let Some((heading, blocks)) = section(&self.markdown.nodes, name) else {
                continue;
            };
            validate_section(self, name, heading, blocks, diagnostics);
        }
    }
}

fn blueprint(document: &Document<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let nodes = &document.markdown.nodes;
    let titles: Vec<_> = nodes
        .iter()
        .filter(|node| node.kind == Kind::Heading(1))
        .collect();
    if titles.len() != 1
        || titles
            .first()
            .is_some_and(|title| title.text.trim().is_empty())
        || nodes
            .first()
            .is_none_or(|node| node.kind != Kind::Heading(1))
    {
        diagnostics.push(document.diagnostic(
            document.metadata.body_line,
            "blueprint",
            "Missing, repeated, or misplaced H1 title",
            "Begin the body with exactly one H1 title",
        ));
    }
    let headings: Vec<_> = nodes
        .iter()
        .filter(|node| node.kind == Kind::Heading(2))
        .collect();
    let actual: Vec<_> = headings.iter().map(|node| node.text.trim()).collect();
    let expected = expected_sections(document.role);
    if actual != expected {
        let position = actual
            .iter()
            .zip(&expected)
            .position(|(a, e)| a != e)
            .unwrap_or(actual.len().min(expected.len()));
        let line = headings
            .get(position)
            .map_or(document.metadata.body_line, |node| node.line);
        diagnostics.push(document.diagnostic(
            line,
            "blueprint",
            &actual.join(" → "),
            &format!("Use these H2 sections in order: {}", expected.join(" → ")),
        ));
    }
    let mut current = "";
    for (index, node) in nodes.iter().enumerate() {
        if node.kind == Kind::Heading(2) {
            current = node.text.trim();
        }
        if node.kind == Kind::Heading(3)
            && (current.is_empty()
                || ["Summary", "Key Files", "Related Documents"].contains(&current))
        {
            diagnostics.push(document.diagnostic(
                node.line,
                "blueprint",
                "H3 outside a role-specific section",
                "Use H3 grouping only inside role-specific sections",
            ));
        }
        if current.is_empty() && index != 0 {
            diagnostics.push(document.diagnostic(
                node.line,
                "blueprint",
                "Content outside a required section",
                "Place content under the required H2 sections",
            ));
        }
    }
}

fn nesting(
    document: &Document<'_>,
    nodes: &[Node],
    depth: usize,
    nested: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        let next = depth + usize::from(matches!(node.kind, Kind::List(_)));
        if next > 2 && matches!(node.kind, Kind::List(_)) {
            diagnostics.push(document.diagnostic(
                node.line,
                "structure",
                "List nested more than two levels",
                "Flatten the list or split a focused topic",
            ));
        }
        if matches!(node.kind, Kind::Heading(4..=6))
            || (nested && matches!(node.kind, Kind::Heading(_)))
        {
            diagnostics.push(document.diagnostic(
                node.line,
                "blueprint",
                "Deep or nested heading",
                "Use H1 title, required H2 sections, and optional top-level H3 topic groups",
            ));
        }
        nesting(document, &node.children, next, true, diagnostics);
    }
}

fn validate_section(
    document: &Document<'_>,
    name: &str,
    heading: &Node,
    blocks: &[Node],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let content: Vec<_> = blocks
        .iter()
        .filter(|node| node.kind != Kind::Heading(3))
        .collect();
    let valid = match name {
        "Summary" => {
            content.len() == 1
                && content[0].kind == Kind::Paragraph
                && !content[0].text.trim().is_empty()
                && content[0].text.split_whitespace().count() <= 80
        }
        "Key Files" => key_files(&content),
        "Related Documents" => related(&content),
        "Entries" => tables(&content, None),
        "Results" => tables(&content, Some(&["Date", "Check", "Result", "Evidence"])),
        "Steps" => lists(&content, true),
        _ => lists(&content, false),
    };
    if !valid {
        let line = content.first().map_or(heading.line, |node| node.line);
        diagnostics.push(document.diagnostic(
            line,
            "structure",
            &format!("Invalid or empty {name} section"),
            expectation(name),
        ));
    }
}

fn expectation(name: &str) -> &'static str {
    match name {
        "Summary" => "One nonempty paragraph of at most 80 words",
        "Key Files" => "One Path | Purpose table with at least one nonempty declaration",
        "Related Documents" => "An unordered list whose entries contain Markdown links",
        "Entries" => "Tables with at least two nonempty column headings and one data row",
        "Results" => "Date | Check | Result | Evidence tables with dated evidence rows",
        "Steps" => "An ordered list; attach fenced examples to the relevant list entry",
        _ => "An unordered list; attach examples to entries, or explain Not applicable: <reason>",
    }
}

fn key_files(content: &[&Node]) -> bool {
    if content.len() != 1 || !tables(content, Some(&["Path", "Purpose"])) {
        return false;
    }
    rows(content[0])
        .iter()
        .skip(1)
        .all(|row| row.len() == 2 && row.iter().all(|cell| !cell.text.trim().is_empty()))
}

fn related(content: &[&Node]) -> bool {
    !content.is_empty()
        && content.iter().all(|node| {
            node.kind == Kind::List(false)
                && !node.children.is_empty()
                && node.children.iter().all(Node::linked)
        })
}

fn lists(content: &[&Node], ordered: bool) -> bool {
    if content.len() == 1 && content[0].kind == Kind::Paragraph {
        let text = content[0].text.trim();
        return text.starts_with("Not applicable:") && text.len() > "Not applicable:".len();
    }
    !content.is_empty()
        && content.iter().all(|node| {
            node.kind == Kind::List(ordered)
                && !node.children.is_empty()
                && node
                    .children
                    .iter()
                    .all(|item| !item.text.trim().is_empty())
        })
}

fn tables(content: &[&Node], expected: Option<&[&str]>) -> bool {
    !content.is_empty()
        && content.iter().all(|table| {
            if table.kind != Kind::Table {
                return false;
            }
            let rows = rows(table);
            let Some(header) = rows.first() else {
                return false;
            };
            let names: Vec<_> = header.iter().map(|cell| cell.text.trim()).collect();
            rows.len() > 1
                && names.len() >= 2
                && names.iter().all(|name| !name.is_empty())
                && expected.is_none_or(|columns| columns == names)
                && rows.iter().skip(1).all(|row| {
                    if let Some(columns) = expected {
                        row.len() == columns.len()
                            && row.iter().all(|cell| !cell.text.trim().is_empty())
                    } else {
                        row.iter().any(|cell| !cell.text.trim().is_empty())
                    }
                })
        })
}
