// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service::{MAX_FRAME_BYTES, MAX_TOOLS_CATALOG_RESPONSE_BYTES};
use serde_json::Value;

fn document(detail: ToolsCatalogDetail) -> String {
    match response(&detail).unwrap() {
        ControlResponse::ToolsCatalog { document } => document,
        _ => panic!("expected a catalog document"),
    }
}

#[test]
fn the_full_catalog_needs_the_m2_budget_and_keeps_headroom() {
    let response = response(&ToolsCatalogDetail::List { schemas: true }).unwrap();
    let frame = serde_json::to_vec(&response).unwrap().len();
    assert!(frame > MAX_FRAME_BYTES, "{frame}");
    assert!(frame <= MAX_TOOLS_CATALOG_RESPONSE_BYTES / 2, "{frame}");
}

#[test]
fn the_names_only_list_has_every_tool_with_two_fields() {
    let full: Value =
        serde_json::from_str(&document(ToolsCatalogDetail::List { schemas: true })).unwrap();
    let names: Value =
        serde_json::from_str(&document(ToolsCatalogDetail::List { schemas: false })).unwrap();
    let full = full["tools"].as_array().unwrap();
    let names = names["tools"].as_array().unwrap();
    assert!(!names.is_empty());
    assert_eq!(names.len(), full.len());
    for (short, complete) in names.iter().zip(full) {
        let keys: Vec<&String> = short.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["description", "name"]);
        assert_eq!(short["name"], complete["name"]);
        assert_eq!(short["description"], complete["description"]);
    }
}

#[test]
fn help_accepts_the_prefixed_name_and_keeps_field_order() {
    let plain = document(ToolsCatalogDetail::Help {
        tool: "search".into(),
    });
    let prefixed = document(ToolsCatalogDetail::Help {
        tool: "airk_search".into(),
    });
    assert_eq!(plain, prefixed);
    assert!(plain.starts_with("{\n  \"name\": \"search\",\n"), "{plain}");
}

#[test]
fn an_unknown_tool_is_a_typed_failure() {
    for name in ["airk_", "airk_missing", "Search", ""] {
        assert!(matches!(
            response(&ToolsCatalogDetail::Help { tool: name.into() }).unwrap(),
            ControlResponse::Failure { code } if code == "unknown_tool"
        ));
    }
}
