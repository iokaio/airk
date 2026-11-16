# Manual agent configuration projection

Owner: `airk-core::agent_manual`. Final-plan D04 records this additive public
presentation contract before its consumers. The module emits inert data and
never reads configuration, filesystem state, credentials or write authority.

`AgentManualEntry` contains client, scope, file, format and exact text. The
supported clients are Claude Code, Codex and VS Code/GitHub Copilot. The CLI
resolves the selected project/user file; endpoint reads supply project-relative
targets. JSON/TOML string escaping and the current canonical loopback URL are
native Rust responsibilities. Entries carry only URL/type and the terse response
header; no bearer, token helper, scope default or credential is emitted.

Manual text is complete entry text to merge while preserving unrelated entries.
It grants no permission to replace an existing file. A port change requires
new text. Invalid URLs, non-AIRK keys, control characters and unbounded targets
refuse. `AgentConfigRefusal` preserves typed outcome reason, complete detail,
target file and optional usable manual entry without parsing display strings.
`conflict_retained` and `outcome_unknown` retain their nonreplay semantics.

I2/I3 schema roots reference these same Rust types. Native semantic emission is
`emit_agent_manual_semantic_cases` with `AIRK_AGENT_MANUAL_CASES_OUTPUT`; literal
shape/escaping tests and native writer comparison independently pin semantics.
`AgentEndpoint.manual` is a runtime projection outside the unchanged persisted
endpoint descriptor/digest. No state schema or digest input changes.
