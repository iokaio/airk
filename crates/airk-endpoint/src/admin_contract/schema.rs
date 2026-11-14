// SPDX-License-Identifier: Apache-2.0
//! JSON Schema for the I3 contract (split plan U47 generation pilot).
//!
//! `document()` renders the schema of the types the browser adapter reads and
//! sends. The checked-in copy, `packages/contracts-gen/i3.schema.json`, is
//! pinned by the `admin_contract_schema` test; `scripts/dev/contract_types.py
//! i3` renders the TypeScript types from it. Formats of identities and
//! secrets, and constant fields, are written here by hand. Admission inputs
//! and outcomes are server-side only and are not exported.
use super::{
    AcknowledgedMutationCall, AdminCode, Envelope, EventStreamRequest, ExchangeRequest,
    ExchangeResponse, MutationPreparation, OperationRow, PreparedMutationCall, RecoverMutationCall,
    ReviewedCall, StreamLine,
};
use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings, json_schema};

/// A canonical 16-byte identity: 32 lowercase hexadecimal characters.
pub(super) fn incarnation(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": "^[0-9a-f]{32}$"})
}

/// A 256-bit secret as canonical unpadded base64url.
pub(super) fn secret(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "pattern": "^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$"})
}

/// An optional canonical 16-byte identity.
pub(super) fn incarnation_option(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": ["string", "null"], "pattern": "^[0-9a-f]{32}$"})
}

pub(super) fn envelope_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "const": "airk-admin/1"})
}

pub(super) fn constant_true(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"const": true})
}

pub(super) fn constant_false(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"const": false})
}

fn root<T: JsonSchema>(generator: &mut SchemaGenerator, roots: &mut Vec<String>) {
    generator.subschema_for::<T>();
    roots.push(T::schema_name().into_owned());
}

/// The I3 schema document, with the root types listed in a fixed order.
pub fn document() -> serde_json::Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    let mut roots = Vec::new();
    root::<Envelope>(&mut generator, &mut roots);
    root::<AdminCode>(&mut generator, &mut roots);
    root::<ExchangeRequest>(&mut generator, &mut roots);
    root::<ExchangeResponse>(&mut generator, &mut roots);
    root::<StreamLine>(&mut generator, &mut roots);
    root::<EventStreamRequest>(&mut generator, &mut roots);
    root::<OperationRow>(&mut generator, &mut roots);
    root::<ReviewedCall>(&mut generator, &mut roots);
    root::<MutationPreparation>(&mut generator, &mut roots);
    root::<PreparedMutationCall>(&mut generator, &mut roots);
    root::<RecoverMutationCall>(&mut generator, &mut roots);
    root::<AcknowledgedMutationCall>(&mut generator, &mut roots);
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = generator
        .definitions()
        .iter()
        .map(|(name, schema)| (name.clone(), schema.clone()))
        .collect();
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "airk admin I3 contract",
        "i3_contract": super::I3_CONTRACT,
        "roots": roots,
        "$defs": definitions,
    })
}
