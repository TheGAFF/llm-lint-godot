#![cfg(test)]
mod support;
use godot_llm_lint::features::architecture::helpers::analyze_project;
use support::{project, write};

#[test]
fn engine_resource_subclasses_and_project_descendants_keep_their_roles() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for (name, base) in [
        ("effect", "RichTextEffect"),
        ("style", "StyleBox"),
        ("texture", "Texture2D"),
        ("material", "ShaderMaterial"),
    ] {
        write(
            root.path(),
            &format!("src/features/ui/resources/{name}_resource.gd"),
            &format!("extends {base}\n"),
        );
    }
    write(
        root.path(),
        "src/features/ui/resources/base_resource.gd",
        "class_name BaseResource\nextends RichTextEffect\n",
    );
    write(
        root.path(),
        "src/features/ui/resources/child_resource.gd",
        "extends BaseResource\n",
    );
    assert!(
        !analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/role-base")
    );
}

#[test]
fn engine_resource_subclasses_are_rejected_in_node_and_model_roles() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/ui/nodes/effect_node.gd",
        "extends RichTextEffect\n",
    );
    write(
        root.path(),
        "src/features/ui/models/style_model.gd",
        "extends StyleBoxFlat\n",
    );
    write(
        root.path(),
        "src/features/ui/resources/wrong_resource.gd",
        "extends Control\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts.get("architecture/role-base"), Some(&3));
}
