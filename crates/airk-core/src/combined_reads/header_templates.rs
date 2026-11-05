// SPDX-License-Identifier: Apache-2.0
//! Lossless metadata templates. Evidence rows are never templated or moved.
use super::shared_values::canonical_value;
use crate::{
    identity::{Sha256Digest, framed_digest},
    index_catalog::{MAX_SELECTION, encoded_size},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const HEADER_SCHEMA: &str = "airk.shared-native-header-templates/1";
pub const HEADER_REFERENCE: &str = "$airk_shared_native_header_fields";
pub const MAX_HEADER_SLOTS: usize = 512;
pub const HEADER_ROOTS: &[&str] = &[
    "collection",
    "discovery_observation",
    "discovery_readiness",
    "semantic_readiness",
    "execution",
    "freshness",
    "generation",
    "publication",
    "request_id",
    "scope_observation",
    "truncated",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
pub enum SharedNativeValues {
    #[serde(rename = "airk.shared-native-header-templates/1")]
    Headers {
        values: BTreeMap<Sha256Digest, NativeHeaderTemplate>,
    },
    #[serde(rename = "airk.shared-capability-fields/1")]
    Capabilities {
        values: BTreeMap<Sha256Digest, Map<String, Value>>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedHeaderTemplates {
    pub schema: String,
    pub values: BTreeMap<Sha256Digest, NativeHeaderTemplate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHeaderTemplate {
    pub fields: Map<String, Value>,
    pub slots: Vec<HeaderSlot>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderSlot {
    pub path: Vec<HeaderSegment>,
    pub kind: PrimitiveKind,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HeaderSegment {
    Key(String),
    Index(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    String,
    Boolean,
    Integer,
    Null,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderReference {
    pub id: Sha256Digest,
    pub values: Vec<Value>,
}

fn primitive(value: &Value) -> Option<PrimitiveKind> {
    match value {
        Value::String(_) => Some(PrimitiveKind::String),
        Value::Bool(_) => Some(PrimitiveKind::Boolean),
        Value::Null => Some(PrimitiveKind::Null),
        Value::Number(n)
            if n.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
                || n.as_i64()
                    .is_some_and(|n| n.unsigned_abs() <= 9_007_199_254_740_991) =>
        {
            Some(PrimitiveKind::Integer)
        }
        _ => None,
    }
}
fn bounded_header_value(value: &Value) -> bool {
    match value {
        Value::String(value) => value.len() <= 8192,
        Value::Object(fields) => {
            fields.len() <= 1024
                && fields
                    .iter()
                    .all(|(key, value)| key.len() <= 128 && bounded_header_value(value))
        }
        Value::Array(items) => items.len() <= 1024 && items.iter().all(bounded_header_value),
        _ => true,
    }
}
fn shape(value: &Value) -> Result<Value> {
    Ok(match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| Ok((key.clone(), shape(value)?)))
                .collect::<Result<_, anyhow::Error>>()?,
        ),
        Value::Array(items) => Value::Array(items.iter().map(shape).collect::<Result<_>>()?),
        _ => serde_json::to_value(primitive(value).context("unsupported header primitive")?)?,
    })
}
fn template_digest(template: &NativeHeaderTemplate) -> Result<Sha256Digest> {
    // Value canonicalization sorts struct keys as well as all nested objects.
    Ok(framed_digest(
        b"airk/shared-native-header-template/1",
        &[&serde_json::to_vec(&serde_json::to_value(template)?)?],
    ))
}
fn allowed_path(path: &[HeaderSegment]) -> bool {
    !path.is_empty()
        && path.len() <= 16
        && matches!(&path[0],HeaderSegment::Key(key) if HEADER_ROOTS.contains(&key.as_str()))
        && path.iter().all(|segment| match segment {
            HeaderSegment::Key(key) => {
                !key.is_empty()
                    && key.len() <= 128
                    && key.is_ascii()
                    && !matches!(
                        key.as_str(),
                        "__proto__" | "constructor" | "prototype" | HEADER_REFERENCE
                    )
            }
            HeaderSegment::Index(offset) => *offset < 1024,
        })
}
fn at<'a>(value: &'a Value, path: &[HeaderSegment]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, segment| match segment {
        HeaderSegment::Key(key) => value.get(key),
        HeaderSegment::Index(offset) => value.get(*offset),
    })
}
fn at_mut<'a>(value: &'a mut Value, path: &[HeaderSegment]) -> Option<&'a mut Value> {
    path.iter().try_fold(value, |value, segment| match segment {
        HeaderSegment::Key(key) => value.get_mut(key),
        HeaderSegment::Index(offset) => value.get_mut(*offset),
    })
}
fn collect_slots(
    values: &[&Value],
    path: &mut Vec<HeaderSegment>,
    slots: &mut Vec<HeaderSlot>,
) -> Result<()> {
    if values.iter().all(|value| *value == values[0]) {
        return Ok(());
    }
    match values[0] {
        Value::Object(fields) => {
            for key in fields.keys() {
                path.push(HeaderSegment::Key(key.clone()));
                collect_slots(
                    &values
                        .iter()
                        .map(|value| value.get(key).context("header shape changed"))
                        .collect::<Result<Vec<_>>>()?,
                    path,
                    slots,
                )?;
                path.pop();
            }
        }
        Value::Array(items) => {
            for offset in 0..items.len() {
                path.push(HeaderSegment::Index(offset));
                collect_slots(
                    &values
                        .iter()
                        .map(|value| value.get(offset).context("header array shape changed"))
                        .collect::<Result<Vec<_>>>()?,
                    path,
                    slots,
                )?;
                path.pop();
            }
        }
        _ => {
            ensure!(
                slots.len() < MAX_HEADER_SLOTS && allowed_path(path),
                "native header slot bound exceeded"
            );
            let kind = primitive(values[0]).context("unsupported native header slot")?;
            ensure!(
                values.iter().all(|value| primitive(value) == Some(kind)),
                "native header primitive shape changed"
            );
            slots.push(HeaderSlot {
                path: path.clone(),
                kind,
            });
        }
    }
    Ok(())
}
impl SharedHeaderTemplates {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == HEADER_SCHEMA
                && !self.values.is_empty()
                && self.values.len() <= MAX_SELECTION,
            "invalid native header template namespace"
        );
        encoded_size(self, crate::MAX_RESPONSE_BYTES)?;
        for (digest, template) in &self.values {
            ensure!(
                !template.fields.is_empty()
                    && template
                        .fields
                        .keys()
                        .all(|key| HEADER_ROOTS.contains(&key.as_str()))
                    && template.slots.len() <= MAX_HEADER_SLOTS,
                "invalid native header roots or slot count"
            );
            let fields = Value::Object(template.fields.clone());
            ensure!(
                canonical_value(&fields, 0, &mut 32_768) && bounded_header_value(&fields),
                "invalid native header template value"
            );
            let mut seen = BTreeSet::new();
            for slot in &template.slots {
                ensure!(
                    allowed_path(&slot.path)
                        && seen.insert(&slot.path)
                        && at(&fields, &slot.path).is_some_and(Value::is_null),
                    "invalid native header slot target"
                );
            }
            ensure!(
                *digest == template_digest(template)?,
                "native header template digest mismatch"
            );
        }
        Ok(())
    }
}

