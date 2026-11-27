// SPDX-License-Identifier: Apache-2.0
//! Real scoped workers, native frontend, CLI call/console/info, and stdio MCP.
use super::changes_verification::{open_arguments, open_pipe};
use super::*;

const MAX_HELPER_OUTPUT_BYTES: usize = 1 << 20;

fn cli_json(label: &str, output: std::process::Output) -> Result<Value> {
    ensure!(
        output.stdout.len() <= MAX_HELPER_OUTPUT_BYTES
            && output.stderr.len() <= MAX_HELPER_OUTPUT_BYTES,
        "{label} output exceeds fixture bound: stdout={} stderr={}",
        output.stdout.len(),
        output.stderr.len()
    );
    let diagnostics = || {
        format!(
            "{label}: status={}; complete bounded stdout: {}; complete bounded stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    };
    ensure!(
        output.status.success() && output.stderr.is_empty(),
        "{}",
        diagnostics()
    );
    serde_json::from_slice(&output.stdout).with_context(diagnostics)
}

fn rpc_result<'a>(wire: &'a Value, label: &str) -> Result<&'a Value> {
    airk_core::index_catalog::encoded_size(wire, 3 * airk_core::MAX_RESPONSE_BYTES + 8192)?;
    ensure!(
        wire.get("error").is_none(),
        "{label} RPC error; complete bounded outer reply: {wire}"
    );
    wire.get("result")
        .filter(|value| value.is_object())
        .with_context(|| {
            format!("{label} RPC result absent or malformed; complete bounded outer reply: {wire}")
        })
}

fn positive_body(reply: &Value, index: IndexId, label: &str) -> Result<Value> {
    airk_core::index_catalog::encoded_size(reply, 3 * airk_core::MAX_RESPONSE_BYTES + 4096)?;
    ensure!(
        reply.is_object()
            && reply
                .get("isError")
                .is_none_or(|flag| flag == &json!(false)),
        "{label} error flag true or malformed; complete bounded outer reply: {reply}"
    );
    let value =
        body(reply).with_context(|| format!("{label}; complete bounded outer reply: {reply}"))?;
    let members = value["members"].as_array().with_context(|| {
        format!("{label} members absent; complete bounded outer reply: {reply}")
    })?;
    ensure!(
        members.len() == 1
            && members[0]["index_id"] == json!(index)
            && members[0]["outcome"] == "complete"
            && members[0]["is_error"] == false,
        "{label} positive exact selected member changed; expected {index}; complete bounded outer reply: {reply}"
    );
    Ok(value)
}

/// U110: a positive one-index open read (MCP or pipe) has no combined
/// member wrapper: its body is the index's own result and its `scope` names
/// exactly that index.
fn single_index_body(reply: &Value, index: IndexId, label: &str) -> Result<Value> {
    airk_core::index_catalog::encoded_size(reply, 3 * airk_core::MAX_RESPONSE_BYTES + 4096)?;
    ensure!(
        reply.is_object()
            && reply
                .get("isError")
                .is_none_or(|flag| flag == &json!(false)),
        "{label} error flag true or malformed; complete bounded outer reply: {reply}"
    );
    let value =
        body(reply).with_context(|| format!("{label}; complete bounded outer reply: {reply}"))?;
    ensure!(
        value["read_scope"]["index_ids"] == json!([index]) && value.get("members").is_none(),
        "{label} one-index open read changed; expected {index}; complete bounded outer reply: {reply}"
    );
    Ok(value)
}


/// The complete family report of one file result, with its bounded rows,
/// file identity and saved-source basis.
fn assert_family_report(result: &Value, label: &str) -> Result<()> {
    let report = &result["profile_families"];
    ensure!(
        report["families"]
            .as_array()
            .is_some_and(|rows| rows.len() == 18)
            && report["file"]["path"] == "api/policy.md"
            && report["file"]["publication"] == result["publication"]["token"]
            && report["live_source_checked"] == false
            // An open read names the whole index, so its collection policy
            // is known and a pending reindex is a definite false; a selected
            // subset (combined) cannot observe it (null).
            && report["pending_collection_reindex"]
                == if report["configured_collection_policy"]["state"] == "known" {
                    json!(false)
                } else {
                    Value::Null
                },
        "{label} family report changed; complete result: {result}"
    );
    Ok(())
}

