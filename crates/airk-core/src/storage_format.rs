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
}

pub const CURRENT: StorageFormats = StorageFormats {
    index: 1,
    service_state: 1,
    logs: 1,
};

#[cfg(test)]
mod tests {
    #[test]
    fn format_identifiers_are_independent_of_release_version() {
        let value = serde_json::to_value(super::CURRENT).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"index":1,"service_state":1,"logs":1})
        );
        assert_eq!(
            serde_json::from_value::<super::StorageFormats>(value).unwrap(),
            super::CURRENT
        );
    }
}
