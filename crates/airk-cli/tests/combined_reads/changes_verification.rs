// SPDX-License-Identifier: Apache-2.0
//! Current remembered evidence must retain exact native saved-source authority.
use super::*;

/// U110: the HTTPS MCP form of a combined read naming its member index:
/// `project_ids` and `member_availability` removed (an open read has no
/// project narrowing, and a combined argument would send a one-index open
/// read to the combined backend, which refuses it) and its `index_ids` left
/// in place, where the tokenless MCP session takes them as the call's scope.
/// Every other argument is unchanged, so a one-index read takes the
/// single-index backend.
pub(super) fn open_arguments(arguments: &Value) -> Result<Value> {
    let mut open = arguments.clone();
    let fields = open.as_object_mut().context("object arguments required")?;
    fields.remove("project_ids");
    fields.remove("member_availability");
    Ok(open)
}

/// The same open read through the pipe (M3 `ToolSelection::Open`, the path
/// `airkctl tools call --index` takes): the MCP arguments' `index_ids` become
/// the selection and every other argument is sent unchanged.
pub(super) async fn open_pipe(f: &Fixture, tool: &str, mcp_arguments: &Value) -> Result<Value> {
    let mut arguments = mcp_arguments.clone();
    let named = arguments
        .as_object_mut()
        .context("object arguments required")?
        .remove("index_ids")
        .context("open read names no index scope")?;
    let ids: Vec<String> = serde_json::from_value(named)?;
    pipe_read(
        f,
        http::Scope::indexes(ids).open_selection(),
        tool,
        &arguments,
    )
    .await?
    .map_err(|refused| anyhow::anyhow!("open pipe read of {tool} refused: {refused}"))
}

/// U110: one read through the tokenless HTTPS MCP session and the same open
/// read through the pipe, compared in full. The MCP arguments are
/// [`open_arguments`]; with no `index_ids` of their own the session's scope
/// names the indexes on both sides. Returns the MCP JSON-RPC reply.
#[cfg(feature = "local-inference")]
pub(super) async fn mcp_matches_open_pipe(
    f: &Fixture,
    mcp: &mut McpSession,
    id: u64,
    tool: &str,
    arguments: &Value,
    label: &str,
) -> Result<Value> {
    let tool = airk_core::tool_names::public_name_of_native(tool).unwrap_or(tool);
    let open = open_arguments(arguments)?;
    let wire = mcp.call(id, tool, open.clone())?;
    ensure!(
        wire.get("error").is_none() && wire["result"]["isError"] != true,
        "{label}: MCP read failed: {wire}"
    );
    let piped = if open.get("index_ids").is_some() {
        open_pipe(f, tool, &open).await?
    } else {
        mcp.pipe(f, tool, &open).await?
    };
    assert_parity(label, &body(&piped)?, &body(&wire["result"])?)?;
    Ok(wire)
}

#[tokio::test]
async fn current_changes_source_receipts_preserve_attribution_through_native_cli_and_mcp()
-> Result<()> {
    let f = Fixture::new(false)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let arguments = json!({"project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],
        "selectors":[{"kind":"path","path":"api/policy.md"}],"limit":4,"max_bytes":8192,
        "verification":"require","verification_max_sources":4,"verification_max_bytes":8192,
        "verification_deadline_ms":4000,"response_mode":"full"});
    let native = native_read(backend.as_ref(), "airk_changes", &arguments).await?;
    let cli = f.cli("airk_changes", &arguments).await?;
    // U110: MCP reads the named index whole as an open read; it compares
    // with the same open read through the pipe.
    let open = open_arguments(&arguments)?;
    let wire = mcp.call(2, "changes", open.clone())?;
    ensure!(
        wire["result"]["isError"] != true,
        "MCP changes failed: {wire}"
    );
    let expected = body(&native)?;
    assert_parity("verified current changes CLI", &expected, &body(&cli)?)?;
    assert_parity(
        "verified current changes MCP",
        &body(&open_pipe(&f, "changes", &open).await?)?,
        &body(&wire["result"])?,
    )?;
    let members = expected["members"]
        .as_array()
        .context("changes members absent")?;
    assert_eq!(members.len(), 1, "{expected}");
    let member = member_body(&expected, &members[0])?;
    assert_eq!(
        member["validations"][0]["outcome"], "index-current",
        "{member}"
    );
    let receipt = &member["source_verification"];
    assert_eq!(receipt["publication_fence"], "current", "{receipt}");
    assert_eq!(receipt["status"], "verified", "{receipt}");
    assert_eq!(receipt["requirements_satisfied"], true, "{receipt}");
    assert_eq!(receipt["counts"]["references"], 1, "{receipt}");
    assert_eq!(receipt["counts"]["checked_sources"], 1, "{receipt}");
    assert_eq!(
        receipt["references"][0]["result_pointer"],
        "/validations/0/current"
    );
    let sources = receipt["sources"]
        .as_array()
        .context("checked sources absent")?;
    assert_eq!(sources.len(), 1, "{receipt}");
    assert_eq!(sources[0]["state"], "matched", "{receipt}");
    assert_eq!(sources[0]["path"], "api/policy.md");
    assert_eq!(
        sources[0]["source_root_id"],
        json!(SourceRootId::from_identity(Incarnation::from_bytes(
            [50; 16]
        )))
    );
    assert_eq!(sources[0]["project_ids"], json!([f.projects[0]]));
    assert_eq!(sources[0]["expected_hash"], sources[0]["observed_hash"]);
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}
