// SPDX-License-Identifier: Apache-2.0
//! Finite schema overlays on ordered, original-offset structural evidence.
mod gitlab;
mod kubernetes;
mod manifests;
mod openapi;
use crate::{
    build_facts::Facts,
    structured_view::{self, Kind, Value},
};
use anyhow::Result;

/// Bounded selection hint. Extraction validates the complete document again.
pub(crate) fn detect(path: &str, bytes: &[u8]) -> Option<&'static str> {
    let text = std::str::from_utf8(bytes.get(..bytes.len().min(4096))?).ok()?;
    let basename = path.rsplit('/').next()?;
    let manifest = matches!(
        basename,
        "package.json" | "composer.json" | "Cargo.toml" | "pyproject.toml"
    ) || (basename.starts_with("tsconfig") && basename.ends_with(".json"));
    // Parse the bounded prefix rather than finding schema words inside comments/strings.
    let view = structured_view::parse(path, text, true).ok()?;
    for document in &view.documents {
        if basename == ".gitlab-ci.yml"
            && document.mapping().is_some_and(|pairs| {
                pairs.iter().any(|(key, value)| {
                    matches!(key.scalar(), Some("stages" | "include" | "workflow"))
                        || value.mapping().is_some_and(|pairs| {
                            pairs.iter().any(|(key, _)| {
                                matches!(key.scalar(), Some("script" | "trigger" | "extends"))
                            })
                        })
                })
            })
        {
            return Some("gitlab-ci");
        }
        if manifest && manifests::recognized(path, document) {
            return Some("project-manifest");
        }
        if openapi::recognized(document) {
            return Some("openapi");
        }
        if kubernetes::recognized(document) {
            return Some("kubernetes");
        }
    }
    None
}

/// Include this discriminator in the selected profile options.
pub(crate) fn flavor(path: &str, adapter: &str) -> String {
    let basename = path.rsplit('/').next().unwrap_or(path);
    let parser = if path.ends_with(".toml") {
        "toml"
    } else if path.ends_with(".json") || path.ends_with(".jsonc") {
        "jsonc"
    } else {
        "yaml"
    };
    let schema = if adapter == "project-manifest" {
        if basename.starts_with("tsconfig") {
            "tsconfig"
        } else {
            basename
        }
    } else {
        adapter
    };
    format!("project-facts/1:{parser}:{schema}")
}

