#![cfg(test)]
mod support;
use godot_llm_lint::features::{
    architecture::helpers::analyze_project, configuration::helpers::discover_project,
};
use support::{project, write};

#[test]
fn all_file_roles_accept_nested_assets_without_reading_binary_bytes() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for path in [
        "src/features/content/resources/levels/addition/addition_01_level_resource.tres",
        "src/app/scenes/menu/main_scene.tscn",
        "src/features/ui/assets/music/worlds/addition_music.ogg",
        "src/features/ui/assets/sound_effects/boss_hit_sound_effect.wav",
        "src/features/content/assets/icons/relics/feather_icon.svg",
        "src/features/ui/assets/textures/backgrounds/canyon_texture.png",
        "src/features/ui/assets/fonts/barlow_font.ttf",
        "src/features/run/testing/fixtures/pre_migration_fixture.json",
    ] {
        write(root.path(), path, "");
        if !path.ends_with(".tres") && !path.ends_with(".tscn") {
            std::fs::write(root.path().join(path), [0xff, 0, 0xfe]).unwrap();
        }
    }
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.errors, 0, "{:?}", report.diagnostics);
    assert_eq!(report.scanned_files, 9);
}

#[test]
fn every_file_role_rejects_a_missing_suffix() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for path in [
        "src/features/content/resources/level.tres",
        "src/app/scenes/main.tscn",
        "src/features/ui/assets/music/addition.ogg",
        "src/features/ui/assets/sound_effects/hit.ogg",
        "src/features/ui/assets/icons/play.svg",
        "src/features/ui/assets/textures/canyon.png",
        "src/features/ui/assets/fonts/barlow.ttf",
        "src/features/ui/testing/fixtures/saves.json",
    ] {
        write(root.path(), path, "");
    }
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/role-filename"], 8);
}

#[test]
fn attached_script_metadata_and_prospective_collisions_are_reported() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/content/resources/level_definition_resource.gd",
        "class_name LevelDefinitionResource\nextends Resource\n",
    );
    for name in ["01", "01_resource"] {
        write(
            root.path(),
            &format!("src/features/content/resources/levels/addition/{name}.tres"),
            "[gd_resource type=\"Resource\" format=3]\n[ext_resource type=\"Script\" path=\"res://src/features/content/resources/level_definition_resource.gd\" id=\"script\"]\n[resource]\nscript = ExtResource(\"script\")\n",
        );
    }
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/asset-collision"], 2);
    assert!(
        report
            .diagnostics
            .iter()
            .filter(|item| item.rule == "architecture/role-filename")
            .all(|item| item.expected.ends_with("addition_01_level_resource.tres"))
    );
}

#[test]
fn numeric_resources_have_metadata_based_suggestions_and_collisions() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for filename in [
        "01.tres",
        "01_resource.tres",
        "addition_01_level_resource.tres",
    ] {
        write(
            root.path(),
            &format!("src/features/content/resources/levels/addition/{filename}"),
            "[gd_resource type=\"Resource\" script_class=\"LevelDefinitionResource\" format=3]\n",
        );
    }
    let path = "src/features/content/resources/levels/addition/01.tres";
    let report = analyze_project(&project, &[path.into()]).unwrap();
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|item| item.rule == "architecture/role-filename" && item.path == path)
        .unwrap();
    assert_eq!(
        diagnostic.expected,
        "src/features/content/resources/levels/addition/addition_01_level_resource.tres"
    );
    assert_eq!(report.counts["architecture/role-filename"], 2);
    assert_eq!(report.counts["architecture/asset-collision"], 2);
}

#[test]
fn unreferenced_media_roles_placement_ambiguity_and_ownership_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for path in [
        "src/features/ui/assets/mystery.png",
        "src/features/ui/assets/mystery.ogg",
        "src/features/ui/assets/audio/sfx/boss_hit.ogg",
        "assets/orphan_icon.svg",
        "src/features/ui/assets/textures/01_texture.png",
        "src/features/ui/assets/fonts/Barlow-Bold.ttf",
        "src/features/ui/assets/icons/portrait_texture.svg",
        "src/features/ui/assets/unknown.bin",
    ] {
        write(root.path(), path, "");
    }
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/asset-kind"], 4);
    assert_eq!(report.counts["architecture/feature-layout"], 1);
    assert!(
        report.diagnostics.iter().any(|item| item.expected
            == "src/features/ui/assets/sound_effects/boss_hit_sound_effect.ogg")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|item| item.expected.ends_with("barlow_bold_font.ttf"))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|item| item.rule == "architecture/role-filename"
                && item.path.ends_with("01_texture.png"))
    );
}

#[test]
fn explicit_classifications_exclusions_and_levels_work_for_asset_targets() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "docs/reference.png", "");
    write(
        root.path(),
        "src/features/ui/assets/fonts/OFL.txt",
        "license",
    );
    write(
        root.path(),
        "src/features/ui/assets/icons/play_icon.svg.import",
        "uid=\"uid://play\"\n",
    );
    write(
        root.path(),
        "src/features/ui/assets/legacy/play.svg.import",
        "uid=\"uid://legacy\"\n",
    );
    write(
        root.path(),
        "src/features/ui/assets/legacy/LICENSE.txt",
        "upstream license",
    );
    write(
        root.path(),
        "tools/alien_sources/boss.tscn.in",
        "authoring input",
    );
    let project = project(
        root.path(),
        "[[excluded]]\npath='docs'\nreason='Documentation references'\n[[asset_classifications]]\nincludes=['src/features/ui/assets/legacy/**']\nrole='icon'\n[[overrides]]\nincludes=['src/features/ui/assets/**']\n[overrides.rules]\n'architecture/role-placement'='warn'\n'architecture/role-filename'='off'\n",
    );
    let path = "src/features/ui/assets/legacy/play.svg";
    write(root.path(), path, "");
    let report = analyze_project(&project, &[path.into()]).unwrap();
    assert_eq!(report.errors, 0);
    assert_eq!(report.warnings, 1);
    assert_eq!(
        report.diagnostics[0].expected,
        "src/features/ui/assets/icons/legacy/play_icon.svg"
    );
    assert_eq!(report.diagnostics[0].severity, "warning");
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(
        encoded["diagnostics"][0]["expected"],
        report.diagnostics[0].expected
    );
    assert!(report.diagnostics.iter().all(|item| item.path == path));
}

#[test]
fn conflicting_or_extension_incompatible_classifications_do_not_guess() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[[asset_classifications]]\nincludes=['src/features/ui/assets/**']\nrole='icon'\n[[asset_classifications]]\nincludes=['src/features/ui/assets/**']\nrole='texture'\n",
    );
    write(
        root.path(),
        "src/features/ui/assets/icons/play_icon.svg",
        "",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/asset-kind"], 1);
    write(
        root.path(),
        "llm-lint.godot.toml",
        "version=1\n[[asset_classifications]]\nincludes=['src/**']\nrole='unknown'\n",
    );
    assert!(discover_project(root.path(), None).is_err());
}

#[test]
fn asset_diagnostics_are_printed_by_the_cli_with_failure_status() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    let path = "src/features/ui/assets/music/music_addition_normal.ogg";
    write(root.path(), path, "");
    for json in [false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"));
        command
            .current_dir(root.path())
            .args(["architecture", path]);
        if json {
            command.arg("--json");
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let text = if json {
            String::from_utf8(output.stdout).unwrap()
        } else {
            String::from_utf8(output.stderr).unwrap()
        };
        assert!(
            text.contains("src/features/ui/assets/music/addition_normal_music.ogg"),
            "{text}"
        );
    }
}
