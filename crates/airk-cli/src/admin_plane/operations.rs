// SPDX-License-Identifier: Apache-2.0
//! The reviewed operation table (contracts §4.3): read from the checked-in
//! fixture, never hand-copied, and validated once with
//! `admin_contract::validate_table` before any row is served.
use crate::admin_contract::{NeverOverHttp, OperationRow, Surface, validate_table};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

const TABLE: &str = include_str!("../../../../fixtures/split/i3/admin_operations.json");

/// The fixture's served parts; its other fields (enumerations, invalid
/// tables) are review material.
#[derive(Deserialize)]
pub(super) struct Table {
    pub(super) operations: Vec<OperationRow>,
    pub(super) never_over_http: Vec<NeverOverHttp>,
}

/// The validated table, or the first rule it breaks (every call is then
/// refused as internal).
pub(super) fn table() -> Result<&'static Table, &'static str> {
    static LOADED: OnceLock<Result<Table, &'static str>> = OnceLock::new();
    LOADED
        .get_or_init(|| {
            let table: Table =
                serde_json::from_str(TABLE).map_err(|_| "operation_table_unreadable")?;
            validate_table(&table.operations, &table.never_over_http)?;
            Ok(table)
        })
        .as_ref()
        .map_err(|error| *error)
}

impl Table {
    /// The row an admin `call` may serve: the operation exists and lists the
    /// `admin` surface.
    pub(super) fn admin_row(&self, operation: &str) -> Option<&OperationRow> {
        self.operations
            .iter()
            .find(|row| row.operation == operation && row.surfaces.contains(&Surface::Admin))
    }

    /// Whether an encoded method is one that never has an HTTP route.
    pub(super) fn never_over_http(&self, encoded: &Value) -> bool {
        self.never_over_http
            .iter()
            .flat_map(|item| item.methods.iter())
            .any(|method| names(method, encoded))
    }
}

/// `ControlMethod` variant names are serde `snake_case` of the Rust names.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// Whether the canonical JSON of a decoded `ControlMethod` is the table's
/// `method`: a variant (`Health`), or one action of it
/// (`HttpIssuerRotation.Status`, matched on its `action` field).
pub(super) fn names(method: &str, encoded: &Value) -> bool {
    let (variant, action) = match method.split_once('.') {
        Some((variant, action)) => (variant, Some(action)),
        None => (method, None),
    };
    let variant = snake(variant);
    match encoded {
        Value::String(name) => action.is_none() && *name == variant,
        Value::Object(object) if object.len() == 1 => match (object.get(&variant), action) {
            (Some(_), None) => true,
            (Some(inner), Some(action)) => {
                inner.get("action").and_then(Value::as_str) == Some(snake(action).as_str())
            }
            (None, _) => false,
        },
        _ => false,
    }
}
