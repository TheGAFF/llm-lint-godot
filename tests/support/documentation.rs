use std::{fs, path::Path};
pub fn render_rules(root: &Path) -> String {
    let catalog: Vec<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(root.join("policy/rules.json")).unwrap()).unwrap();
    let mut output = String::from(
        "# Rule reference\n\nGenerated from the rule catalog and executable examples. After changing either source, regenerate this file with `cargo test --locked --test rule_examples regenerate_rule_documentation -- --ignored`. Use `godot-llm-lint rules --json` to inspect rule IDs.\n\nCustom and individually selected native rules have passing/failing fixtures. Rules inherited from pinned upstream groups retain upstream coverage and local group integration tests.\n",
    );
    for rule in catalog {
        let id = rule["id"].as_str().unwrap();
        output.push_str(&format!(
            "\n## {id}\n\n{}\n\nDefault: **{}**. {}\n",
            rule["description"].as_str().unwrap(),
            rule["default"].as_str().unwrap(),
            if rule["configurable"] == false {
                "Input integrity: always an error."
            } else {
                "Levels: off, warn, error."
            }
        ));
        let options = rule["options"].as_array().unwrap();
        if !options.is_empty() {
            output.push_str(&format!(
                "\nOptions: {}.\n",
                options
                    .iter()
                    .map(|option| format!("`{}`", option.as_str().unwrap()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let name = format!("{}.json", id.replace('/', "--"));
        let filename = root.join("examples/rules").join(&name);
        if !filename.exists() {
            assert!(
                !["custom", "native", "formatting"].contains(&rule["source"].as_str().unwrap()),
                "Missing examples: {id}"
            );
            if let Some(upstream) = rule["upstream"].as_str() {
                output.push_str(&format!("\n[Upstream rule reference]({upstream}).\n"));
            }
            continue;
        }
        let example: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(filename).unwrap()).unwrap();
        output.push_str(&format!(
            "\n[Complete executable fixture](../examples/rules/{name}).\n"
        ));
        append_examples(&mut output, &example);
    }
    output
}

fn append_examples(output: &mut String, example: &serde_json::Value) {
    for label in ["passing", "failing"] {
        let files = example[label]["files"].as_object().unwrap();
        let target = example["expected"]["path"]
            .as_str()
            .filter(|file| files.contains_key(*file));
        let filename = target
            .or_else(|| {
                files
                    .keys()
                    .find(|file| {
                        ["rs", "gd", "tscn", "tres"]
                            .iter()
                            .any(|ext| file.ends_with(&format!(".{ext}")))
                    })
                    .map(String::as_str)
            })
            .unwrap_or_else(|| files.keys().next().unwrap());
        let language = match filename.rsplit('.').next().unwrap() {
            "rs" => "rust",
            "gd" => "gdscript",
            _ => "text",
        };
        output.push_str(&format!(
            "\n### {}\n\n`{filename}`\n\n```{language}\n{}\n```\n",
            if label == "passing" {
                "Passing"
            } else {
                "Failing"
            },
            files[filename].as_str().unwrap().trim_end()
        ));
    }
}
