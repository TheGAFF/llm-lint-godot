#![cfg(test)]
mod support;

use godot_llm_lint::{
    features::{architecture::helpers::analyze_project, configuration::helpers::discover_project},
    shared::models::Diagnostic,
};
use support::{project, write};

const OWNER: &str = "src/features/ui";
const ENABLED: &str = "[bundles]\nowners = ['src/features/ui']\n";
const PLACEMENT: &str = "architecture/bundle-placement";
const COMPANION: &str = "architecture/bundle-companion";
const SIZE: &str = "architecture/bundle-size";
const CONFIGURATION: &str = "architecture/bundle-configuration";

fn findings<'a>(diagnostics: &'a [Diagnostic], rule: &str) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|item| item.rule == rule)
        .collect()
}

#[test]
fn mixed_scripts_and_assets_keep_their_role_contracts() {
    let root = tempfile::tempdir().unwrap();
    let base = "src/features/ui/enemies/honey_tank";
    write(
        root.path(),
        &format!("{base}/honey_tank_scene.tscn"),
        "[gd_scene format=3]\n",
    );
    write(
        root.path(),
        &format!("{base}/honey_tank_icon.svg"),
        "<svg/>",
    );
    write(root.path(), &format!("{base}/honey_tank_icon.png"), "");
    write(root.path(), &format!("{base}/honey_tank_texture.png"), "");
    write(
        root.path(),
        &format!("{base}/honey_tank_resource.tres"),
        "[gd_resource type=\"Resource\" format=3]\n",
    );
    write(
        root.path(),
        &format!("{base}/testing/test_honey_tank.gd"),
        "extends GutTest\n",
    );
    write(root.path(), &format!("{base}/localization/pt_BR.po"), "");
    write(root.path(), &format!("{base}/localization/zh_Hans.po"), "");
    write(
        root.path(),
        &format!("{base}/helper_bucket/poses_helpers.gd"),
        "extends RefCounted\nstatic func poses() -> int:\n\treturn 1\n",
    );
    write(
        root.path(),
        &format!("{base}/honey_tank_model.gd"),
        "extends Node\n",
    );
    write(
        root.path(),
        &format!("{base}/honey_tank_pose_mutator.gd"),
        "extends RefCounted\nstatic func calculate_pose() -> int:\n\treturn 1\n",
    );
    write(
        root.path(),
        &format!("{base}/honey_tank_actor_node.gd"),
        "class_name HoneyTankActorNode\nextends Node\nvar _secret_pose: int\n",
    );
    write(
        root.path(),
        &format!("{base}/borrower_node.gd"),
        "extends Node\nfunc _refresh(actor: HoneyTankActorNode) -> void:\n\tprint(actor._secret_pose)\n",
    );
    let project = project(root.path(), ENABLED);
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/function-role"], 1);
    assert_eq!(report.counts["architecture/private-member-access"], 1);
    assert_eq!(report.counts["architecture/role-base"], 1);
    for rule in [
        PLACEMENT,
        COMPANION,
        "architecture/role-filename",
        "architecture/asset-kind",
        "architecture/file-name",
    ] {
        assert!(
            findings(&report.diagnostics, rule).is_empty(),
            "{:?}",
            report.diagnostics
        );
    }
}

#[test]
fn recognizable_test_scripts_need_the_local_testing_folder() {
    let root = tempfile::tempdir().unwrap();
    let path = format!("{OWNER}/enemies/honey_tank/honey_tank_test_harness.gd");
    write(root.path(), &path, "extends Node\n");
    let project = project(root.path(), ENABLED);
    let report = analyze_project(&project, &[]).unwrap();
    let item = findings(&report.diagnostics, PLACEMENT)[0];
    assert_eq!(item.path, path);
    assert!(
        item.expected
            .ends_with("/testing/honey_tank_test_harness.gd")
    );
}

#[test]
fn only_enabled_owners_switch_and_their_top_level_role_buckets_fail() {
    let root = tempfile::tempdir().unwrap();
    let legacy = "src/shared/helpers/string_helpers.gd";
    let wrong = "src/features/ui/nodes/ember_node.gd";
    let arbitrary = "src/features/ui/ember/ember_behavior.gd";
    write(root.path(), legacy, "extends RefCounted\n");
    write(root.path(), wrong, "extends Node\n");
    write(root.path(), arbitrary, "extends Node\n");
    let project = project(root.path(), ENABLED);
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(findings(&report.diagnostics, PLACEMENT)[0].path, wrong);
    assert_eq!(
        findings(&report.diagnostics, "architecture/role-filename")[0].path,
        arbitrary
    );
    assert!(report.diagnostics.iter().all(|item| item.path != legacy));
}