pub fn share_native_headers(results: &mut [&mut Value]) -> Result<Option<SharedHeaderTemplates>> {
    ensure!(
        results.len() <= MAX_SELECTION,
        "too many native header members"
    );
    let mut headers = Vec::with_capacity(results.len());
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (offset, result) in results.iter().enumerate() {
        ensure!(
            result.is_object() && result.get(HEADER_REFERENCE).is_none(),
            "native header member is not canonical"
        );
        let fields = HEADER_ROOTS
            .iter()
            .filter_map(|key| {
                result
                    .get(*key)
                    .filter(|value| {
                        canonical_value(value, 0, &mut 32_768) && bounded_header_value(value)
                    })
                    .map(|value| ((*key).to_owned(), value.clone()))
            })
            .collect::<Map<_, _>>();
        let header = Value::Object(fields);
        let key = serde_json::to_string(&shape(&header)?)?;
        groups.entry(key).or_default().push(offset);
        headers.push(header);
    }
    let mut table = BTreeMap::new();
    for indices in groups.values() {
        let values = indices
            .iter()
            .map(|index| &headers[*index])
            .collect::<Vec<_>>();
        if values[0].as_object().is_none_or(Map::is_empty) {
            continue;
        }
        let mut slots = Vec::new();
        collect_slots(&values, &mut Vec::new(), &mut slots)?;
        let mut fields = values[0].clone();
        for slot in &slots {
            *at_mut(&mut fields, &slot.path).context("native header slot missing")? = Value::Null;
        }
        let template = NativeHeaderTemplate {
            fields: fields
                .as_object()
                .context("native header is not an object")?
                .clone(),
            slots,
        };
        let id = template_digest(&template)?;
        let references = values
            .iter()
            .map(|value| {
                Ok(HeaderReference {
                    id,
                    values: template
                        .slots
                        .iter()
                        .map(|slot| {
                            at(value, &slot.path)
                                .cloned()
                                .context("native header slot value missing")
                        })
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let old = values
            .iter()
            .map(|value| encoded_size(value, crate::MAX_RESPONSE_BYTES))
            .sum::<Result<usize>>()?;
        let new = encoded_size(&template, crate::MAX_RESPONSE_BYTES)?
            + references
                .iter()
                .map(|reference| {
                    encoded_size(reference, crate::MAX_RESPONSE_BYTES)
                        .map(|bytes| bytes + HEADER_REFERENCE.len() + 6)
                })
                .sum::<Result<usize>>()?
            + id.to_string().len()
            + 6;
        if new >= old {
            continue;
        }
        for (offset, reference) in indices.iter().zip(references) {
            let body = results[*offset]
                .as_object_mut()
                .context("native header member changed shape")?;
            for key in template.fields.keys() {
                body.remove(key);
            }
            body.insert(HEADER_REFERENCE.into(), serde_json::to_value(reference)?);
        }
        table.insert(id, template);
    }
    if table.is_empty() {
        return Ok(None);
    }
    let table = SharedHeaderTemplates {
        schema: HEADER_SCHEMA.into(),
        values: table,
    };
    table.validate()?;
    Ok(Some(table))
}

pub fn materialize_header(table: &SharedHeaderTemplates, result: &Value) -> Result<Value> {
    table.validate()?;
    let Some(reference) = result.get(HEADER_REFERENCE) else {
        return Ok(result.clone());
    };
    let reference: HeaderReference =
        serde_json::from_value(reference.clone()).context("invalid native header reference")?;
    let template = table
        .values
        .get(&reference.id)
        .context("native header reference is missing")?;
    ensure!(
        reference.values.len() == template.slots.len(),
        "native header slot count mismatch"
    );
    let mut fields = Value::Object(template.fields.clone());
    for (slot, value) in template.slots.iter().zip(reference.values) {
        ensure!(
            primitive(&value) == Some(slot.kind)
                && value.as_str().is_none_or(|value| value.len() <= 8192),
            "native header slot primitive mismatch"
        );
        *at_mut(&mut fields, &slot.path).context("native header slot target is missing")? = value;
    }
    let mut body = result
        .as_object()
        .context("native header result is not an object")?
        .clone();
    body.remove(HEADER_REFERENCE);
    let fields = fields
        .as_object()
        .context("native header fields are not an object")?;
    ensure!(
        fields.keys().all(|key| !body.contains_key(key)),
        "native header fields collide with inline member data"
    );
    body.extend(fields.clone());
    let value = Value::Object(body);
    encoded_size(&value, crate::MAX_RESPONSE_BYTES)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn bodies() -> Vec<Value> {
        (0..32).map(|n|json!({"collection":format!("fixture{n}"),"generation":2,"truncated":false,
        "request_id":format!("{n:032x}"),"publication":{"member":n},"execution":{"version":1,"served_mode":"lexical","requested_mode":"lexical","incomplete":false},
        "freshness":{"observed_unix_ms":1000+n,"state":"unavailable","error":"original bounded fixture ".repeat(20),"live_source_verified":false},
        "scope_observation":{"schema":1,"fingerprint":format!("{n:064x}"),"grant":{"id":"original-fixture","revision":1},"members":[{"index_id":format!("index-{n}"),"source_revision":1,"policy_epoch":0}]},
        "hits":[{"citation":{"path":"api/one.rs","content_hash":"a".repeat(64)},"text":"original evidence"}],
        "source_verification":{"original_complete_receipt":n}})).collect()
    }
    #[test]
    fn typed_slots_roundtrip_all_thirty_two_individual_observations_and_leave_evidence_inline()
    -> Result<()> {
        let originals = bodies();
        let mut bodies = originals.clone();
        let table = share_native_headers(&mut bodies.iter_mut().collect::<Vec<_>>())?
            .context("templates")?;
        assert_eq!(table.values.len(), 1);
        assert!(!table.values.values().next().unwrap().slots.is_empty());
        for (body, original) in bodies.iter().zip(&originals) {
            assert_eq!(body["hits"], original["hits"]);
            assert_eq!(body["source_verification"], original["source_verification"]);
            assert_eq!(materialize_header(&table, body)?, *original);
        }
        Ok(())
    }
    #[test]
    fn header_decoder_refuses_wrong_kind_count_target_namespace_collision_and_evidence_root()
    -> Result<()> {
        let mut bodies = bodies();
        let table = share_native_headers(&mut bodies.iter_mut().collect::<Vec<_>>())?
            .context("templates")?;
        let first = &bodies[0];
        let mut bad = first.clone();
        bad[HEADER_REFERENCE]["values"][0] = json!({"object":"cannot inject"});
        assert!(materialize_header(&table, &bad).is_err());
        let mut bad = first.clone();
        bad[HEADER_REFERENCE]["values"] = json!([]);
        assert!(materialize_header(&table, &bad).is_err());
        let mut bad = first.clone();
        bad["generation"] = json!(99);
        assert!(materialize_header(&table, &bad).is_err());
        let mut bad = table.clone();
        bad.schema = "another-namespace/1".into();
        assert!(materialize_header(&bad, first).is_err());
        let mut bad = table.clone();
        bad.values
            .values_mut()
            .next()
            .unwrap()
            .fields
            .insert("hits".into(), json!([]));
        assert!(materialize_header(&bad, first).is_err());
        let mut bad = table.clone();
        bad.values.values_mut().next().unwrap().slots[0].path =
            vec![HeaderSegment::Key("hits".into()), HeaderSegment::Index(0)];
        assert!(materialize_header(&bad, first).is_err());
        let mut bad = table.clone();
        bad.values.values_mut().next().unwrap().slots[0].path =
            vec![HeaderSegment::Key("execution".into())];
        assert!(materialize_header(&bad, first).is_err());
        let mut bad = first.clone();
        bad[HEADER_REFERENCE]["id"] = json!("00".repeat(32));
        assert!(materialize_header(&table, &bad).is_err());
        Ok(())
    }
    #[test]
    fn overlong_varying_strings_and_unprofitable_headers_remain_exactly_inline() -> Result<()> {
        let mut bodies = bodies();
        for (offset, body) in bodies.iter_mut().enumerate() {
            body["freshness"]["error"] = json!(format!("{offset}{}", "x".repeat(8193)));
        }
        let originals = bodies.clone();
        let table = share_native_headers(&mut bodies.iter_mut().collect::<Vec<_>>())?
            .context("other headers")?;
        for (body, original) in bodies.iter().zip(originals) {
            assert_eq!(body["freshness"], original["freshness"]);
            assert_eq!(materialize_header(&table, body)?, original);
        }
        let original = json!({"generation":1,"truncated":false,"hits":[]});
        let mut tiny = [original.clone()];
        assert!(share_native_headers(&mut tiny.iter_mut().collect::<Vec<_>>())?.is_none());
        assert_eq!(tiny[0], original);
        Ok(())
    }
    #[test]
    fn native_header_template_digest_matches_host_golden() -> Result<()> {
        let template = NativeHeaderTemplate {
            fields: json!({"freshness":{"observed_unix_ms":null,"state":"unavailable"}})
                .as_object()
                .unwrap()
                .clone(),
            slots: vec![HeaderSlot {
                path: vec![
                    HeaderSegment::Key("freshness".into()),
                    HeaderSegment::Key("observed_unix_ms".into()),
                ],
                kind: PrimitiveKind::Integer,
            }],
        };
        assert_eq!(
            template_digest(&template)?.to_string(),
            "3c2b1d950550b1d7514895aa1645c730a1221132ba50b864215167c9d49f3bc9"
        );
        Ok(())
    }
}
