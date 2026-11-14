// SPDX-License-Identifier: Apache-2.0
//! Schema derives from the actual Core contract, using the approved pilot.
use super::*;
use crate::identity::RevisionToken;
use schemars::{Schema, SchemaGenerator, generate::SchemaSettings, json_schema};
use std::borrow::Cow;

macro_rules! identity_schema {
    ($type:ty, $name:literal, $pattern:literal) => {
        impl JsonSchema for $type {
            fn schema_name() -> Cow<'static, str> { Cow::Borrowed($name) }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({"type":"string","pattern":$pattern})
            }
        }
    };
}
identity_schema!(Incarnation, "Incarnation", "^[0-9a-f]{32}$");
identity_schema!(Sha256Digest, "Sha256Digest", "^[0-9a-f]{64}$");
identity_schema!(IndexId, "IndexId", "^idx_[0-9a-f]{32}$");
identity_schema!(
    RevisionToken,
    "RevisionToken",
    "^airk-r1:[0-9a-f]{32}:(0|[1-9][0-9]{0,19}):[0-9a-f]{64}$"
);

fn root<T: JsonSchema>(generator: &mut SchemaGenerator, roots: &mut Vec<String>) {
    generator.subschema_for::<T>();
    roots.push(T::schema_name().into_owned());
}
pub fn document() -> serde_json::Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    let mut roots = Vec::new();
    root::<Configuration>(&mut generator, &mut roots);
    root::<Profile>(&mut generator, &mut roots);
    root::<CompilerProfile>(&mut generator, &mut roots);
    root::<ContextRequest>(&mut generator, &mut roots);
    root::<ContextResponse>(&mut generator, &mut roots);
    root::<Advice>(&mut generator, &mut roots);
    root::<ManagementRequest>(&mut generator, &mut roots);
    root::<Status>(&mut generator, &mut roots);
    root::<context_action::ContextRequestV2>(&mut generator, &mut roots);
    root::<context_action::ContextResponseV2>(&mut generator, &mut roots);
    root::<context_action::ContextActionRoster>(&mut generator, &mut roots);
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = generator
        .definitions()
        .iter()
        .map(|(name, schema)| (name.clone(), schema.clone()))
        .collect();
    serde_json::json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "title":"AIRK Arbiter Core contract","arbiter_contract":1,
        "roots":roots,"$defs":definitions,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn schema_matches_reviewed_capture() {
        let actual = super::document();
        let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/contracts-gen/arbiter.schema.json");
        let captured = std::fs::read(&expected)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        if captured.as_ref() != Some(&actual) {
            let observed = std::env::temp_dir().join("airk-arbiter.schema.observed.json");
            std::fs::write(&observed, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
            panic!(
                "review native schema capture at {} before replacing {}",
                observed.display(),
                expected.display()
            );
        }
    }
}
