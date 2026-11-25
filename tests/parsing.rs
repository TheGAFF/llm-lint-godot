#![cfg(test)]
use godot_llm_lint::features::parsing::parsers::{parse_script, parse_text};

#[test]
fn modern_syntax_callbacks_inner_classes_and_utf8_are_parsed() {
    let source = parse_script("src/features/run/models/run_model.gd", "@tool\nclass_name RunModel\nextends RefCounted\nvar values: Dictionary[String, int] = {}\nsignal finished(value: int)\nfunc _ready() -> void:\n\tvar callback := func(value: int) -> int: return value + 1\n\tcallback.call(2)\nclass Entry:\n\tvar title: String = 'CafÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©'\n".into()).unwrap();
    assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
    assert_eq!(source.class_name.as_deref(), Some("RunModel"));
    assert!(source.instance_state);
    assert_eq!(source.functions.len(), 2);
    assert!(source.code_lines > 8);
}

#[test]
fn static_collections_and_loader_calls_are_resolved() {
    let source = parse_script("src/shared/queries/catalog_query.gd", "extends RefCounted\nconst PATHS = [\"res://one_resource.tres\", \"res://two_resource.tres\"]\nstatic func load_all() -> void:\n\tfor path: String in PATHS:\n\t\tload(path)\n\tResourceLoader.load(\"user://tests/one_resource.tres\")\n\tvar other = ResourceLoader.load(path)\n".into()).unwrap();
    assert!(source.diagnostics.is_empty());
    assert!(
        source
            .references
            .iter()
            .any(|reference| reference.target == "res://one_resource.tres" && reference.required)
    );
    assert!(source.references.iter().any(|reference| reference.target
        == "user://tests/one_resource.tres"
        && reference.dynamic));
    assert!(
        source
            .references
            .iter()
            .any(|reference| reference.target == "path" && reference.dynamic)
    );
}

#[test]
fn loader_comments_do_not_replace_the_first_argument() {
    let source = parse_script(
        "src/shared/catalog/catalog_query.gd",
        r#"extends RefCounted
const CATALOG = preload(
	# reason: Exercise a style directive before the preload literal.
	# gdstyle:ignore=format/max-line-length
	"res://catalog_resource.tres"
)

static func load_catalog(path: String) -> Resource:
	ResourceLoader.load(
		# res://comment_is_not_a_dependency.tres
		"uid://catalog",
		"Resource"
	)
	return load(
		# An actual computed path still requires a declaration.
		path
	)
"#
        .into(),
    )
    .unwrap();
    assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
    let references: Vec<_> = source
        .references
        .iter()
        .filter(|reference| reference.required)
        .map(|reference| (reference.target.as_str(), reference.dynamic))
        .collect();
    assert_eq!(
        references,
        [
            ("res://catalog_resource.tres", false),
            ("uid://catalog", false),
            ("path", true),
        ],
    );
}

#[test]
fn syntax_errors_and_literal_token_boundaries_are_visible() {
    let invalid = parse_script("bad.gd", "func bad(:\n\treturn ???\n".into()).unwrap();
    assert!(!invalid.diagnostics.is_empty());
    let source = parse_script(
        "a.gd",
        "static func text() -> String:\n\t# Comment\n\treturn \"\"\"one\ntwo\"\"\"\n".into(),
    )
    .unwrap();
    let tokens = &source.functions[0].tokens;
    assert!(!tokens.iter().any(|token| token.text.contains("Comment")));
    assert!(
        tokens
            .iter()
            .any(|token| token.text.contains("two") && token.end_line > token.line)
    );
}

#[test]
fn godot_metadata_and_relative_plugin_registration_are_references() {
    let scene = parse_text("src/app/scenes/main_scene.tscn", "[gd_scene format=3]\n[ext_resource type=\"Script\" path=\"res://src/app/nodes/main_node.gd\" id=\"main\"]\n".into());
    assert_eq!(scene.references.len(), 1);
    let plugin = parse_text(
        "addons/editor/plugin.cfg",
        "[plugin]\nscript=\"plugin.gd\"\n".into(),
    );
    assert_eq!(plugin.references[0].target, "res://addons/editor/plugin.gd");
}

#[test]
fn config_file_loads_are_not_resource_loader_calls() {
    let source = parse_script("src/features/settings/services/settings_service.gd",
        "extends RefCounted\nfunc read_settings() -> void:\n\tvar config: ConfigFile = ConfigFile.new()\n\tconfig.load('user://settings.cfg')\n\tResourceLoader.load('user://world_resource.tres')\n".into()).unwrap();
    assert!(
        !source
            .references
            .iter()
            .any(|reference| reference.target == "user://settings.cfg")
    );
    assert!(
        source
            .references
            .iter()
            .any(|reference| reference.target == "user://world_resource.tres" && reference.dynamic)
    );
}

#[test]
fn simple_resource_loader_aliases_retain_dynamic_dependency_analysis() {
    let source = parse_script("src/features/content/services/content_service.gd",
        "extends RefCounted\nconst LOADER = ResourceLoader\nfunc load_world(path: String) -> void:\n\tvar loader = ResourceLoader\n\tLOADER.load(path)\n\tloader.load(path)\n".into()).unwrap();
    assert_eq!(
        source
            .references
            .iter()
            .filter(|reference| reference.dynamic)
            .count(),
        2
    );
}
