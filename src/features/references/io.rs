use crate::features::parsing::parsers::{io_call, literal_argument, path_arguments};
use tree_sitter::{Node, Tree};

pub(super) struct ResourceAccess<'a> {
    pub site: Node<'a>,
    pub argument: Node<'a>,
    pub literal: Option<Node<'a>>,
    pub kind: &'static str,
    pub directory: bool,
    pub targets: Vec<String>,
    pub item: &'a str,
}

pub(super) fn accesses<'a>(tree: &'a Tree, text: &'a str) -> Vec<ResourceAccess<'a>> {
    let mut accesses = Vec::new();
    scan(tree.root_node(), text, "<script>", &mut accesses);
    accesses
}

fn scan<'a>(node: Node<'a>, text: &'a str, item: &'a str, accesses: &mut Vec<ResourceAccess<'a>>) {
    if node.is_error() || node.is_missing() {
        return;
    }
    let item = if matches!(
        node.kind(),
        "function_definition" | "constructor_definition"
    ) {
        node.child_by_field_name("name")
            .map_or(item, |name| &text[name.byte_range()])
    } else {
        item
    };
    if let Some((kind, argument)) = io_call(node, text, true)
        && matches!(kind, "directory_access" | "scene_load")
    {
        accesses.push(ResourceAccess {
            site: node,
            argument,
            literal: literal_argument(argument),
            kind,
            directory: kind == "directory_access",
            targets: path_arguments(argument, node, text),
            item,
        });
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        scan(child, text, item, accesses);
    }
}
