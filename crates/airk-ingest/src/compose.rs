// SPDX-License-Identifier: Apache-2.0
//! Written Compose configuration; no interpolation, include loading or runtime claims.
use crate::{
    build_facts::Facts,
    structured_yaml::{self, Value, emit},
};
use anyhow::Result;

pub fn analyze(path: &str, text: &str) -> Result<crate::syntax::Analysis> {
    let view = structured_yaml::parse(text)?;
    let mut facts = Facts::new(path, text, "compose")?;
    view.diagnostics(&mut facts)?;
    let mut complete = view.issues.is_empty();
    for document in &view.documents {
        let Some(entries) = document.mapping() else {
            complete &= wrong(&mut facts, document, "Compose mapping")?;
            continue;
        };
        for (key, value) in entries {
            match key.scalar() {
                Some("services") => {
                    let Some(services) = value.mapping() else {
                        complete &= wrong(&mut facts, value, "services mapping")?;
                        continue;
                    };
                    for (name, body) in services {
                        complete &= emit(&mut facts, name, "compose_service")?;
                        complete &= service(&mut facts, body)?;
                    }
                }
                Some("networks" | "volumes" | "configs" | "secrets") => {
                    let Some(entries) = value.mapping() else {
                        complete &= wrong(&mut facts, value, "resource mapping")?;
                        continue;
                    };
                    let kind = format!("compose_{}", key.scalar().unwrap().trim_end_matches('s'));
                    for (name, body) in entries {
                        complete &= emit(&mut facts, name, &kind)?;
                        complete &= written(&mut facts, body, "compose_config")?;
                    }
                }
                Some("include" | "extends") => {
                    complete &= written(&mut facts, value, "compose_written_reference")?;
                    complete &= wrong(
                        &mut facts,
                        value,
                        "locally self-contained configuration; external configuration is not loaded",
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(facts.finish(!complete))
}

fn service(facts: &mut Facts<'_>, value: &Value) -> Result<bool> {
    let Some(entries) = value.mapping() else {
        return wrong(facts, value, "service mapping");
    };
    let mut complete = true;
    for (key, value) in entries {
        match key.scalar() {
            Some("image") => complete &= emit(facts, value, "compose_image")?,
            Some("build") => {
                if let Some(entries) = value.mapping() {
                    for (key, value) in entries {
                        complete &= written(
                            facts,
                            value,
                            if key.scalar() == Some("context") {
                                "build_context"
                            } else {
                                "build_config"
                            },
                        )?;
                    }
                } else {
                    complete &= emit(facts, value, "build_context")?;
                }
            }
            Some("depends_on") => {
                if let Some(entries) = value.mapping() {
                    for (key, value) in entries {
                        complete &= emit(facts, key, "compose_dependency")?;
                        complete &= written(facts, value, "dependency_config")?;
                    }
                } else {
                    complete &= written(facts, value, "compose_dependency")?;
                }
            }
            Some("networks" | "volumes") => {
                complete &= written(
                    facts,
                    value,
                    if key.scalar() == Some("networks") {
                        "network_reference"
                    } else {
                        "volume_reference"
                    },
                )?
            }
            Some(
                "environment" | "env_file" | "configs" | "secrets" | "ports" | "command"
                | "entrypoint" | "profiles" | "healthcheck",
            ) => complete &= written(facts, value, "compose_config")?,
            Some("extends") => {
                complete &= written(facts, value, "compose_written_reference")?;
                complete &= wrong(
                    facts,
                    value,
                    "configuration without external inheritance; extends is not resolved",
                )?;
            }
            _ => {}
        }
    }
    Ok(complete)
}

fn written(facts: &mut Facts<'_>, value: &Value, kind: &str) -> Result<bool> {
    if matches!(value.kind, structured_yaml::Kind::Null) {
        return Ok(true);
    }
    let mut complete = true;
    if let Some(entries) = value.mapping() {
        for (key, value) in entries {
            complete &= emit(facts, key, kind)?;
            complete &= written(facts, value, kind)?;
        }
    } else if let Some(values) = value.sequence() {
        for value in values {
            complete &= written(facts, value, kind)?;
        }
    } else {
        complete &= emit(facts, value, kind)?;
    }
    Ok(complete)
}
fn wrong(facts: &mut Facts<'_>, value: &Value, expected: &str) -> Result<bool> {
    facts.push(
        value.start,
        value.end,
        "extraction_diagnostic",
        "yaml_shape",
        &format!("Expected {expected}"),
    )?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_services_resources_and_unexpanded_values() -> Result<()> {
        let text = "services:\n  api:\n    image: example/api:${TAG}\n    build: {context: ./api, dockerfile: Dockerfile}\n    depends_on: {database: {condition: service_healthy}}\n    volumes: [cache:/cache]\n    networks: [internal]\n  database:\n    image: example/db:v1\nvolumes: {cache: {}}\nnetworks: {internal: {}}\n";
        let result = analyze("compose.yaml", text)?;
        for kind in [
            "compose_service",
            "compose_image",
            "build_context",
            "compose_dependency",
            "volume_reference",
            "network_reference",
            "compose_volume",
            "compose_network",
        ] {
            assert!(result.facts.iter().any(|f| f.kind == kind), "{kind}");
        }
        assert!(result.facts.iter().any(|f| f.name == "example/api:${TAG}"));
        Ok(())
    }
    #[test]
    fn wrong_service_shape_is_explicit() -> Result<()> {
        let result = analyze("compose.yml", "services: [wrong]\n")?;
        assert_eq!(result.status, "partial-syntax");
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "extraction_diagnostic")
        );
        Ok(())
    }
}
