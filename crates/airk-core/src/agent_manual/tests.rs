// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn manual_entries_pin_all_clients_paths_formats_and_current_port() {
    let entries = project_entries(LoopbackMcpUrl::new(57429).unwrap());
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].file, ".mcp.json");
    assert_eq!(entries[1].file, ".codex/config.toml");
    assert_eq!(entries[2].file, ".vscode/mcp.json");
    assert_eq!(entries[1].format, ManualFormat::Toml);
    assert_eq!(
        entries[1].text,
        "[mcp_servers.airk]\nurl = \"https://127.0.0.1:57429/mcp\"\n\n[mcp_servers.airk.http_headers]\n\"X-AIRK-Response-Mode\" = \"terse\"\n"
    );
    for (index, container) in [(0, "mcpServers"), (2, "servers")] {
        assert_eq!(entries[index].format, ManualFormat::Json);
        let value: serde_json::Value = serde_json::from_str(&entries[index].text).unwrap();
        assert_eq!(
            value,
            serde_json::json!({container: {"airk": {"type":"http", "url":"https://127.0.0.1:57429/mcp", "headers":{"X-AIRK-Response-Mode":"terse"}}}})
        );
    }
}

#[test]
fn manual_text_escapes_keys_and_preserves_exact_selected_target() {
    let manual = entry(
        ManualClient::Codex,
        ManualScope::User,
        "C:\\Users\\a b\\.codex\\config.toml".into(),
        "airk-\"scope\\x",
        LoopbackMcpUrl::new(65535).unwrap(),
    )
    .unwrap();
    assert_eq!(manual.file, "C:\\Users\\a b\\.codex\\config.toml");
    assert_eq!(
        manual.text,
        "[mcp_servers.\"airk-\\\"scope\\\\x\"]\nurl = \"https://127.0.0.1:65535/mcp\"\n\n[mcp_servers.\"airk-\\\"scope\\\\x\".http_headers]\n\"X-AIRK-Response-Mode\" = \"terse\"\n"
    );
    for key in ["foreign", "airk-\n", "airk_\0"] {
        assert!(
            entry(
                ManualClient::Claude,
                ManualScope::Project,
                ".mcp.json".into(),
                key,
                LoopbackMcpUrl::new(1).unwrap()
            )
            .is_err()
        );
    }
}

#[test]
fn emit_agent_manual_semantic_cases() {
    let Some(output) = std::env::var_os("AIRK_AGENT_MANUAL_CASES_OUTPUT") else {
        return;
    };
    let mut rows = Vec::new();
    for (client, file) in [
        (ManualClient::Claude, ".mcp.json"),
        (ManualClient::Codex, ".codex/config.toml"),
        (ManualClient::Vscode, ".vscode/mcp.json"),
    ] {
        for (port, key) in [(57421, "airk"), (57429, "airk-\"scope\\x")] {
            rows.push(
                entry(
                    client,
                    ManualScope::Project,
                    file.into(),
                    key,
                    LoopbackMcpUrl::new(port).unwrap(),
                )
                .unwrap(),
            );
        }
    }
    let text = serde_json::to_string_pretty(
        &serde_json::json!({"schema":"airk.agent-manual-cases/1", "entries":rows}),
    )
    .unwrap()
        + "\n";
    std::fs::write(output, text).unwrap();
}
