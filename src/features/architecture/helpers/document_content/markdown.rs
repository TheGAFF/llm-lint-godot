mod structure;
use super::Document;
use pulldown_cmark::{Event, Options, Parser, Tag};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Heading(u8),
    Paragraph,
    List(bool),
    Item,
    Table,
    TableHead,
    TableRow,
    TableCell,
    CodeBlock,
    Link,
    Inline,
    Other,
}

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub kind: Kind,
    pub line: usize,
    pub text: String,
    pub children: Vec<Node>,
}

pub(super) struct Markdown {
    pub nodes: Vec<Node>,
    pub words: usize,
}

#[derive(Clone)]
pub(crate) struct Declaration {
    pub path: String,
    pub purpose: String,
    pub line: usize,
}

impl Node {
    fn new(kind: Kind, line: usize) -> Self {
        Self {
            kind,
            line,
            text: String::new(),
            children: Vec::new(),
        }
    }

    fn append(&mut self, child: Self) {
        self.text.push_str(&child.text);
        if !matches!(child.kind, Kind::Inline | Kind::Link) {
            self.text.push('\n');
        }
        self.children.push(child);
    }

    fn linked(&self) -> bool {
        self.kind == Kind::Link || self.children.iter().any(Self::linked)
    }
}

impl Markdown {
    pub(super) fn parse(text: &str, body_offset: usize) -> Self {
        let body = &text[body_offset..];
        let mut stack = vec![Node::new(Kind::Other, 1)];
        for (event, range) in
            Parser::new_ext(body, Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS)
                .into_offset_iter()
        {
            let line = text[..body_offset + range.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            consume(event, line, &mut stack);
        }
        let root = stack.pop().unwrap_or_else(|| Node::new(Kind::Other, 1));
        let words = root.text.split_whitespace().count();
        Self {
            nodes: root.children,
            words,
        }
    }
}

fn consume(event: Event<'_>, line: usize, stack: &mut Vec<Node>) {
    match event {
        Event::Start(tag) => stack.push(Node::new(kind(&tag), line)),
        Event::End(_) => {
            if let Some(node) = stack.pop()
                && let Some(parent) = stack.last_mut()
            {
                parent.append(node);
            }
        }
        Event::Text(value)
        | Event::Code(value)
        | Event::InlineMath(value)
        | Event::DisplayMath(value)
        | Event::FootnoteReference(value)
        | Event::Html(value) => {
            if let Some(node) = stack.last_mut() {
                node.text.push_str(&value);
            }
        }
        Event::SoftBreak | Event::HardBreak => {
            if let Some(node) = stack.last_mut() {
                node.text.push('\n');
            }
        }
        Event::Rule => {
            if let Some(node) = stack.last_mut() {
                node.append(Node::new(Kind::Other, line));
            }
        }
        Event::InlineHtml(_) | Event::TaskListMarker(_) => {}
    }
}

fn kind(tag: &Tag<'_>) -> Kind {
    match tag {
        Tag::Heading { level, .. } => Kind::Heading(*level as u8),
        Tag::Paragraph => Kind::Paragraph,
        Tag::List(number) => Kind::List(number.is_some()),
        Tag::Item => Kind::Item,
        Tag::Table(_) => Kind::Table,
        Tag::TableHead => Kind::TableHead,
        Tag::TableRow => Kind::TableRow,
        Tag::TableCell => Kind::TableCell,
        Tag::CodeBlock(_) => Kind::CodeBlock,
        Tag::Link { .. } => Kind::Link,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Image { .. } => Kind::Inline,
        _ => Kind::Other,
    }
}

pub(super) fn expected_sections(role: &str) -> Vec<&'static str> {
    let mut result = vec!["Summary", "Key Files"];
    result.extend(match role {
        "specification" => vec!["Requirements", "Acceptance Criteria"],
        "guide" => vec!["Prerequisites", "Steps", "Validation"],
        "reference" => vec!["Entries"],
        "standard" => vec!["Rules", "Validation"],
        "decision" => vec!["Decision", "Rationale", "Alternatives", "Consequences"],
        "verification" => vec!["Results", "Limitations"],
        _ => vec![],
    });
    result.push("Related Documents");
    result
}

fn section<'a>(nodes: &'a [Node], name: &str) -> Option<(&'a Node, &'a [Node])> {
    let start = nodes
        .iter()
        .position(|node| node.kind == Kind::Heading(2) && node.text.trim() == name)?;
    let tail = &nodes[start + 1..];
    let length = tail
        .iter()
        .position(|node| node.kind == Kind::Heading(2))
        .unwrap_or(tail.len());
    Some((&nodes[start], &tail[..length]))
}

fn rows(table: &Node) -> Vec<Vec<&Node>> {
    table
        .children
        .iter()
        .filter(|node| matches!(node.kind, Kind::TableHead | Kind::TableRow))
        .map(|node| {
            node.children
                .iter()
                .filter(|child| child.kind == Kind::TableCell)
                .collect()
        })
        .collect()
}

pub(super) fn declarations(document: &Document<'_>) -> Vec<Declaration> {
    let Some((_, blocks)) = section(&document.markdown.nodes, "Key Files") else {
        return vec![];
    };
    let Some(table) = blocks.iter().find(|node| node.kind == Kind::Table) else {
        return vec![];
    };
    rows(table)
        .into_iter()
        .skip(1)
        .filter_map(|row| {
            if row.len() != 2 {
                return None;
            }
            Some(Declaration {
                path: row[0].text.trim().into(),
                purpose: row[1].text.trim().into(),
                line: row[0].line,
            })
        })
        .collect()
}
