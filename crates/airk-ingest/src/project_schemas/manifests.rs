// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn recognized(path: &str, document: &Value) -> bool {
    let basename = path.rsplit('/').next().unwrap_or(path);
    entries(document).iter().any(|(key, _)| {
        key.scalar().is_some_and(|name| match basename {
            "package.json" => matches!(
                name,
                "name" | "dependencies" | "devDependencies" | "scripts" | "workspaces"
            ),
            "composer.json" => matches!(name, "name" | "require" | "require-dev" | "autoload"),
            "Cargo.toml" => {
                matches!(name, "package" | "workspace" | "dependencies")
                    || name.starts_with("dependencies.")
            }
            "pyproject.toml" => {
                matches!(name, "project" | "build-system" | "tool") || name.starts_with("tool.")
            }
            name if name.starts_with("tsconfig") => matches!(
                key.scalar(),
                Some(
                    "compilerOptions" | "extends" | "references" | "files" | "include" | "exclude"
                )
            ),
            _ => false,
        })
    })
}
pub(super) fn extract(facts: &mut Facts<'_>, path: &str, document: &Value) -> Result<bool> {
    if document.mapping().is_none() {
        return issue(facts, document, "Project manifest requires a mapping");
    }
    if !recognized(path, document) {
        return issue(facts, document, "Manifest lacks recognized schema evidence");
    }
    let basename = path.rsplit('/').next().unwrap_or(path);
    let mut complete = true;
    for (key, value) in entries(document) {
        let Some(name) = key.scalar() else {
            complete &= issue(facts, key, "Manifest key must be scalar")?;
            continue;
        };
        let kind = match basename {
            "package.json" => match name {
                "name" | "version" => Some("package_identity"),
                "dependencies"
                | "devDependencies"
                | "peerDependencies"
                | "optionalDependencies" => Some("package_dependency"),
                "scripts" => Some("package_script"),
                "workspaces" => Some("workspace_member"),
                _ => None,
            },
            "composer.json" => match name {
                "name" | "version" => Some("package_identity"),
                "require" | "require-dev" | "conflict" | "replace" | "provide" | "suggest" => {
                    Some("package_dependency")
                }
                "scripts" => Some("package_script"),
                _ => None,
            },
            "Cargo.toml" => {
                if name == "package" {
                    Some("package_identity")
                } else if name.starts_with("workspace") {
                    Some("workspace_configuration")
                } else if name.contains("dependencies") {
                    Some("package_dependency")
                } else {
                    None
                }
            }
            "pyproject.toml" => {
                if name == "project" || name.starts_with("project.") {
                    Some("project_configuration")
                } else if name == "build-system" {
                    Some("build_system")
                } else if name == "tool" || name.starts_with("tool.") {
                    Some("tool_configuration")
                } else {
                    None
                }
            }
            name if name.starts_with("tsconfig") && name.ends_with(".json") => match key.scalar() {
                Some("compilerOptions") => Some("compiler_option"),
                Some("extends") => Some("configuration_extends"),
                Some("references") => Some("project_reference"),
                Some("files" | "include" | "exclude") => Some("configuration_path"),
                _ => None,
            },
            _ => return issue(facts, document, "Unsupported manifest filename"),
        };
        if let Some(kind) = kind {
            complete &= emit(facts, key, kind)?;
            complete &= written(facts, value, kind)?;
        }
    }
    Ok(complete)
}