pub(crate) fn analyze_selected(
    path: &str,
    text: &str,
    adapter: &str,
) -> Result<crate::syntax::Analysis> {
    let view = structured_view::parse(path, text, adapter == "kubernetes")?;
    let mut facts = Facts::new(path, text, adapter)?;
    for (start, end, reason) in &view.issues {
        facts.push(
            *start,
            *end,
            "extraction_diagnostic",
            "schema_diagnostic",
            reason,
        )?;
    }
    let mut complete = view.issues.is_empty();
    for document in &view.documents {
        complete &= match adapter {
            "kubernetes" => kubernetes::extract(&mut facts, document)?,
            "openapi" => openapi::extract(&mut facts, document)?,
            "gitlab-ci" => gitlab::extract(&mut facts, document)?,
            "project-manifest" => manifests::extract(&mut facts, path, document)?,
            _ => anyhow::bail!("unsupported project schema profile"),
        };
    }
    for fact in &mut facts.facts {
        if let Some(code) = &mut fact.code {
            code.producer_version = "project-facts/1".into();
        }
    }
    Ok(facts.finish(!complete))
}
fn entries(value: &Value) -> &[(Value, Value)] {
    value.mapping().unwrap_or(&[])
}
fn named<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    let mut matching = entries(value)
        .iter()
        .filter(|(key, _)| key.scalar() == Some(name));
    let (_, value) = matching.next()?;
    matching.next().is_none().then_some(value)
}
fn emit(facts: &mut Facts<'_>, value: &Value, kind: &str) -> Result<bool> {
    crate::structured_yaml::emit(facts, value, kind)
}
fn issue(facts: &mut Facts<'_>, value: &Value, reason: &str) -> Result<bool> {
    if value.start < value.end {
        facts.push(
            value.start,
            value.end,
            "extraction_diagnostic",
            "schema_diagnostic",
            reason,
        )?;
    }
    Ok(false)
}
fn written(facts: &mut Facts<'_>, value: &Value, kind: &str) -> Result<bool> {
    let mut complete = true;
    match &value.kind {
        Kind::Mapping(values) => {
            for (key, value) in values {
                complete &= emit(facts, key, kind)?;
                complete &= written(facts, value, kind)?;
            }
        }
        Kind::Sequence(values) => {
            for value in values {
                complete &= written(facts, value, kind)?;
            }
        }
        Kind::Null => {}
        _ => complete &= emit(facts, value, kind)?,
    }
    Ok(complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_finite_schema_facts_preserve_source_locations() -> Result<()> {
        let fixtures = [
            (
                "resources.yaml",
                include_str!("../tests/fixtures/project-schemas/resources.yaml"),
                "kubernetes",
                "resource_name",
                "beacon-service",
            ),
            (
                "api.yaml",
                include_str!("../tests/fixtures/project-schemas/openapi.yaml"),
                "openapi",
                "api_operation",
                "listBeacons",
            ),
            (
                ".gitlab-ci.yml",
                include_str!("../tests/fixtures/project-schemas/gitlab.yml"),
                "gitlab-ci",
                "job_needs",
                "assemble",
            ),
            (
                "package.json",
                include_str!("../tests/fixtures/project-schemas/package.json"),
                "project-manifest",
                "package_dependency",
                "example-library",
            ),
            (
                "tsconfig.json",
                include_str!("../tests/fixtures/project-schemas/tsconfig.json"),
                "project-manifest",
                "compiler_option",
                "strict",
            ),
            (
                "composer.json",
                include_str!("../tests/fixtures/project-schemas/composer.json"),
                "project-manifest",
                "package_dependency",
                "example/library",
            ),
            (
                "Cargo.toml",
                include_str!("../tests/fixtures/project-schemas/Cargo.toml"),
                "project-manifest",
                "workspace_configuration",
                "local-member",
            ),
            (
                "pyproject.toml",
                include_str!("../tests/fixtures/project-schemas/pyproject.toml"),
                "project-manifest",
                "build_system",
                "example-builder",
            ),
        ];
        for (path, text, adapter, kind, name) in fixtures {
            assert_eq!(detect(path, text.as_bytes()), Some(adapter), "{path}");
            let result = analyze_selected(path, text, adapter)?;
            assert!(
                result
                    .facts
                    .iter()
                    .any(|f| f.kind == kind && f.name == name),
                "{path} missing {kind}/{name}"
            );
            for fact in result.facts {
                let span = fact.code.unwrap().span.unwrap();
                assert!(text.get(span.start_byte..span.end_byte).is_some());
                assert_eq!(
                    fact.start_line,
                    text[..span.start_byte]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1
                );
            }
        }
        Ok(())
    }
    #[test]
    fn unknown_schema_comments_and_strings_do_not_select_overlays() {
        assert_eq!(
            detect(
                "notes.yaml",
                b"note: 'apiVersion: v1 kind: Service metadata: {}'\n"
            ),
            None
        );
        assert_eq!(detect("package.json", br#"{"unrecognized":true}"#), None);
        assert_eq!(detect("other.json", br#"{"name":"package"}"#), None);
    }
    #[test]
    fn references_cycles_external_spelling_duplicates_and_limits_are_partial() -> Result<()> {
        let text = r##"{"openapi":"3.1.0","components":{"schemas":{"Loop":{"$ref":"#/components/schemas/Loop"},"External":{"$ref":"https://example.invalid/schema"}}}}"##;
        let result = analyze_selected("api.json", text, "openapi")?;
        assert_eq!(result.status, "partial-syntax");
        assert!(result.facts.iter().any(|f| f.name.contains("Cyclic")));
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "api_reference" && f.name == "https://example.invalid/schema")
        );
        let duplicate = analyze_selected(
            "package.json",
            r#"{"name":"first","name":"second"}"#,
            "project-manifest",
        )?;
        assert_eq!(duplicate.status, "partial-syntax");
        assert_eq!(
            duplicate
                .facts
                .iter()
                .filter(|f| f.kind == "package_identity"
                    && matches!(f.name.as_str(), "first" | "second"))
                .count(),
            2
        );
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(structured_view::parse("deep.json", &deep, false).is_err());
        Ok(())
    }
}