#[test]
fn exact_cap_counts_godot_assets_and_ignores_sidecars_and_nested_subjects() {
    let root = tempfile::tempdir().unwrap();
    let subject = format!("{OWNER}/enemies/honey_tank");
    let nested = format!("{subject}/testing/test_honey_tank.gd");
    for index in 0..28 {
        write(root.path(), &format!("{subject}/part_{index}_icon.svg"), "");
        write(
            root.path(),
            &format!("{subject}/part_{index}_icon.svg.import"),
            "",
        );
    }
    write(
        root.path(),
        &format!("{subject}/body_node.gd"),
        "extends Node\n",
    );
    write(
        root.path(),
        &format!("{subject}/body_node.gd.uid"),
        "uid://body\n",
    );
    write(
        root.path(),
        &format!("{subject}/body.gdshader"),
        "shader_type canvas_item;",
    );
    write(root.path(), &nested, "extends GutTest\n");
    write(
        root.path(),
        &format!("{subject}/testing/seed_fixture.json"),
        "{}",
    );
    let project = project(root.path(), ENABLED);
    assert!(findings(&analyze_project(&project, &[]).unwrap().diagnostics, SIZE).is_empty());
    write(root.path(), &format!("{subject}/part_0_icon.png"), "");
    let full = analyze_project(&project, &[]).unwrap();
    let targeted = analyze_project(&project, &[nested]).unwrap();
    for report in [full, targeted] {
        let size = findings(&report.diagnostics, SIZE);
        assert_eq!(size.len(), 1);
        assert_eq!(size[0].path, subject);
        assert_eq!(size[0].severity, "error");
        assert!(size[0].actual.contains("31 directly"));
    }
}

#[test]
fn inferred_exclusive_companions_honor_class_uses_uid_assets_and_attachment() {
    let root = tempfile::tempdir().unwrap();
    let scene = format!("{OWNER}/enemies/hop/hop_scene.tscn");
    let actor = format!("{OWNER}/actors/hop_node.gd");
    let helper = format!("{OWNER}/motion/hop_pose_helpers.gd");
    let shared = format!("{OWNER}/drawing/common_texture.svg");
    let child = format!("{OWNER}/widgets/icon_scene.tscn");
    let asset = format!("{OWNER}/drawing/hop_icon.svg");
    let other = format!("{OWNER}/enemies/crawl/crawl_scene.tscn");
    let actor_text =
        "class_name HopNode\nextends Node\nfunc _show() -> void:\n\tprint(HopPoseHelpers.pose())\n";
    write(root.path(), &actor, actor_text);
    write(
        root.path(),
        &helper,
        "class_name HopPoseHelpers\nextends RefCounted\nstatic func pose() -> int:\n\treturn 2\n",
    );
    write(root.path(), &child, "[gd_scene format=3]\n");
    write(root.path(), &shared, "<svg/>");
    write(root.path(), &asset, "<svg/>");
    write(
        root.path(),
        &format!("{asset}.import"),
        "[remap]\nuid=\"uid://hopasset\"\n",
    );
    let external = format!("[ext_resource type=\"Texture2D\" path=\"res://{shared}\" id=\"2\"]\n");
    write(
        root.path(),
        &scene,
        &format!(
            "[gd_scene format=3]\n[ext_resource type=\"Script\" path=\"res://{actor}\" id=\"1\"]\n{external}[ext_resource type=\"Texture2D\" uid=\"uid://hopasset\" id=\"3\"]\n[ext_resource type=\"PackedScene\" path=\"res://{child}\" id=\"4\"]\n[node name=\"Hop\" type=\"Node\"]\nscript = ExtResource(\"1\")\n"
        ),
    );
    write(
        root.path(),
        &other,
        &format!(
            "[gd_scene format=3]\n{external}[node name=\"Crawl\" type=\"Sprite2D\"]\ntexture = ExtResource(\"2\")\n"
        ),
    );
    let project = project(root.path(), ENABLED);
    let expected = [&actor, &asset, &helper];
    for target in [&[][..], &[scene.clone()][..]] {
        let report = analyze_project(&project, target).unwrap();
        let items = findings(&report.diagnostics, COMPANION);
        assert_eq!(
            items.iter().map(|item| &item.path).collect::<Vec<_>>(),
            expected
        );
        assert!(items[0].actual.contains("attachment"));
        assert!(items[1].actual.contains("asset reference"));
        assert!(items[2].actual.contains("class use"));
    }
}

