// SPDX-License-Identifier: Apache-2.0
//! Current remembered evidence must retain exact native saved-source authority.
use super::*;

#[tokio::test]
async fn current_changes_source_receipts_preserve_attribution_through_native_cli_and_mcp()
-> Result<()> {
    let f = Fixture::new(false)?;
    let _service = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let arguments = json!({"project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],
        "selectors":[{"kind":"path","path":"api/policy.md"}],"limit":4,"max_bytes":8192,
        "verification":"require","verification_max_sources":4,"verification_max_bytes":8192,
        "verification_deadline_ms":4000,"response_mode":"full"});
    let native = native_read(backend.as_ref(), "airk_changes", &arguments).await?;
    let cli = f.cli("airk_changes", &arguments).await?;
    let wire = mcp.call(2, "changes", arguments)?;
    let expected = body(&native)?;
    assert_parity("verified current changes CLI", &expected, &body(&cli)?)?;
    assert_parity(
        "verified current changes MCP",
        &expected,
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
