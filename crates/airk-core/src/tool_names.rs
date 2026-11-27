// SPDX-License-Identifier: Apache-2.0
//! Public MCP tool names and the operation names they serve.
//!
//! Three spellings name one operation: the native tool (`airk_search`), the
//! operation (`search`, used by Parse plan steps, Workbench recipes and
//! routing), and the public MCP tool (`search_passages`). The user renamed
//! nine public tools on 26 November 2026 (completion plan decision 12) as a
//! clean rename at 1.9: the old public spelling of a renamed tool is no
//! longer a public tool name. Native names, operation names and CLI commands
//! are unchanged. This table is the single owner of that mapping.

/// `(operation, public)` for every operation whose public MCP name differs
/// from its operation name.
pub const RENAMED_PUBLIC_TOOLS: [(&str, &str); 9] = [
    ("answer", "gather_context"),
    ("search", "search_passages"),
    ("symbols", "find_definition"),
    ("references", "find_identifier_occurrences"),
    ("read_file", "read_indexed_range"),
    ("context", "read_around"),
    ("outline", "outline_file"),
    ("list_files", "list_indexed_files"),
    ("status", "index_status"),
];

/// The public tools an MCP client sees and may call (user decision, 27
/// November 2026): the core retrieval tools only. Every other operation
/// stays in the CLI, Admin and Workbench; an MCP call to it is refused as
/// an unknown tool, under its public or its native name.
pub const MCP_TOOLS: [&str; 7] = [
    "gather_context",
    "search_passages",
    "find_definition",
    "find_identifier_occurrences",
    "read_indexed_range",
    "outline_file",
    "list_indexed_files",
];

/// Whether an MCP client may see and call `name`: one of [`MCP_TOOLS`], or
/// the native `airk_*` name of one. An old public name selects nothing.
pub fn on_mcp(name: &str) -> bool {
    MCP_TOOLS.contains(&name)
        || public_name_of_native(name).is_some_and(|public| MCP_TOOLS.contains(&public))
}

/// The operation a native, public or operation spelling names. A renamed
/// public name maps to its operation; a native name loses one `airk_`
/// prefix; anything else is returned unchanged. Routing uses this to treat
/// the three spellings of one operation alike. It is not a catalog check.
pub fn operation(name: &str) -> &str {
    if let Some((operation, _)) = RENAMED_PUBLIC_TOOLS
        .iter()
        .find(|(_, public)| *public == name)
    {
        return operation;
    }
    name.strip_prefix("airk_").unwrap_or(name)
}

/// The public MCP name of an operation (`search` -> `search_passages`).
pub fn public_name(operation: &str) -> &str {
    RENAMED_PUBLIC_TOOLS
        .iter()
        .find(|(old, _)| *old == operation)
        .map_or(operation, |(_, public)| public)
}

/// The public MCP name of a native `airk_*` tool, or `None` when `native`
/// is not one native spelling.
pub fn public_name_of_native(native: &str) -> Option<&str> {
    native
        .strip_prefix("airk_")
        .filter(|operation| !operation.starts_with("airk_"))
        .map(public_name)
}

