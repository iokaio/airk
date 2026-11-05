// SPDX-License-Identifier: Apache-2.0
//! Exact, capability-only common fields. References compress representation;
//! they never omit native data or convey authority across member observations.
use crate::{
    combined_reads::COMBINED_READ_SCHEMA,
    identity::{Sha256Digest, framed_digest},
    index_catalog::{IndexId, MAX_SELECTION, encoded_size, unique_bounded},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const SHARED_CAPABILITY_SCHEMA: &str = "airk.shared-capability-fields/1";
pub const SHARED_CAPABILITY_REFERENCE: &str = "$airk_shared_capability_fields";
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedCapabilityValues {
    pub schema: String,
    pub values: BTreeMap<Sha256Digest, Map<String, Value>>,
}

/// Observation fields stay physically attached to their individual member,
/// even when two current observations happen to have identical values.
pub fn shareable_capability_field(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key.is_ascii()
        && !key.starts_with("runtime")
        && !key.ends_with("_observation")
        && !matches!(
            key,
            SHARED_CAPABILITY_REFERENCE
                | "__proto__"
                | "constructor"
                | "prototype"
                | "$airk_shared_native_header_fields"
                | "request_admission"
                | "runtime"
                | "work"
                | "scope_observation"
                | "publication"
                | "freshness"
                | "collection"
                | "generation"
                | "observed_unix_ms"
                | "cache_age_ms"
                | "cached"
                | "status_observation"
                | "request_id"
                | "query_concurrency"
                | "source_verification"
                | "error"
                | "requested_scope"
        )
}

/// The shared subset uses the same canonical JSON bytes in Rust and host JS:
/// recursively sorted ASCII keys, strings, booleans, null and safe integers.
/// Other native values remain inline and therefore retain their exact encoding.
pub(crate) fn canonical_value(value: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth > 32 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => true,
        Value::Number(number) => {
            number.as_u64().is_some_and(|n| n <= MAX_SAFE_INTEGER)
                || number
                    .as_i64()
                    .is_some_and(|n| n.unsigned_abs() <= MAX_SAFE_INTEGER)
        }
        Value::Array(items) => items
            .iter()
            .all(|item| canonical_value(item, depth + 1, remaining)),
        Value::Object(fields) => fields.iter().all(|(key, value)| {
            key.is_ascii()
                && !matches!(
                    key.as_str(),
                    SHARED_CAPABILITY_REFERENCE
                        | "$airk_shared_native_header_fields"
                        | "__proto__"
                        | "constructor"
                        | "prototype"
                )
                && canonical_value(value, depth + 1, remaining)
        }),
    }
}

fn identity(fields: &Map<String, Value>) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/shared-capability-fields/1",
        &[&serde_json::to_vec(fields)?],
    ))
}

impl SharedCapabilityValues {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SHARED_CAPABILITY_SCHEMA
                && !self.values.is_empty()
                && self.values.len() <= MAX_SELECTION,
            "unsupported shared capability values"
        );
        encoded_size(self, crate::MAX_RESPONSE_BYTES)?;
        for (digest, fields) in &self.values {
            ensure!(
                !fields.is_empty()
                    && fields.keys().all(|key| shareable_capability_field(key))
                    && canonical_value(&Value::Object(fields.clone()), 0, &mut 32_768),
                "invalid shared capability field group"
            );
            ensure!(
                *digest == identity(fields)?,
                "shared capability value digest mismatch"
            );
        }
        Ok(())
    }
}

