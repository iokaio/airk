// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn recognized(value: &Value) -> bool {
    named(value, "apiVersion").and_then(Value::scalar).is_some()
        && named(value, "kind").and_then(Value::scalar).is_some()
        && named(value, "metadata").and_then(Value::mapping).is_some()
}
pub(super) fn extract(facts: &mut Facts<'_>, value: &Value) -> Result<bool> {
    if !recognized(value) {
        return issue(
            facts,
            value,
            "Kubernetes document requires unique apiVersion, kind and metadata mapping",
        );
    }
    let mut complete = true;
    for (key, value) in entries(value) {
        match key.scalar() {
            Some("apiVersion") => complete &= emit(facts, value, "kubernetes_api_version")?,
            Some("kind") => complete &= emit(facts, value, "kubernetes_kind")?,
            Some("metadata") => {
                for (key, value) in entries(value) {
                    match key.scalar() {
                        Some("name") => complete &= emit(facts, value, "resource_name")?,
                        Some("namespace") => complete &= emit(facts, value, "resource_namespace")?,
                        Some("labels" | "annotations") => {
                            complete &= written(facts, value, "resource_metadata")?
                        }
                        _ => {}
                    }
                }
            }
            Some("spec" | "data" | "stringData") => complete &= walk(facts, value)?,
            Some("items") => {
                if let Some(items) = value.sequence() {
                    for item in items {
                        complete &= extract(facts, item)?;
                    }
                } else {
                    complete &= issue(facts, value, "Kubernetes items must be a sequence")?;
                }
            }
            _ => {}
        }
    }
    Ok(complete)
}
fn walk(facts: &mut Facts<'_>, value: &Value) -> Result<bool> {
    let mut complete = true;
    match &value.kind {
        Kind::Mapping(entries) => {
            for (key, value) in entries {
                let kind = match key.scalar() {
                    Some("selector" | "matchLabels" | "matchExpressions") => {
                        Some("resource_selector")
                    }
                    Some("image") => Some("container_image"),
                    Some(
                        "serviceAccountName" | "claimName" | "secretName" | "configMapRef"
                        | "secretRef" | "configMapKeyRef" | "secretKeyRef",
                    ) => Some("resource_reference"),
                    Some("ports" | "targetPort" | "port" | "type" | "replicas") => {
                        Some("resource_configuration")
                    }
                    _ => None,
                };
                if let Some(kind) = kind {
                    complete &= written(facts, value, kind)?;
                } else {
                    complete &= walk(facts, value)?;
                }
            }
        }
        Kind::Sequence(values) => {
            for value in values {
                complete &= walk(facts, value)?;
            }
        }
        Kind::Unknown => complete &= issue(facts, value, "Unsupported Kubernetes structure")?,
        _ => {}
    }
    Ok(complete)
}