async fn cli_catalog(f: &Fixture) -> Result<Value> {
    let mut command = Ctl::new();
    command
        .window(f.session_window().await?)
        .arg("--service-runtime-dir")
        .arg(&f.control)
        .args(["tools", "list", "--json"]);
    let output = tokio::task::spawn_blocking(move || read_retry::output(&mut command)).await??;
    cli_json("native CLI catalog", output)
}
#[tokio::test]
async fn opted_file_family_report_keeps_full_native_cli_console_mcp_and_input_catalog_parity()
-> Result<()> {
    let mut f = Fixture::new(true)?;
    f.session.project_ids = f.projects.clone();
    let _service = f.start().await?;
    let expected_schema = airk_core::native_reads::file_info_profile_families_schema();
    let catalogue = cli_catalog(&f).await?;
    let cli_info = catalogue["tools"]
        .as_array()
        .context("CLI tool catalog absent")?
        .iter()
        .find(|tool| tool["name"] == "file_info")
        .context("CLI FileInfo absent")?;
    assert_eq!(
        cli_info["inputSchema"]["properties"]["include_profile_families"],
        expected_schema
    );
    assert_eq!(
        airk_kernel::workbench::operation_catalog()["operations"]["file_info"]["properties"]["include_profile_families"],
        expected_schema
    );
    let mut report_identity = None;
    for mode in ["full", "terse"] {
        for detail in [None, Some("lean"), Some("rich")] {
            // Fresh response sessions avoid sharing omission observations across
            // transports; actual payload/provenance/scope fields remain intact.
            let backend = f.resolve(&f.session).await?;
            let mut mcp = McpSession::start(&f)?;
            let catalog = mcp.request(2, "tools/list", json!({}))?;
            let catalog = rpc_result(&catalog, "MCP tool catalog")?;
            // FileInfo is a CLI operation only (27 November 2026): MCP does
            // not list it, and refuses it below.
            assert!(
                !catalog["tools"]
                    .as_array()
                    .context("MCP tool catalog absent")?
                    .iter()
                    .any(|tool| tool["name"] == "file_info"),
                "MCP must not list file_info"
            );
            let mut arguments = json!({"path":"api/policy.md","include_profile_families":true,"project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"member_availability":"require","verification":"off","response_mode":mode});
            if let Some(detail) = detail {
                arguments["reply_detail"] = json!(detail);
            }
            let native = native_read(backend.as_ref(), "airk_file_info", &arguments).await?;
            let cli = f.cli("airk_file_info", &arguments).await?;
            let open = open_arguments(&arguments)?;
            let _refused = mcp.call(3, "file_info", open.clone())?;
            let native = positive_body(&native, f.indexes[0], "opted native")?;
            let cli = positive_body(&cli, f.indexes[0], "opted CLI")?;
            let piped = single_index_body(
                &open_pipe(&f, "file_info", &open).await?,
                f.indexes[0],
                "opted open pipe",
            )?;
            assert_parity("complete opted file-info CLI result", &native, &cli)?;
            assert_family_report(&piped, "opted open pipe")?;
            let member = native["members"]
                .as_array()
                .and_then(|members| members.first())
                .context("positive selected member absent")?;
            assert_eq!(member["outcome"], "complete");
            assert_eq!(member["is_error"], false);
            let result = member_body(&native, member)?;
            let report = &result["profile_families"];
            assert_eq!(
                report["families"]
                    .as_array()
                    .with_context(|| {
                        format!(
                            "family rows absent or malformed (mode={mode}, reply_detail={detail:?}); complete bounded native member: {result}"
                        )
                    })?
                    .len(),
                18
            );
            assert_eq!(
                report["configured_collection_policy"]["state"],
                "unavailable"
            );
            assert_eq!(
                report["configured_collection_policy"]["reason"],
                "selected_scope"
            );
            assert_eq!(
                report["published_collection_policy"]["state"],
                "unavailable"
            );
            assert!(report["pending_collection_reindex"].is_null());
            assert_eq!(
                report["file"]["publication"],
                result["publication"]["token"]
            );
            assert_eq!(report["file"]["path"], "api/policy.md");
            assert_eq!(report["live_source_checked"], false);
            if let Some(expected) = &report_identity {
                assert_eq!(report, expected);
            } else {
                report_identity = Some(report.clone());
            }
            // The tools console and narrowed convenience `info` rows went with
            // the removed `--combined-session` CLI selector: `tools shell` has
            // only its project form and the convenience commands no
            // `--project-ids/--index-ids` narrowing.
            drop(mcp);
            backend.close();
            backend.shutdown().await?;
        }
    }
    // Absent/false preserves the exact legacy conditional representation key.
    let backend = f.resolve(&f.session).await?;
    let base_args = json!({"path":"api/policy.md","project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"member_availability":"require","verification":"off","response_mode":"full"});
    let legacy = positive_body(
        &native_read(backend.as_ref(), "airk_file_info", &base_args).await?,
        f.indexes[0],
        "native base_args",
    )?;
    let mut explicit_false = base_args.clone();
    explicit_false["include_profile_families"] = json!(false);
    let explicit_false = positive_body(
        &native_read(backend.as_ref(), "airk_file_info", &explicit_false).await?,
        f.indexes[0],
        "native explicit_false",
    )?;
    let legacy = member_body(&legacy, &legacy["members"][0])?;
    let explicit_false = member_body(&explicit_false, &explicit_false["members"][0])?;
    assert!(legacy.get("profile_families").is_none());
    assert!(explicit_false.get("profile_families").is_none());
    assert_eq!(
        legacy["read"]["validator"],
        explicit_false["read"]["validator"]
    );
    let mut conditional = base_args.clone();
    conditional["include_profile_families"] = json!(true);
    conditional["if_evidence"] =
        json!(legacy["read"]["validator"].as_str().with_context(|| {
            format!("legacy conditional validator absent; complete bounded member: {legacy}")
        })?);
    let mut conditional_mcp = McpSession::start(&f)?;
    let native = positive_body(
        &native_read(backend.as_ref(), "airk_file_info", &conditional).await?,
        f.indexes[0],
        "native conditional",
    )?;
    let cli = positive_body(
        &f.cli("airk_file_info", &conditional).await?,
        f.indexes[0],
        "CLI conditional",
    )?;
    let mut open = open_arguments(&conditional)?;
    let _refused = conditional_mcp.call(3, "file_info", open.clone())?;
    // FileInfo is CLI-only (27 November 2026): the open pipe is the
    // service path MCP used to reach.
    let wire = single_index_body(&open_pipe(&f, "file_info", &open).await?, f.indexes[0], "conditional MCP")?;
    assert_parity("opt-in conditional CLI representation", &native, &cli)?;
    assert_parity(
        "opt-in conditional MCP representation",
        &single_index_body(
            &open_pipe(&f, "file_info", &open).await?,
            f.indexes[0],
            "conditional open pipe",
        )?,
        &wire,
    )?;
    // The open read's legacy-validated opt-in also returns its family
    // representation; its own opted validator then answers not-modified.
    assert_eq!(
        wire["read"]["status"], "content",
        "legacy validator suppressed the requested MCP family representation"
    );
    assert_family_report(&wire, "conditional MCP")?;
    open["if_evidence"] = json!(wire["read"]["validator"].as_str().with_context(|| {
        format!("opted MCP conditional validator absent; complete bounded result: {wire}")
    })?);
    let result = member_body(&native, &native["members"][0])?;
    assert_eq!(
        result["read"]["status"], "content",
        "legacy validator suppressed the requested family representation"
    );
    conditional["if_evidence"] =
        json!(result["read"]["validator"].as_str().with_context(|| {
            format!("opted conditional validator absent; complete bounded member: {result}")
        })?);
    let native = positive_body(
        &native_read(backend.as_ref(), "airk_file_info", &conditional).await?,
        f.indexes[0],
        "native conditional",
    )?;
    let cli = positive_body(
        &f.cli("airk_file_info", &conditional).await?,
        f.indexes[0],
        "CLI conditional",
    )?;
    let _refused = conditional_mcp.call(4, "file_info", open.clone())?;
    // FileInfo is CLI-only (27 November 2026): the open pipe is the
    // service path MCP used to reach.
    let wire = single_index_body(&open_pipe(&f, "file_info", &open).await?, f.indexes[0], "conditional MCP")?;
    assert_parity("unchanged conditional CLI representation", &native, &cli)?;
    assert_parity(
        "unchanged conditional MCP representation",
        &single_index_body(
            &open_pipe(&f, "file_info", &open).await?,
            f.indexes[0],
            "conditional open pipe",
        )?,
        &wire,
    )?;
    assert_eq!(wire["read"]["status"], "not_modified", "{wire}");
    assert!(
        wire.get("profile_families").is_none(),
        "not-modified MCP response repeated payload: {wire}"
    );
    let result = member_body(&native, &native["members"][0])?;
    assert_eq!(result["read"]["status"], "not_modified");
    assert!(
        result.get("profile_families").is_none(),
        "not-modified response repeated payload: {result}"
    );
    drop(conditional_mcp);
    backend.close();
    backend.shutdown().await?;
    // The full-root participant transports the known configured/published
    // policy observations as well; selected-scope unknowns must not become a
    // permanently reduced response contract for every native caller.
    let backend = f.resolve(&f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let full_args = json!({"path":"api/policy.md","include_profile_families":true,"project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"member_availability":"require","verification":"off","response_mode":"full","reply_detail":"rich"});
    let native = positive_body(
        &native_read(backend.as_ref(), "airk_file_info", &full_args).await?,
        f.indexes[2],
        "native full_args",
    )?;
    let cli = positive_body(
        &f.cli("airk_file_info", &full_args).await?,
        f.indexes[2],
        "CLI full_args",
    )?;
    let open = open_arguments(&full_args)?;
    let _refused = mcp.call(3, "file_info", open.clone())?;
    // FileInfo is CLI-only (27 November 2026): the open pipe is the
    // service path MCP used to reach.
    let wire = single_index_body(&open_pipe(&f, "file_info", &open).await?, f.indexes[2], "known policy MCP")?;
    assert_parity("known collection policy CLI FileInfo", &native, &cli)?;
    assert_parity(
        "known collection policy MCP FileInfo",
        &single_index_body(
            &open_pipe(&f, "file_info", &open).await?,
            f.indexes[2],
            "known policy open pipe",
        )?,
        &wire,
    )?;
    assert_family_report(&wire, "known policy MCP")?;
    for policy in [
        "configured_collection_policy",
        "published_collection_policy",
    ] {
        assert_eq!(
            wire["profile_families"][policy]["state"], "known",
            "known policy MCP {policy}: {wire}"
        );
    }
    let result = member_body(&native, &native["members"][0])?;
    assert_eq!(
        result["profile_families"]["configured_collection_policy"]["state"],
        "known"
    );
    assert_eq!(
        result["profile_families"]["published_collection_policy"]["state"],
        "known"
    );
    let mut invalid = open;
    invalid["include_profile_families"] = json!("true");
    let refused = mcp.call(4, "file_info", invalid)?;
    assert!(
        refused.get("error").is_some() || refused["result"]["isError"] == true,
        "nonboolean opt-in accepted; complete bounded native reply: {refused}"
    );
    drop(mcp);
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}