/// Called only by the capability composer, on successful canonical member
/// bodies. One reference names an entire common field group, rather than
/// multiplying a per-field reference catalog by up to 32 members.
pub fn share_capability_fields(
    results: &mut [&mut Value],
) -> Result<Option<SharedCapabilityValues>> {
    ensure!(
        results.len() <= MAX_SELECTION,
        "too many shared capability members"
    );
    let Some(first) = results.first() else {
        return Ok(None);
    };
    let first = first
        .as_object()
        .context("capability member is not a canonical object")?;
    for result in results.iter() {
        ensure!(
            result.is_object()
                && result.get(SHARED_CAPABILITY_REFERENCE).is_none()
                && result.get("structuredContent").is_none()
                && result.get("content").is_none(),
            "capability sharing requires an unreferenced canonical native body"
        );
        encoded_size(result, crate::MAX_RESPONSE_BYTES)?;
    }
    let common = first
        .iter()
        .filter(|(key, value)| {
            shareable_capability_field(key)
                && canonical_value(value, 0, &mut 32_768)
                && results
                    .iter()
                    .all(|result| result.get(key.as_str()) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<_, _>>();
    if common.is_empty() {
        return Ok(None);
    }
    let digest = identity(&common)?;
    for result in results.iter_mut() {
        let body = result
            .as_object_mut()
            .context("capability member is not an object")?;
        for key in common.keys() {
            body.remove(key);
        }
        body.insert(
            SHARED_CAPABILITY_REFERENCE.into(),
            Value::String(digest.to_string()),
        );
    }
    let shared = SharedCapabilityValues {
        schema: SHARED_CAPABILITY_SCHEMA.into(),
        values: BTreeMap::from([(digest, common)]),
    };
    shared.validate()?;
    Ok(Some(shared))
}

/// Materialize precisely one original native capability body. There are no
/// arbitrary JSON pointers, recursive references or cross-namespace fallbacks.
pub fn materialize_combined_member_result(combined: &Value, index: IndexId) -> Result<Value> {
    ensure!(
        combined.get("schema").and_then(Value::as_str) == Some(COMBINED_READ_SCHEMA),
        "shared capability values require the combined read namespace"
    );
    let members = combined
        .get("members")
        .and_then(Value::as_array)
        .context("combined members are missing")?;
    ensure!(
        !members.is_empty() && members.len() <= MAX_SELECTION,
        "invalid combined member count"
    );
    let ids = members
        .iter()
        .map(|member| serde_json::from_value::<IndexId>(member["index_id"].clone()))
        .collect::<Result<Vec<_>, _>>()?;
    unique_bounded(&ids, MAX_SELECTION)?;
    let member = members
        .iter()
        .find(|member| member["index_id"] == Value::String(index.to_string()))
        .context("combined member is not selected")?;
    let result = member
        .get("result")
        .context("combined member has no result")?;
    if combined
        .pointer("/shared_values/schema")
        .and_then(Value::as_str)
        == Some(super::header_templates::HEADER_SCHEMA)
    {
        ensure!(
            result.get(SHARED_CAPABILITY_REFERENCE).is_none(),
            "mixed native shared-value namespaces"
        );
        let table: super::header_templates::SharedHeaderTemplates =
            serde_json::from_value(combined["shared_values"].clone())?;
        let mut materialized = super::header_templates::materialize_header(&table, result)?;
        super::origins::materialize_member_publications(combined, index, &mut materialized)?;
        return Ok(materialized);
    }
    ensure!(
        result
            .get(super::header_templates::HEADER_REFERENCE)
            .is_none(),
        "native header table is missing"
    );
    let reference = result.get(SHARED_CAPABILITY_REFERENCE);
    let shared = combined
        .get("shared_values")
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value::<SharedCapabilityValues>(value.clone()))
        .transpose()?;
    if let Some(shared) = &shared {
        shared.validate()?;
    }
    let Some(reference) = reference else {
        encoded_size(result, crate::MAX_RESPONSE_BYTES)?;
        let mut materialized = result.clone();
        super::origins::materialize_member_publications(combined, index, &mut materialized)?;
        return Ok(materialized);
    };
    let id: Sha256Digest = reference
        .as_str()
        .context("invalid shared capability reference")?
        .parse()?;
    let shared = shared.context("shared capability table is missing")?;
    let fields = shared
        .values
        .get(&id)
        .context("shared capability reference is missing")?;
    let mut materialized = result
        .as_object()
        .context("referenced capability member is not an object")?
        .clone();
    materialized.remove(SHARED_CAPABILITY_REFERENCE);
    ensure!(
        fields.keys().all(|key| !materialized.contains_key(key)),
        "shared capability fields collide with member fields"
    );
    materialized.extend(fields.clone());
    let mut materialized = Value::Object(materialized);
    super::origins::materialize_member_publications(combined, index, &mut materialized)?;
    encoded_size(&materialized, crate::MAX_RESPONSE_BYTES)?;
    Ok(materialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Incarnation;
    use serde_json::json;
    fn index(n: u8) -> IndexId {
        IndexId::from_identity(Incarnation::from_bytes([n; 16]))
    }
    fn pair() -> Vec<Value> {
        vec![
            json!({"workbench":{"tools":["search","symbols"]},"adapters":[{"id":"rust"}],"read_guidance":{"complete":true},
            "request_admission":{"limit":16},"scope_observation":{"fingerprint":"one"},"runtime":{"worker":1},"distinct":{"a":1}}),
            json!({"workbench":{"tools":["search","symbols"]},"adapters":[{"id":"rust"}],"read_guidance":{"complete":true},
            "request_admission":{"limit":16},"scope_observation":{"fingerprint":"two"},"runtime":{"worker":2},"distinct":{"a":2}}),
        ]
    }
    fn combined(results: Vec<Value>, shared: Option<SharedCapabilityValues>) -> Value {
        json!({"schema":COMBINED_READ_SCHEMA,"members":results.into_iter().enumerate().map(|(i,result)|json!({"index_id":index(i as u8+1),"result":result})).collect::<Vec<_>>(),"shared_values":shared})
    }
    #[test]
    fn shared_fields_materialize_exactly_and_observations_remain_member_owned() -> Result<()> {
        let original = pair();
        let mut results = original.clone();
        let shared = share_capability_fields(&mut results.iter_mut().collect::<Vec<_>>())?
            .context("shared fields")?;
        assert_eq!(shared.values.len(), 1);
        for result in &results {
            for key in [
                "request_admission",
                "scope_observation",
                "runtime",
                "distinct",
            ] {
                assert!(result.get(key).is_some());
            }
        }
        let value = combined(results, Some(shared));
        assert_eq!(
            materialize_combined_member_result(&value, index(1))?,
            original[0]
        );
        assert_eq!(
            materialize_combined_member_result(&value, index(2))?,
            original[1]
        );
        Ok(())
    }
    #[test]
    fn malformed_missing_colliding_cyclic_or_foreign_references_are_refused() -> Result<()> {
        let mut results = pair();
        let shared = share_capability_fields(&mut results.iter_mut().collect::<Vec<_>>())?;
        let value = combined(results, shared);
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("shared_values");
        assert!(materialize_combined_member_result(&missing, index(1)).is_err());
        let mut absent = value.clone();
        absent["members"][0]["result"][SHARED_CAPABILITY_REFERENCE] = json!("00".repeat(32));
        assert!(materialize_combined_member_result(&absent, index(1)).is_err());
        let mut collision = value.clone();
        collision["members"][0]["result"]["workbench"] = json!({});
        assert!(materialize_combined_member_result(&collision, index(1)).is_err());
        let mut foreign = value.clone();
        foreign["shared_values"]["schema"] = json!("other/1");
        assert!(materialize_combined_member_result(&foreign, index(1)).is_err());
        let mut digest = value.clone();
        let fields = digest["shared_values"]["values"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        fields["adapters"] = json!([]);
        assert!(materialize_combined_member_result(&digest, index(1)).is_err());
        let mut nested = value.clone();
        let fields = nested["shared_values"]["values"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        fields["workbench"] = json!({SHARED_CAPABILITY_REFERENCE:"00".repeat(32)});
        assert!(materialize_combined_member_result(&nested, index(1)).is_err());
        let mut pointer = value.clone();
        pointer["members"][0]["result"][SHARED_CAPABILITY_REFERENCE] = json!("/members/1/result");
        assert!(materialize_combined_member_result(&pointer, index(1)).is_err());
        Ok(())
    }
    #[test]
    fn unsafe_numeric_or_object_keys_stay_inline_and_single_member_is_lossless() -> Result<()> {
        let original = json!({"adapters":[{"id":"rust"}],"runtime":{"worker":1},"numeric":{"threshold":0.5},
            "large":{"n":9007199254740992_u64},"unsafe":{"constructor":"native data"}});
        let mut results = vec![original.clone()];
        let shared = share_capability_fields(&mut results.iter_mut().collect::<Vec<_>>())?;
        assert!(results[0].get("numeric").is_some());
        assert!(results[0].get("large").is_some());
        assert!(results[0].get("unsafe").is_some());
        assert_eq!(
            materialize_combined_member_result(&combined(results, shared), index(1))?,
            original
        );
        Ok(())
    }
    #[test]
    fn canonical_group_digest_matches_host_golden_for_integer_index_keys() -> Result<()> {
        let mut results = [json!({"workbench":{"10":"a","2":"b"}})];
        let shared = share_capability_fields(&mut results.iter_mut().collect::<Vec<_>>())?
            .context("group")?;
        assert_eq!(
            shared.values.keys().next().unwrap().to_string(),
            "ba2e0fd072e154161d7a53c77f61ea42c8dc4f8d9f6f4beaf9f7dc0a9824e5e0"
        );
        Ok(())
    }
}
