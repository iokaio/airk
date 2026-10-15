// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
pub(super) fn recognized(value: &Value) -> bool {
    named(value, "openapi")
        .and_then(Value::scalar)
        .is_some_and(|v| v.starts_with("3."))
        || named(value, "swagger").and_then(Value::scalar) == Some("2.0")
}
pub(super) fn extract(facts: &mut Facts<'_>, document: &Value) -> Result<bool> {
    if !recognized(document) {
        return issue(facts, document, "Unsupported or ambiguous OpenAPI version");
    }
    let mut refs = Vec::new();
    let mut complete = walk(facts, document, None, &mut refs)?;
    for name in ["paths", "components", "definitions"] {
        if let Some(value) = named(document, name)
            && value.mapping().is_none()
        {
            complete &= issue(
                facts,
                value,
                "OpenAPI paths/components/definitions must be mappings",
            )?;
        }
    }
    let mut edges = 0usize;
    let mut steps = 0usize;
    for reference in refs {
        complete &= follow(
            facts,
            document,
            reference,
            &mut edges,
            &mut steps,
            &mut BTreeSet::new(),
        )?;
    }
    Ok(complete)
}
fn walk<'a>(
    facts: &mut Facts<'_>,
    value: &'a Value,
    parent: Option<&str>,
    refs: &mut Vec<&'a Value>,
) -> Result<bool> {
    let mut complete = true;
    match &value.kind {
        Kind::Mapping(entries) => {
            for (key, value) in entries {
                let name = key.scalar();
                let kind = match (parent, name) {
                    (Some("paths"), Some(name)) if name.starts_with('/') => Some("api_path"),
                    (
                        _,
                        Some(
                            "get" | "put" | "post" | "delete" | "options" | "head" | "patch"
                            | "trace",
                        ),
                    ) if parent.is_some_and(|p| p.starts_with('/')) => Some("api_method"),
                    (Some("schemas" | "definitions"), _) => Some("api_schema"),
                    (Some("responses"), _) => Some("api_response"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    complete &= emit(facts, key, kind)?;
                }
                match name {
                    Some("operationId") => complete &= emit(facts, value, "api_operation")?,
                    Some("parameters") => complete &= written(facts, value, "api_parameter")?,
                    Some("requestBody") => complete &= written(facts, value, "api_request")?,
                    Some("$ref") => {
                        complete &= emit(facts, value, "api_reference")?;
                        refs.push(value);
                    }
                    Some("openapi" | "swagger") => complete &= emit(facts, value, "api_version")?,
                    _ => {}
                }
                complete &= walk(facts, value, name, refs)?;
            }
        }
        Kind::Sequence(values) => {
            for value in values {
                complete &= walk(facts, value, parent, refs)?;
            }
        }
        Kind::Unknown => complete &= issue(facts, value, "Unsupported OpenAPI structure")?,
        _ => {}
    }
    Ok(complete)
}
fn resolve<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer == "#" {
        return Some(root);
    }
    let mut value = root;
    for raw in pointer.strip_prefix("#/")?.split('/') {
        if raw.contains('%') {
            return None;
        }
        let mut token = String::new();
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c == '~' {
                token.push(match chars.next()? {
                    '0' => '~',
                    '1' => '/',
                    _ => return None,
                });
            } else {
                token.push(c);
            }
        }
        value = match &value.kind {
            Kind::Mapping(_) => named(value, &token)?,
            Kind::Sequence(items) => items.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}
fn follow(
    facts: &mut Facts<'_>,
    root: &Value,
    reference: &Value,
    edges: &mut usize,
    steps: &mut usize,
    active: &mut BTreeSet<(usize, usize)>,
) -> Result<bool> {
    let Some(name) = reference.scalar() else {
        return Ok(false);
    };
    if !name.starts_with('#') {
        return issue(
            facts,
            reference,
            "External or cross-file OpenAPI reference reported but not fetched",
        );
    }
    if active.len() >= 16 || *edges >= 1024 {
        return issue(
            facts,
            reference,
            "OpenAPI local reference traversal budget exceeded",
        );
    }
    *edges += 1;
    let Some(target) = resolve(root, name) else {
        return issue(
            facts,
            reference,
            "Unresolved or unsupported local JSON pointer",
        );
    };
    if !active.insert((target.start, target.end)) {
        return issue(
            facts,
            reference,
            "Cyclic local OpenAPI reference; traversal stopped",
        );
    }
    let mut pending = vec![target];
    let mut complete = true;
    while let Some(value) = pending.pop() {
        *steps += 1;
        if *steps > 100_000 {
            active.remove(&(target.start, target.end));
            return issue(facts, reference, "OpenAPI reference node budget exceeded");
        }
        match &value.kind {
            Kind::Mapping(entries) => {
                for (key, value) in entries {
                    if key.scalar() == Some("$ref") {
                        complete &= follow(facts, root, value, edges, steps, active)?;
                    } else {
                        pending.push(value);
                    }
                }
            }
            Kind::Sequence(values) => pending.extend(values),
            _ => {}
        }
    }
    active.remove(&(target.start, target.end));
    Ok(complete)
}
