// SPDX-License-Identifier: Apache-2.0
//! JSON Schema for the I2 contract (split plan U47 generation pilot).
//!
//! `document()` renders the schema of the I2 root types. The checked-in copy,
//! `packages/vscode/src/generated/i2.schema.json`, is pinned by the
//! `contract_schema` test; `scripts/dev/contract_types.py` renders the
//! TypeScript types from it. Identity formats and opaque I1 values are written
//! here by hand: rules inside `airk-core` newtypes are invisible to generators
//! (contract-generation pilot), and I1 stays fixtures-only (D5).
use super::{
    ErrorLine, ExitClass, ServiceStatus, SessionEventFrame, SessionRequest, SessionResponse,
    VersionDocument,
};
use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings, json_schema};

/// A canonical 16-byte identity: 32 lowercase hexadecimal characters.
pub(super) fn incarnation(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": "^[0-9a-f]{32}$"})
}

/// An optional identity; absent or null when unknown.
pub(super) fn optional_incarnation(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [{"type": "string", "pattern": "^[0-9a-f]{32}$"}, {"type": "null"}]})
}

/// An I1 value carried unchanged (fixtures-only under D5; not generated).
pub(super) fn native_value(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"description": "An I1 control-protocol value, carried unchanged; see fixtures/wire."})
}

fn root<T: JsonSchema>(generator: &mut SchemaGenerator, roots: &mut Vec<String>) {
    generator.subschema_for::<T>();
    roots.push(T::schema_name().into_owned());
}

/// The I2 schema document, with the root types listed in a fixed order.
pub fn document() -> serde_json::Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    let mut roots = Vec::new();
    root::<VersionDocument>(&mut generator, &mut roots);
    root::<ErrorLine>(&mut generator, &mut roots);
    root::<ExitClass>(&mut generator, &mut roots);
    root::<SessionRequest>(&mut generator, &mut roots);
    root::<SessionResponse>(&mut generator, &mut roots);
    root::<SessionEventFrame>(&mut generator, &mut roots);
    root::<ServiceStatus>(&mut generator, &mut roots);
    root::<super::SetupReceiptObservation>(&mut generator, &mut roots);
    root::<super::Entry>(&mut generator, &mut roots);
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = generator
        .definitions()
        .iter()
        .map(|(name, schema)| (name.clone(), schema.clone()))
        .collect();
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "airkctl I2 contract",
        "i2_contract": super::I2_CONTRACT,
        "roots": roots,
        "$defs": definitions,
    })
}