#[test]
fn a_script_only_bundle_can_anchor_class_companions_without_claiming_unrelated_apis() {
    let root = tempfile::tempdir().unwrap();
    let panel = format!("{OWNER}/screens/forge/forge_node.gd");
    let helper = format!("{OWNER}/effects/forge_helpers.gd");
    let icon = format!("{OWNER}/screens/forge/forge_icon.svg");
    write(
        root.path(),
        &panel,
        "extends Node\nfunc _ready() -> void:\n\tprint(ForgeHelpers.sparkle())\n",
    );
    write(
        root.path(),
        &helper,
        "class_name ForgeHelpers\nextends RefCounted\nstatic func sparkle() -> bool:\n\treturn true\n",
    );
    write(root.path(), &icon, "<svg/>");
    let unclaimed = project(root.path(), ENABLED);
    assert!(
        findings(
            &analyze_project(&unclaimed, &[]).unwrap().diagnostics,
            COMPANION
        )
        .is_empty()
    );
    let configured = project(
        root.path(),
        &format!("{ENABLED}[[bundles.companions]]\nanchor = '{panel}'\npaths = ['{icon}']\n"),
    );
    let report = analyze_project(&configured, &[panel]).unwrap();
    let findings = findings(&report.diagnostics, COMPANION);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].path, helper);
}

#[test]
fn declarations_cover_indirect_assets_and_exact_exclusions_do_not_hide_invalid_roles() {
    let root = tempfile::tempdir().unwrap();
    let anchor = format!("{OWNER}/enemies/hop/hop_scene.tscn");
    let companion = format!("{OWNER}/nodes/alternate_node.gd");
    let wrong = format!("{OWNER}/nodes/alternate_hopping.gd");
    write(root.path(), &anchor, "[gd_scene format=3]\n");
    write(
        root.path(),
        &companion,
        "# llm-lint-ignore architecture/bundle-companion: A plugin consumes the rig from the shared path.\nextends Node\n",
    );
    write(root.path(), &wrong, "extends Node\n");
    let configured = project(
        root.path(),
        &format!(
            "{ENABLED}[[bundles.companions]]\nanchor = '{anchor}'\npaths = ['{companion}']\n[[bundles.exclusions]]\npath = '{OWNER}/nodes'\nrule = '{PLACEMENT}'\nreason = 'The plugin discovers this fixed directory at startup.'\n"
        ),
    );
    let report = analyze_project(&configured, &[]).unwrap();
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(report.diagnostics[0].rule, "architecture/role-filename");
    assert_eq!(report.diagnostics[0].path, wrong);
    std::fs::remove_file(root.path().join(&companion)).unwrap();
    let report = analyze_project(&configured, &[anchor]).unwrap();
    let stale = findings(&report.diagnostics, CONFIGURATION);
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].item, companion);
}

#[test]
fn cap_exclusions_do_not_extend_to_child_directories_and_stale_exclusions_fail() {
    let root = tempfile::tempdir().unwrap();
    let base = format!("{OWNER}/enemies/hop");
    let child = format!("{base}/warning");
    for directory in [&base, &child] {
        for leaf in ["head", "body"] {
            write(root.path(), &format!("{directory}/{leaf}_icon.svg"), "");
        }
    }
    let project = project(
        root.path(),
        &format!(
            "{ENABLED}max_files = 1\n[[bundles.exclusions]]\npath = '{base}'\nrule = '{SIZE}'\nreason = 'External pipeline reads this directory as one composite sheet.'\n"
        ),
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(findings(&report.diagnostics, SIZE)[0].path, child);
    assert_eq!(report.diagnostics.len(), 1);
    std::fs::remove_file(root.path().join(format!("{base}/head_icon.svg"))).unwrap();
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(findings(&report.diagnostics, CONFIGURATION)[0].path, base);
}

#[test]
fn rejects_conflicting_or_global_opt_out_configuration() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(OWNER)).unwrap();
    write(root.path(), "project.godot", "config_version=5\n");
    let actor = format!("{OWNER}/enemies/hop/hop_node.gd");
    let first = format!("{OWNER}/enemies/hop/hop_scene.tscn");
    let second = format!("{OWNER}/enemies/crawl/crawl_scene.tscn");
    for config in [
        format!(
            "{ENABLED}[[bundles.companions]]\nanchor = '{first}'\npaths = ['{actor}']\n[[bundles.companions]]\nanchor = '{second}'\npaths = ['{actor}']\n"
        ),
        format!(
            "{ENABLED}[[bundles.exclusions]]\npath = '{OWNER}/**'\nrule = '{PLACEMENT}'\nreason = 'A generic exception.'\n"
        ),
        format!(
            "{ENABLED}[[bundles.exclusions]]\npath = '{OWNER}'\nrule = '{PLACEMENT}'\nreason = ''\n"
        ),
        format!("{ENABLED}[rules]\n'{SIZE}' = 'warn'\n"),
    ] {
        write(root.path(), "llm-lint.godot.toml", &config);
        assert!(discover_project(root.path(), None).is_err(), "{config}");
    }
}