/// The native tool a public MCP name selects. The old public spelling of a
/// renamed tool (`search`) selects nothing. Callers still check the catalog.
pub fn native_name_of_public(public: &str) -> Option<String> {
    if public.starts_with("airk_") || RENAMED_PUBLIC_TOOLS.iter().any(|(old, _)| *old == public) {
        return None;
    }
    Some(format!("airk_{}", operation(public)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tooling outside Rust (the evaluation harness) reads this emitted table
    /// instead of keeping its own copy. Regenerate with
    /// `AIRK_WRITE_PUBLIC_TOOL_NAMES=1 cargo test -p airk-core --lib tool_names`.
    #[test]
    fn emitted_public_tool_names_match_the_rust_owner() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/split/catalog/public_tool_names.json");
        let renamed: Vec<_> = RENAMED_PUBLIC_TOOLS
            .iter()
            .map(|(operation, public)| {
                serde_json::json!({
                    "native": format!("airk_{operation}"),
                    "operation": operation,
                    "public": public,
                })
            })
            .collect();
        let value = serde_json::json!({
            "schema": "airk.public-tool-names/1",
            "decision": "completion plan decision 12, 26 November 2026: clean rename at 1.9",
            "owner": "airk-core::tool_names::RENAMED_PUBLIC_TOOLS",
            "renamed": renamed,
            "mcp_tools": MCP_TOOLS,
            "mcp_decision": "27 November 2026: MCP advertises and accepts only seven core retrieval tools; find_references renamed find_identifier_occurrences",
        });
        let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
        if std::env::var_os("AIRK_WRITE_PUBLIC_TOOL_NAMES").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            std::fs::write(&path, &text).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n"),
            text,
            "regenerate with AIRK_WRITE_PUBLIC_TOOL_NAMES=1"
        );
    }

    #[test]
    fn the_nine_public_renames_map_both_ways_and_old_public_names_select_nothing() {
        // Literal expectations from the user's decision of 26 November 2026.
        for (native, public) in [
            ("airk_answer", "gather_context"),
            ("airk_search", "search_passages"),
            ("airk_symbols", "find_definition"),
            ("airk_references", "find_identifier_occurrences"),
            ("airk_read_file", "read_indexed_range"),
            ("airk_context", "read_around"),
            ("airk_outline", "outline_file"),
            ("airk_list_files", "list_indexed_files"),
            ("airk_status", "index_status"),
            ("airk_file_info", "file_info"),
            ("airk_code_facts", "code_facts"),
            ("airk_parse", "parse"),
        ] {
            assert_eq!(public_name_of_native(native), Some(public));
            assert_eq!(native_name_of_public(public).as_deref(), Some(native));
            assert_eq!(operation(public), &native[5..]);
            assert_eq!(operation(native), &native[5..]);
        }
        for old in [
            "answer",
            "search",
            "symbols",
            "references",
            "read_file",
            "context",
            "outline",
            "list_files",
            "status",
        ] {
            assert_eq!(native_name_of_public(old), None, "{old}");
            assert_eq!(operation(old), old);
        }
        assert_eq!(native_name_of_public("airk_search"), None);
        assert_eq!(public_name_of_native("airk_airk_search"), None);
        assert_eq!(public_name_of_native("search"), None);
        // The 26 November public name, renamed on 27 November, names no operation.
        assert_eq!(operation("find_references"), "find_references");
    }

    #[test]
    fn mcp_offers_exactly_the_seven_core_retrieval_tools_under_either_spelling() {
        // Literal list from the user's decision of 27 November 2026.
        let kept = [
            ("gather_context", "airk_answer"),
            ("search_passages", "airk_search"),
            ("find_definition", "airk_symbols"),
            ("find_identifier_occurrences", "airk_references"),
            ("read_indexed_range", "airk_read_file"),
            ("outline_file", "airk_outline"),
            ("list_indexed_files", "airk_list_files"),
        ];
        for (public, native) in kept {
            assert!(on_mcp(public), "{public}");
            assert!(on_mcp(native), "{native}");
        }
        for gone in [
            "capabilities", "changes", "code_facts", "relationships", "imports",
            "document_facts", "parse", "query_scope", "projects", "help", "diagnostics",
            "draft_search", "draft_strategy", "draft_plan", "list_templates", "get_template",
            "validate_draft", "proposal_status", "get_handoff", "suggest_view",
            "airk_capabilities", "airk_changes", "airk_code_facts", "airk_relationships",
            "airk_imports", "airk_document_facts", "airk_parse", "airk_query_scope",
            "airk_help", "airk_diagnostics", "airk_explorer_suggest_search",
            "find_references", "search", "references", "status",
            // Removed the same day: the CLI keeps context, file_info and status.
            "read_around", "file_info", "index_status", "airk_context", "airk_file_info",
            "airk_status",
        ] {
            assert!(!on_mcp(gone), "{gone}");
        }
    }
}
