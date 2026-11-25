#![cfg(test)]
mod support;
use godot_llm_lint::features::{
    duplicates::helpers::analyze_duplicates, parsing::parsers::parse_script,
};
use support::{project, write};

#[test]
fn observed_unrelated_ui_art_and_reducers_are_not_structural_clones() {
    let cases: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(include_str!("fixtures/duplicate_false_positives.json")).unwrap();
    assert_eq!(cases.len(), 6);
    for pair in cases {
        let root = tempfile::tempdir().unwrap();
        let project = project(root.path(), "");
        for (index, item) in pair.iter().enumerate() {
            let path = format!("src/features/ui/helpers/case_{index}_helpers.gd");
            let text = item["text"].as_str().unwrap();
            let parsed = parse_script(&path, text.into()).unwrap();
            assert_eq!(parsed.functions.len(), 1);
            assert_eq!(parsed.functions[0].name, item["item"].as_str().unwrap());
            assert!(parsed.functions[0].tokens.len() >= 50, "{item}");
            write(root.path(), &path, text);
        }
        let report = analyze_duplicates(&project, &[]).unwrap();
        assert!(
            !report.diagnostics.iter().any(|f| f.kind == "similar"
                && f.occurrences[0].item == pair[0]["item"].as_str().unwrap()
                && f.occurrences[1].item == pair[1]["item"].as_str().unwrap()),
            "Observed false positive survived: {pair:?}"
        );
    }
}

#[test]
fn local_bindings_preserve_members_types_globals_and_shadowing() {
    let text = "extends RefCounted\nfunc inspect(input: int, data: Data) -> int:\n\tvar value: int = input + data.input + API.convert(input)\n\tfor input: int in [1, 2]:\n\t\tvalue += input\n\treturn value + input + BASE\n";
    let parsed = parse_script("one.gd", text.into()).unwrap();
    let tokens = &parsed.functions[0].tokens;
    let inputs: Vec<_> = tokens.iter().filter(|t| t.text == "input").collect();
    assert!(inputs[0].binding.is_some());
    assert!(
        inputs[1].binding.is_none(),
        "member data.input is not a local"
    );
    assert_ne!(
        inputs[0].binding, inputs[3].binding,
        "loop shadows the parameter"
    );
    assert_eq!(inputs[0].binding, inputs.last().unwrap().binding);
    assert!(
        tokens
            .iter()
            .filter(|t| ["int", "API", "convert", "BASE"].contains(&t.text.as_str()))
            .all(|t| t.binding.is_none())
    );
}

#[test]
fn renamed_detection_never_substitutes_external_members_or_callees() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\n",
    );
    let one = "extends Node\nfunc run_one(input: int) -> void:\n\tself.first(input)\n\tself.first(input)\n\tself.first(input)\n\tself.first(input)\n";
    write(root.path(), "src/features/ui/nodes/one_node.gd", one);
    write(
        root.path(),
        "src/features/ui/nodes/two_node.gd",
        &one.replace("run_one", "run_two").replace("first", "second"),
    );
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn pure_capture_tables_do_not_create_structural_algorithm_suggestions() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\n",
    );
    for (name, time) in [("one", 1), ("two", 8)] {
        let text = format!(
            "extends Node\nfunc jobs() -> Array:\n\treturn [\n\t\t[\"idle\", {time}, false],\n\t\t[\"hit\", {time}, true],\n\t\t[\"finish\", {time}, false],\n\t]\n"
        );
        write(
            root.path(),
            &format!("src/features/ui/testing/{name}_test_harness.gd"),
            &text,
        );
    }
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert!(!report.diagnostics.iter().any(|f| f.kind == "similar"));
}

#[test]
fn fingerprints_ignore_formatting_comments_and_include_signature_and_values() {
    let one = "func first(input: int) -> int:\n\treturn input + 1\n";
    let hash = |text: &str| {
        parse_script("one.gd", text.into()).unwrap().functions[0]
            .fingerprint
            .clone()
    };
    assert_eq!(
        hash(one),
        hash(
            &one.replace("\treturn", "\t# review comment\n\treturn")
                .replace(" + ", "+")
        )
    );
    assert_ne!(hash(one), hash(&one.replace("+ 1", "+ 2")));
    assert_ne!(hash(one), hash(&one.replace("input: int", "input: float")));
}

#[test]
fn indexed_members_and_literal_dictionary_keys_cannot_be_alpha_renamed() {
    let text = "func inspect(items: Array, object: Node) -> Dictionary:\n\tvar first = object.items[items[0]]\n\tvar second = {items = items}\n\treturn {items: items}\n";
    let parsed = parse_script("one.gd", text.into()).unwrap();
    assert!(parsed.diagnostics.is_empty());
    let occurrences: Vec<_> = parsed.functions[0]
        .tokens
        .iter()
        .filter(|t| t.text == "items")
        .collect();
    assert_eq!(occurrences.len(), 6);
    assert!(occurrences[0].binding.is_none(), "indexed external member");
    assert!(
        occurrences[1].binding.is_some(),
        "local subscript expression"
    );
    assert!(
        occurrences[2].binding.is_none(),
        "literal Lua-style dictionary key"
    );
    assert!(occurrences[3..].iter().all(|t| t.binding.is_some()));
}
