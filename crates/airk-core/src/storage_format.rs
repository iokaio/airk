// SPDX-License-Identifier: Apache-2.0
//! Storage compatibility identifiers are independent of executable versions.
use serde::{Deserialize, Serialize};

/// These identifiers describe compatibility families, not application releases
/// or individual SQL migration numbers. Existing format-140 service indexes
/// belong to index-v1; their historical filenames remain compatible aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageFormats {
    pub index: u32,
    pub service_state: u32,
    pub logs: u32,
    /// Binary-supported optional JSON record capability, not proof that a selected
    /// legacy index has projected records or private-root eligibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_projection_records: Option<u32>,
}

pub const CURRENT: StorageFormats = StorageFormats {
    index: 1,
    service_state: 1,
    logs: 1,
    query_projection_records: Some(NATIVE_QUERY_PROJECTION_VERSION),
};

#[cfg(test)]
mod tests {
    #[test]
    fn format_identifiers_are_independent_of_release_version() {
        let value = serde_json::to_value(super::CURRENT).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"index":1,"service_state":1,"logs":1,"query_projection_records":4})
        );
        assert_eq!(
            serde_json::from_value::<super::StorageFormats>(value).unwrap(),
            super::CURRENT
        );
    }
    #[test]
    fn legacy_format_report_does_not_claim_optional_projection_support() {
        let legacy: super::StorageFormats =
            serde_json::from_value(serde_json::json!({"index":1,"service_state":1,"logs":1}))
                .unwrap();
        assert_eq!(legacy.query_projection_records, None);
        assert_eq!(
            serde_json::to_value(legacy).unwrap(),
            serde_json::json!({"index":1,"service_state":1,"logs":1})
        );
    }
}

/// Optional current-owner JSON record capability; independent of index-v1 layout,
/// SQLite user_version and executable version. Unknown capabilities refuse access.
pub const NATIVE_QUERY_PROJECTION_VERSION: u32 = 4;
pub const NATIVE_QUERY_PROJECTION_NAME: &str = "native-query-projections-v4";
