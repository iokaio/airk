// SPDX-License-Identifier: Apache-2.0
//! The reviewed operation table (contracts §4.3): read from the checked-in
//! fixture, never hand-copied, and validated once with
//! `admin_contract::validate_table` before any row is served.
use airk_endpoint::admin_contract::{NeverOverHttp, OperationRow, Surface, validate_table};
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

/// How a reviewed row is served when its `method` is not itself a
/// `ControlMethod` variant (contracts §4.3 amendment, Phase 5b). Reviewed
/// here and recorded in `CONTRACT.md`; a row not listed is served by its own
/// `method`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Served {
    /// The row's `method` is the `ControlMethod` (or `Variant.Action`).
    Direct,
    /// The implemented `ControlMethod` with the same result; the request body
    /// is that method's JSON.
    Equivalent(&'static str),
    /// `https.trust_status`: the read-only trust inspection `airk runtime
    /// http-trust --action inspect` ran, through the same callers, in the
    /// service. Its body is exactly `{"http_trust":{"action":"inspect"}}`.
    TrustInspect,
}

/// The reviewed mapping of table methods that name no `ControlMethod`.
pub(super) const MAPPINGS: &[(&str, Served)] = &[
    (
        "HttpConnectionInfo",
        Served::Equivalent("AgentConnectionInfo"),
    ),
    (
        "HttpConnectionOwnership",
        Served::Equivalent("AgentConnectionOwnership"),
    ),
    ("LogsExport", Served::Equivalent("Logs")),
    ("HttpTrust.Inspect", Served::TrustInspect),
];

/// How the row whose table `method` is `method` is served.
pub(super) fn served(method: &str) -> Served {
    MAPPINGS
        .iter()
        .find(|(name, _)| *name == method)
        .map_or(Served::Direct, |(_, served)| *served)
}

/// `ControlMethod` variant names are serde `snake_case` of the Rust names.
pub(super) fn snake(name: &str) -> String {
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
                if variant == "arbiter" {
                    inner
                        .get("request")
                        .and_then(|request| request.get("operation"))
                        .and_then(Value::as_str)
                        == Some(snake(action).as_str())
                } else {
                    inner.get("action").and_then(Value::as_str) == Some(snake(action).as_str())
                }
            }
            (None, _) => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod arbiter_tests {
    use super::*;
    #[test]
    fn arbiter_rows_match_only_the_typed_nested_management_operation() {
        for action in [
            "Status",
            "Configure",
            "Provision",
            "Cancel",
            "Remove",
            "Recover",
        ] {
            let body = serde_json::json!({"arbiter":{"request":{"operation":snake(action)}}});
            assert!(names(&format!("Arbiter.{action}"), &body));
            for other in [
                "Status",
                "Configure",
                "Provision",
                "Cancel",
                "Remove",
                "Recover",
            ] {
                assert_eq!(names(&format!("Arbiter.{other}"), &body), action == other);
            }
        }
        assert!(!names(
            "Arbiter.Status",
            &serde_json::json!({"arbiter":{"action":"status"}})
        ));
        assert!(!names(
            "Arbiter.Status",
            &serde_json::json!({"arbiter_next_read":{}})
        ));
        let table = table().unwrap();
        assert!(table.never_over_http(&serde_json::json!({"arbiter_next_read":{}})));
        assert!(table.never_over_http(&serde_json::json!({"arbiter_application":{}})));
    }
}
