// SPDX-License-Identifier: Apache-2.0
//! Real scoped workers, native frontend, CLI call/console/info, and stdio MCP.
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

fn console_output(stdout: &Path, stderr: &Path) -> Result<(String, String)> {
    ensure!(
        fs::metadata(stdout)?.len() <= MAX_HELPER_OUTPUT_BYTES as u64
            && fs::metadata(stderr)?.len() <= MAX_HELPER_OUTPUT_BYTES as u64,
        "console output exceeds fixture bound"
    );
    Ok((fs::read_to_string(stdout)?, fs::read_to_string(stderr)?))
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

fn positive_mcp_body(wire: &Value, index: IndexId, label: &str) -> Result<Value> {
    positive_body(rpc_result(wire, label)?, index, label)
}

async fn cli_catalog(f: &Fixture) -> Result<Value> {
    let mut command = Command::new(&f.candidate);
    command
        .arg("--combined-session")
        .arg(serde_json::to_string(&f.session)?)
        .arg("--service-runtime-dir")
        .arg(&f.control)
        .args(["tools", "list", "--json"]);
    let output = tokio::task::spawn_blocking(move || read_retry::output(&mut command)).await??;
    cli_json("native CLI catalog", output)
}
async fn console(f: &Fixture, args: &Value) -> Result<Value> {
    let input = f.owner.path().join("profile-families-console.in");
    let stdout = f.owner.path().join("profile-families-console.out");
    let stderr = f.owner.path().join("profile-families-console.err");
    fs::write(&input, format!("call airk_file_info {args}\nexit\n"))?;
    let mut command = Command::new(&f.candidate);
    command
        .arg("--combined-session")
        .arg(serde_json::to_string(&f.session)?)
        .arg("--service-runtime-dir")
        .arg(&f.control)
        .args(["tools", "shell"])
        .stdin(fs::File::open(input)?)
        .stdout(
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&stdout)?,
        )
        .stderr(
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&stderr)?,
        );
    let mut process = OwnedProcess::start(command)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = process.child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            drop(process);
            let (out, err) = console_output(&stdout, &stderr)?;
            anyhow::bail!(
                "native console timeout; complete bounded stdout: {out}; complete bounded stderr: {err}"
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    drop(process);
    let (text, errors) = console_output(&stdout, &stderr)?;
    let diagnostics = || {
        format!(
            "native console: status={status}; complete bounded stdout: {text}; complete bounded stderr: {errors}"
        )
    };
    ensure!(status.success() && errors.is_empty(), "{}", diagnostics());
    // tools::call(raw=false) prints a Result marker before the complete JSON
    // text block, while shell prints a separate prompt before and after it.
    // Match the established helper_outage_reads parser, then also reject extra
    // response blocks instead of quietly discarding them.
    let normalized = text.replace("\r\n", "\n");
    let (banner, rendered) = normalized
        .split_once("Result:\n")
        .with_context(diagnostics)?;
    ensure!(
        banner.trim_end().ends_with("airk>") && !banner.contains("MCP error:"),
        "{}",
        diagnostics()
    );
    let mut values = serde_json::Deserializer::from_str(rendered).into_iter::<Value>();
    let value = values
        .next()
        .with_context(diagnostics)?
        .with_context(diagnostics)?;
    ensure!(
        rendered[values.byte_offset()..].trim() == "airk>",
        "{}",
        diagnostics()
    );
    Ok(value)
}
async fn convenience(f: &Fixture) -> Result<Value> {
    let mut command = Command::new(&f.candidate);
    command
        .arg("--combined-session")
        .arg(serde_json::to_string(&f.session)?)
        .arg("--service-runtime-dir")
        .arg(&f.control)
        .arg("--project-ids")
        .arg(f.projects[0].to_string())
        .arg("--index-ids")
        .arg(f.indexes[0].to_string())
        .args([
            "--member-availability",
            "require",
            "info",
            "api/policy.md",
            "--include-profile-families",
            "--verification",
            "off",
            "--reply-detail",
            "rich",
        ])
        .env("AIRK_MCP_RESPONSE_MODE", "full");
    let output = tokio::task::spawn_blocking(move || read_retry::output(&mut command)).await??;
    cli_json("native convenience info", output)
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
            let backend =
                airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
            let mut mcp = McpSession::start(&f)?;
            let catalog = mcp.request(2, "tools/list", json!({}))?;
            let catalog = rpc_result(&catalog, "MCP tool catalog")?;
            let public = catalog["tools"]
                .as_array()
                .context("MCP tool catalog absent")?
                .iter()
                .find(|tool| tool["name"] == "file_info")
                .context("MCP FileInfo absent")?;
            assert_eq!(
                cli_info, public,
                "actual CLI and MCP must retain the complete public FileInfo definition"
            );
            let native = serde_json::to_value(
                backend
                    .catalog()
                    .into_iter()
                    .find(|tool| tool.name == "airk_file_info")
                    .context("native FileInfo absent")?,
            )?;
            assert_eq!(
                public["inputSchema"]["properties"]["include_profile_families"],
                expected_schema
            );
            assert_eq!(
                native["inputSchema"]["properties"]["include_profile_families"],
                expected_schema
            );
            assert_eq!(cli_info["inputSchema"], native["inputSchema"]);
            let native_keys = native["inputSchema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<std::collections::BTreeSet<_>>();
            let public_keys = public["inputSchema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                native_keys, public_keys,
                "aliases must expose the same complete input inventory"
            );
            let mut arguments = json!({"path":"api/policy.md","include_profile_families":true,"project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"member_availability":"require","verification":"off","response_mode":mode});
            if let Some(detail) = detail {
                arguments["reply_detail"] = json!(detail);
            }
            let native = native_read(backend.as_ref(), "airk_file_info", &arguments).await?;
            let cli = f.cli("airk_file_info", &arguments).await?;
            let wire = mcp.call(3, "file_info", arguments.clone())?;
            let native = positive_body(&native, f.indexes[0], "opted native")?;
            let cli = positive_body(&cli, f.indexes[0], "opted CLI")?;
            let wire = positive_mcp_body(&wire, f.indexes[0], "opted MCP")?;
            assert_parity("complete opted file-info CLI result", &native, &cli)?;
            assert_parity("complete opted file-info MCP alias result", &native, &wire)?;
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
            if mode == "full" && detail == Some("rich") {
                let console = console(&f, &arguments).await?;
                assert_parity(
                    "complete actual tools console file-info",
                    &native,
                    &positive_body(&console, f.indexes[0], "canonical tools console")?,
                )?;
                assert_parity(
                    "complete convenience info family report",
                    &native,
                    &positive_body(&convenience(&f).await?, f.indexes[0], "convenience info")?,
                )?;
            }
            drop(mcp);
            backend.close();
            backend.shutdown().await?;
        }
    }
    // Absent/false preserves the exact legacy conditional representation key.
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
    let wire = conditional_mcp.call(3, "file_info", conditional.clone())?;
    assert_parity("opt-in conditional CLI representation", &native, &cli)?;
    assert_parity(
        "opt-in conditional MCP representation",
        &native,
        &positive_mcp_body(&wire, f.indexes[0], "conditional MCP")?,
    )?;
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
    let wire = conditional_mcp.call(4, "file_info", conditional)?;
    assert_parity("unchanged conditional CLI representation", &native, &cli)?;
    assert_parity(
        "unchanged conditional MCP representation",
        &native,
        &positive_mcp_body(&wire, f.indexes[0], "conditional MCP")?,
    )?;
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
    let wire = mcp.call(3, "file_info", full_args.clone())?;
    assert_parity("known collection policy CLI FileInfo", &native, &cli)?;
    assert_parity(
        "known collection policy MCP FileInfo",
        &native,
        &positive_mcp_body(&wire, f.indexes[2], "known policy MCP")?,
    )?;
    let result = member_body(&native, &native["members"][0])?;
    assert_eq!(
        result["profile_families"]["configured_collection_policy"]["state"],
        "known"
    );
    assert_eq!(
        result["profile_families"]["published_collection_policy"]["state"],
        "known"
    );
    let mut invalid = full_args;
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
