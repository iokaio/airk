# Evidence guidance (removed 27 November 2026)

Native read results no longer carry a `guidance` block, and `airk_capabilities`
no longer lists `read_guidance`. The user decided on 27 November 2026 that an
agent gains confidence in a reply from its content and the references to
specific files, and that no editorial is allowed in what it receives. The
observations this block added (reasons, explanatory messages and a suggested
next read) were editorial over facts the reply already carries: truncation,
continuation cursors, readiness, coverage, failures and selection policy remain
in their own fields, unchanged.

The removed owner was `airk-core::guidance`. The decision is recorded in the
[MCP response contract](../crates/airk-mcp/src/response/CONTRACT.md#no-editorial-in-replies-user-decision-27-november-2026)
and the [Answer loop change log](arbiter/answer-loop-changes.md). Earlier
behavior is in the Git history of this page.