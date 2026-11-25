#![cfg(test)]
use godot_llm_lint::features::configuration::helpers::contracts::{
    Options, captures, missing_companions, options, plural, snake, validity,
};
use serde_json::json;
#[test]
fn contract_configuration_rejects_ambiguous_or_unsafe_options() {
    let mapper = "architecture/mapper-return-type";
    let companion = "architecture/companion-files";
    for values in [
        json!({"unknown":1}),
        json!({"pluralForms":{"bad-name":"Names"}}),
        json!({"pluralForms":{"Name":"bad-name"}}),
        json!({"wrappers":[{"module":"","export":"Box","argument":0}]}),
        json!({"wrappers":[{"module":"types","export":"bad-name","argument":0}]}),
    ] {
        assert!(validity(mapper, &serde_json::from_value(values).unwrap()).is_err());
    }
    for pair in [
        json!({"match":"x","require":[]}),
        json!({"match":"a//b","require":["x"]}),
        json!({"match":"a/../b","require":["x"]}),
        json!({"match":"a\\b","require":["x"]}),
        json!({"match":"{a}/{a}","require":["x"]}),
        json!({"match":"{broken","require":["x"]}),
        json!({"match":"bad}","require":["x"]}),
        json!({"match":"bad}/{x}","require":["x"]}),
        json!({"match":"{1bad}","require":["x"]}),
        json!({"match":"x","require":["/absolute"]}),
        json!({"match":"x","require":["a//b"]}),
        json!({"match":"x","require":["*.rs"]}),
        json!({"match":"{name}","require":["${unknown}"]}),
    ] {
        assert!(
            validity(
                companion,
                &serde_json::from_value(json!({"pairs":{"pair":pair}})).unwrap()
            )
            .is_err()
        );
    }
    assert!(
        validity(
            companion,
            &serde_json::from_value(json!({"pairs":{"Bad ID":false}})).unwrap()
        )
        .is_err()
    );
    assert!(
        validity(
            "architecture/mapper-output-declaration",
            &serde_json::from_value(json!({"externalTypes":["*"]})).unwrap()
        )
        .is_err()
    );
    assert!(!validity("unknown", &Default::default()).unwrap());
    assert!(validity(companion,&serde_json::from_value(json!({"pairs":{"skip":false,"pair":{"match":"src/**/{name}.rs","require":["../types/${name}.md"]}}})).unwrap()).unwrap());
}
#[test]
fn option_merging_captures_plurals_and_companion_containment() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/types")).unwrap();
    let rules=serde_json::from_value(json!({"architecture/mapper-return-type":{"options":{"pluralForms":{"Person":"People"}}},"architecture/companion-files":{"options":{"pairs":{"skip":false,"notes":{"match":"src/{name}.rs","require":["types/${name}.md"]}}}}})).unwrap();
    let overrides: Vec<godot_llm_lint::features::configuration::models::RuleOverride>=serde_json::from_value(json!([{"includes":["src/**"],"rules":{"architecture/mapper-return-type":{"options":{"pluralForms":{"Mouse":"Mice"}}}}}])).unwrap();
    let settings = options(
        &rules,
        &overrides,
        "architecture/mapper-return-type",
        "src/x.rs",
    );
    assert_eq!(plural("Person", &settings), "People");
    assert_eq!(plural("Mouse", &settings), "Mice");
    assert_eq!(plural("Category", &settings), "Categories");
    assert_eq!(plural("Box", &settings), "Boxes");
    assert_eq!(plural("Dto", &settings), "Dtos");
    assert_eq!(snake("HTTPViewDto"), "http_view_dto");
    let settings = options(
        &rules,
        &overrides,
        "architecture/companion-files",
        "src/x.rs",
    );
    assert_eq!(
        missing_companions(root.path(), "src/x.rs", &settings)
            .unwrap()
            .len(),
        1
    );
    std::fs::write(root.path().join("src/types/X.md"), "").unwrap();
    assert_eq!(
        missing_companions(root.path(), "src/x.rs", &settings)
            .unwrap()
            .len(),
        1
    );
    std::fs::write(root.path().join("src/types/y.md"), "").unwrap();
    assert!(
        missing_companions(root.path(), "src/y.rs", &settings)
            .unwrap()
            .is_empty()
    );
    assert!(
        missing_companions(root.path(), "other/x.rs", &settings)
            .unwrap()
            .is_empty()
    );
    let settings: Options = serde_json::from_value(
        json!({"pairs":{"escape":{"match":"src/**","require":["../../outside"]}}}),
    )
    .unwrap();
    assert!(missing_companions(root.path(), "src/a.rs", &settings).is_err());
    let settings: Options = serde_json::from_value(
        json!({"pairs":{"parent":{"match":"src/**","require":[".././src/types/y.md"]}}}),
    )
    .unwrap();
    assert!(
        missing_companions(root.path(), "src/a.rs", &settings)
            .unwrap()
            .is_empty()
    );
    for (pattern, path) in [
        ("src/**/{name}.rs", "src/a/b/x.rs"),
        ("src/**/{name}.rs", "src/x.rs"),
        ("*/a?.rs", "src/ab.rs"),
        ("**", "src/a.rs"),
    ] {
        assert!(captures(pattern, path).is_some());
    }
    for (pattern, path) in [
        ("src/*/x.rs", "src/a/b/x.rs"),
        ("src/{x}.rs", "src/.rs"),
        ("a?", "a"),
        ("a", "ab"),
    ] {
        assert!(captures(pattern, path).is_none());
    }
    let off = serde_json::from_value(json!({"architecture/companion-files":"off"})).unwrap();
    assert!(
        options(&off, &[], "architecture/companion-files", "src/a.rs")
            .pairs
            .is_empty()
    );
}

#[test]
fn collection_names_preserve_acronyms() {
    let options = Options::default();
    assert!(
        godot_llm_lint::features::configuration::helpers::contracts::matches_output(
            "map_user_dtos",
            "UserDTO",
            true,
            &options
        )
    );
    assert!(
        godot_llm_lint::features::configuration::helpers::contracts::matches_output(
            "map_input_to_http_responses",
            "HTTPResponse",
            true,
            &options
        )
    );
    assert!(
        !godot_llm_lint::features::configuration::helpers::contracts::matches_output(
            "map_user_dt_os",
            "UserDTO",
            true,
            &options
        )
    );
}
