// SPDX-License-Identifier: Apache-2.0
//! Per-read status that varies between two runs of the same read at the same
//! publication (change-log entry 54). A retained record's identity, its
//! `full_bytes`, `full_digest` and retention accounting bind the read's
//! result without these fields; custody keeps the result as received.
//!
//! The field lists are Core's (`VOLATILE_READ_FIELDS`, `READ_STATUS_FIELDS`).
//! A field that differs between two runs of one read must be classified
//! there; the MCP test `answer_reads_vary_only_in_classified_status_fields`
//! fails until it is.
use airk_core::arbiter::answer_loop::{READ_STATUS_FIELDS as STATUS_FIELDS, VOLATILE_READ_FIELDS};
use serde_json::{Map, Value};

/// The result without its volatile status: what a record's identity binds.
/// A failure receipt (an MCP tool result) is stabilized in its structured
/// content as well; anything that is not an object is returned unchanged.
pub(crate) fn stable(received: &Value) -> Value {
    let mut body = received.clone();
    strip(&mut body);
    if let Some(structured) = body.get_mut("structuredContent") {
        strip(structured);
    }
    body
}

fn strip(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        for field in VOLATILE_READ_FIELDS {
            object.remove(field);
        }
    }
}

/// The status a received result carried, for the reply's one `read_status`.
pub(crate) fn status(received: &Value) -> Option<Map<String, Value>> {
    let object = received.as_object()?;
    let status: Map<String, Value> = STATUS_FIELDS
        .iter()
        .filter_map(|field| {
            object
                .get(*field)
                .map(|value| ((*field).to_owned(), value.clone()))
        })
        .collect();
    (!status.is_empty()).then_some(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stable_removes_exactly_the_classified_status() {
        let received = json!({"publication":{"token":"t"},"hits":[{"text":"x"}],
            "request_id":"r","freshness":{"observed_unix_ms":1},"discovery_readiness":{},
            "semantic_readiness":{},"discovery_observation":"unavailable_after_read",
            "indexing":{},"read":{"request_id":"kept: not top level"}});
        assert_eq!(
            stable(&received),
            json!({"publication":{"token":"t"},"hits":[{"text":"x"}],
                "read":{"request_id":"kept: not top level"}})
        );
        let receipt =
            json!({"isError":true,"structuredContent":{"error":{"code":"x"},"request_id":"r"}});
        assert_eq!(
            stable(&receipt),
            json!({"isError":true,"structuredContent":{"error":{"code":"x"}}})
        );
        assert_eq!(stable(&json!("text")), json!("text"));
        let status = status(&received).unwrap();
        assert_eq!(
            status
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            [
                "discovery_observation",
                "discovery_readiness",
                "freshness",
                "indexing",
                "semantic_readiness"
            ]
        );
        assert_eq!(super::status(&json!({"hits":[]})), None);
    }
}
