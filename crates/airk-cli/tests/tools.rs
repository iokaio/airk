// SPDX-License-Identifier: Apache-2.0
use serde_json::{Value, json};
mod common;
use common::parity::{CLOCK_FIELDS, Parity};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn command(root: &Path, state: &Path) -> common::ctl::Ctl {
    common::evidence_command(root, state, "tools-test")
}

/// A read with no project fixture. `airkctl` asks a service for the catalog
/// and the help tool (the pre-split CLI answered them in process), so a
/// project-less service stands in for the absent installation.
fn offline(root: &Path, state: &Path, service: &common::BareService) -> common::ctl::Ctl {
    let mut command = common::ctl::Ctl::new();
    command
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(state)
        .args(["--collection", "tools-test", "--service-runtime-dir"])
        .arg(&service.control)
        .env("AIRK_MCP_RESPONSE_MODE", "full");
    command
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn evidence(output: Output) -> Value {
    let envelope = success(output);
    assert_ne!(envelope["isError"], true);
    serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn tools_help_works_without_a_workspace_and_exposes_live_schemas() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("missing root");
    let state = base.path().join("missing state");
    let service = common::BareService::start();
    let command = |root: &Path, state: &Path| offline(root, state, &service);
    let list = success(
        command(&root, &state)
            .args(["tools", "list", "--json"])
            .output()
            .unwrap(),
    );
    let tools = list["tools"].as_array().unwrap();
    common::assert_unified_mcp_catalog_names(tools);
    assert_eq!(
        list["tools"],
        serde_json::to_value(airk_mcp::unified::tools()).unwrap()
    );
    assert!(tools.iter().any(|tool| tool["name"] == "help"));
    let query_scope = tools
        .iter()
        .find(|tool| tool["name"] == "query_scope")
        .expect("query scope metadata tool");
    assert_eq!(query_scope["annotations"]["readOnlyHint"], true);
    let mut discovery_properties =
        airk_core::combined_reads::query_scope::schema()["properties"].clone();
    // Public routing permits only the already configured project binding;
    // it is consumed by Unified before native grant-bound discovery.
    discovery_properties["project"] = query_scope["inputSchema"]["properties"]["project"].clone();
    assert_eq!(discovery_properties["project"]["pattern"], "^[0-9a-f]{64}$");
    assert_eq!(
        query_scope["inputSchema"]["properties"],
        discovery_properties
    );
    assert_eq!(query_scope["inputSchema"]["additionalProperties"], false);
    for field in ["project_ids", "index_ids", "verification", "response_mode"] {
        assert!(
            query_scope["inputSchema"]["properties"]
                .get(field)
                .is_none(),
            "discovery must not advertise {field}"
        );
    }
    for alias in ["query_scope", "airk_query_scope"] {
        let discovery = command(&root, &state)
            .args(["tools", "call", alias, "--args", r#"{"view":"projects"}"#])
            .output()
            .unwrap();
        assert_eq!(discovery.status.code(), Some(1));
        assert!(
            discovery.stdout.is_empty(),
            "missing authority must not produce successful discovery"
        );
        assert!(
            String::from_utf8_lossy(&discovery.stderr).contains("project_unresolved"),
            "{}",
            String::from_utf8_lossy(&discovery.stderr)
        );
        assert!(
            !root.exists() && !state.exists(),
            "scope discovery created source/private state"
        );
    }
    let help = evidence(
        command(&root, &state)
            .args(["tools", "call", "airk_help"])
            .output()
            .unwrap(),
    );
    assert!(help["topics"].as_array().unwrap().len() >= 25);
    let help = evidence(
        command(&root, &state)
            .args([
                "tools",
                "call",
                "airk_help",
                "--args",
                r#"{"command":"airkctl agents connect"}"#,
            ])
            .output()
            .unwrap(),
    );
    assert!(
        help["content"]
            .as_str()
            .unwrap()
            .contains("--client <CLIENT>")
    );
    assert!(!root.exists() && !state.exists());
    let status = tools.iter().find(|tool| tool["name"] == "status").unwrap();
    assert!(
        status["description"]
            .as_str()
            .unwrap()
            .contains("Quick version, health and project-index check")
    );
    for tool in tools {
        let help = success(
            command(&root, &state)
                .args(["tools", "help", tool["name"].as_str().unwrap(), "--json"])
                .output()
                .unwrap(),
        );
        assert_eq!(&help, tool);
    }
    let search = tools.iter().find(|t| t["name"] == "search").unwrap();
    assert_eq!(search["inputSchema"]["properties"]["limit"]["default"], 10);
    let output = command(&root, &state)
        .args(["tools", "help", "airk_search"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--args-file query.json") && help.contains("Explicit vector search"));
    assert!(
        !command(&root, &state)
            .args(["tools", "help", "not_a_tool"])
            .status()
            .unwrap()
            .success()
    );
    assert!(!root.exists() && !state.exists());
}

#[test]
fn tool_calls_preserve_evidence_scope_errors_and_argument_budgets() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("README.md"),
        "# Fixture\nAuthenticate the local account.\n",
    )
    .unwrap();
    let service = common::Service::reviewed(root.path(), state.path(), "tools-test");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    let accepted = success(
        common::command(root.path(), state.path(), "tools-test")
            .arg("index")
            .output()
            .unwrap(),
    );
    service.wait(serde_json::from_value(accepted["job"]["id"].clone()).unwrap());
    let query = state.path().join("query with spaces.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    // An accepted Index job can finish before its setup/source continuations.
    // Settle the owned native work before asserting running-worker diagnostics.
    service
        .settle_reviewed_index(root.path(), state.path(), deadline)
        .unwrap();
    let status = evidence(
        common::read_retry::output_until(
            command(root.path(), state.path()).args(["tools", "call", "airk_diagnostics"]),
            deadline,
        )
        .unwrap(),
    );
    common::read_retry::publication_pending(
        &status,
        "tools-test",
        &service.project.binding.to_string(),
    )
    .unwrap();
    // A reviewed index grant intentionally cannot disclose project-wide jobs,
    // process inventory, or watcher state. Observe those using the fixture's
    // native administrative authority, never widen its evidence grant.
    assert_eq!(
        status["runtime"],
        json!({"schema":1,"error":"native_runtime_context_unavailable"})
    );
    assert_eq!(
        status["work"],
        json!({"error":"native_work_context_unavailable"})
    );
    assert!(status["scope_observation"]["fingerprint"].is_string());
    use airk_core::service::{ControlMethod, ControlResponse};
    let health = match service
        .request_until(ControlMethod::Health, deadline)
        .unwrap()
    {
        ControlResponse::Health { observation } => observation,
        _ => panic!("native health unavailable"),
    };
    let worker = match service
        .request_until(
            ControlMethod::Worker {
                project: service.project.binding,
            },
            deadline,
        )
        .unwrap()
    {
        ControlResponse::Worker {
            project,
            identity: Some(identity),
            ..
        } => {
            assert_eq!(project, service.project.binding);
            identity
        }
        _ => panic!("native worker identity unavailable"),
    };
    assert_eq!(worker.project, service.project.binding);
    assert_eq!(worker.service, health.instance);
    assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    // The current single executable is the actual service image. Do not invent
    // a scoped worker/front-end process report that the native API withholds.
    use sha2::{Digest, Sha256};
    let expected_image = format!(
        "{:x}",
        Sha256::digest(fs::read(env!("CARGO_BIN_EXE_airk")).unwrap())
    );
    assert_eq!(
        serde_json::to_value(&health).unwrap()["executable_sha256"],
        expected_image
    );
    let version =
        common::read_retry::once_until(&*common::ctl::Ctl::new().arg("--version"), deadline)
            .unwrap();
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );
    match service
        .request_until(
            ControlMethod::Progress {
                project: worker.project,
            },
            deadline,
        )
        .unwrap()
    {
        ControlResponse::Progress {
            project,
            pending_job,
            queued_jobs,
            observation,
            ..
        } => {
            assert_eq!(project, worker.project);
            assert!(pending_job.is_none_or(|job| job.project == worker.project));
            assert!(queued_jobs.iter().all(|job| job.project == worker.project));
            assert!(
                observation.is_none_or(|sample| sample.worker.project == worker.project
                    && sample.job.project == worker.project)
            );
        }
        _ => panic!("native work observation unavailable"),
    }
    match service
        .request_until(
            ControlMethod::Watch {
                project: worker.project,
            },
            deadline,
        )
        .unwrap()
    {
        ControlResponse::Watch {
            project,
            observation,
        } => {
            assert_eq!(project, worker.project);
            observation.validate().unwrap();
            assert!(serde_json::to_value(observation).unwrap()["enabled"].is_boolean());
        }
        _ => panic!("native watch observation unavailable"),
    }
    fs::write(
        &query,
        br#"{"query":"Authenticate","mode":"lexical","limit":1}"#,
    )
    .unwrap();
    let found = evidence(
        common::read_retry::output(
            command(root.path(), state.path())
                .args(["tools", "call", "airk_search", "--args-file"])
                .arg(&query),
        )
        .unwrap(),
    );
    assert_eq!(found["collection"], "tools-test");
    assert_eq!(found["hits"][0]["citation"]["path"], "README.md");
    let read = json!({"path":"README.md","start_line":1,"end_line":2,"revision":found["publication"]["token"]});
    let read = evidence(
        common::read_retry::output(command(root.path(), state.path()).args([
            "tools",
            "call",
            "airk_read_file",
            "--args",
            &read.to_string(),
        ]))
        .unwrap(),
    );
    assert_eq!(read["publication"], found["publication"]);
    assert_eq!(
        read["hits"][0]["citation"]["content_hash"],
        found["hits"][0]["citation"]["content_hash"]
    );
    for (tool, args) in [
        ("airk_search", "[]"),
        ("airk_search", "{"),
        ("not_a_tool", "{}"),
        (
            "airk_search",
            r#"{"query":"Authenticate","collection":"other"}"#,
        ),
        ("airk_search", r#"{"query":"Authenticate","mode":"vector"}"#),
        ("airk_search", r#"{"query":42}"#),
        (
            "airk_read_file",
            r#"{"path":"../secret","start_line":1,"end_line":1}"#,
        ),
    ] {
        let output = command(root.path(), state.path())
            .args(["tools", "call", tool, "--args", args])
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {tool}: {args}");
        if !output.stdout.is_empty() {
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["isError"], true);
        }
    }
    fs::write(&query, vec![b' '; 65537]).unwrap();
    let output = command(root.path(), state.path())
        .args(["tools", "call", "airk_search", "--args-file"])
        .arg(&query)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("64 KiB"));
    assert!(
        !command(root.path(), state.path())
            .args([
                "tools",
                "call",
                "airk_search",
                "--args",
                "{}",
                "--args-file"
            ])
            .arg(&query)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn interactive_tools_show_help_and_recover_from_invalid_requests() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let _service = common::Service::reviewed(root.path(), state.path(), "tools-test");
    let query = root.path().join("arguments with spaces.json");
    fs::write(&query, r#"{"reply_detail":"rich","adapter":"text"}"#).unwrap();
    // Interactive failure must not end the session. A window-routed shell
    // has no project folder, so `--args-file` names the file absolutely.
    let output = command(root.path(), state.path())
        .args(["tools", "shell"])
        .input(format!("help\nhelp airk_search\ncall not_a_tool\ncall airk_search []\ncall airk_capabilities --args-file {}\nexit\n", query.to_str().unwrap()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("AIRK Tools")
            && text.contains("Window selection")
            && text.contains("Explicit vector search")
    );
    assert!(text.contains("Result:") && text.contains("\"search_modes\": ["));
    assert!(text.contains("\"reply_detail\": \"rich\""), "{text}");
    assert!(
        text.contains("\"subject\": \"selected_adapter\"") && text.contains("\"read_guidance\""),
        "{text}"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("JSON object"));
}

#[test]
fn native_help_cli_reports_detail_selector_field_without_opening_a_workspace() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("missing root");
    let state = base.path().join("missing state");
    let service = common::BareService::start();
    let command = |root: &Path, state: &Path| offline(root, state, &service);
    let args = json!({"topic":"workbench","detail":"full"});
    let output = command(&root, &state)
        .args(["tools", "call", "airk_help", "--args", &args.to_string()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result,
        serde_json::to_value(airk_mcp::help::call(args)).unwrap()
    );
    let value: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["error"]["code"], "invalid_help_arguments");
    assert_eq!(value["error"]["field"], "detail");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("query or topic_id")
    );
    assert!(!root.exists() && !state.exists());
}

/// Fields that vary by observation, as the CLI/MCP parity tests strip them.
fn stable(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for key in [
                "request_id",
                "observed_unix_ms",
                "observed_from_unix_ms",
                "observed_to_unix_ms",
                "cache_age_ms",
                "cached",
                "elapsed_ms",
            ] {
                fields.remove(key);
            }
            fields.values_mut().for_each(stable);
        }
        Value::Array(rows) => rows.iter_mut().for_each(stable),
        _ => {}
    }
}

/// An envelope with its JSON text content parsed, then stabilized.
fn comparable(mut envelope: Value) -> Value {
    if let Some(content) = envelope["content"].as_array_mut() {
        for item in content {
            if let Some(text) = item["text"].as_str()
                && let Ok(parsed) = serde_json::from_str::<Value>(text)
            {
                item["text"] = parsed;
            }
        }
    }
    stable(&mut envelope);
    envelope
}

/// M3 runs the CLI's own call path in the service: for each selection the
/// control answer equals what `airk tools call` printed (split step 3a.2b).
// Current parity never opens or rewrites the historical U95 captures. Only the
// documented per-call identities/observation clocks in `comparable` vary; all
// evidence, revision, source, compiler and model digests compare byte for byte.
fn current_native(
    control: &Path,
    method: airk_core::service::ControlMethod,
) -> anyhow::Result<airk_core::service::ControlResponse> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(airk_cli::service_runtime::request(control, method))
}

fn current_unix_ms() -> anyhow::Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

// The final Diagnostics observation in the ordinary settlement helper can
// create a due automatic audit. Observe that distinct native scheduler too.
fn current_quiescent(
    service: &common::Service,
    root: &Path,
    state: &Path,
    until: std::time::Instant,
) -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse, JobKind, JobOutcome, JobState};
    service.settle_reviewed_index(root, state, until)?;
    let mut observed_audits = std::collections::BTreeSet::new();
    loop {
        anyhow::ensure!(
            std::time::Instant::now() < until,
            "original parity quiescence deadline exhausted"
        );
        let progress = service.metadata(
            ControlMethod::Progress {
                project: service.project.binding,
            },
            until,
        );
        let ControlResponse::Progress {
            observation,
            pending_job,
            queued_jobs,
            queue_truncated,
            ..
        } = progress
        else {
            anyhow::bail!("native quiescence progress unavailable");
        };
        for job in observation
            .as_ref()
            .map(|value| &value.job)
            .into_iter()
            .chain(pending_job.iter())
            .chain(queued_jobs.iter())
        {
            anyhow::ensure!(
                job.project == service.project.binding,
                "quiescence crossed native project"
            );
            if job.kind == JobKind::Verify {
                observed_audits.insert(job.id);
            }
        }
        let idle = pending_job.is_none()
            && queued_jobs.is_empty()
            && queue_truncated == Some(false)
            && observation.as_ref().is_none_or(|value| {
                !matches!(value.job.state, JobState::Pending | JobState::Running)
            });
        let registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &service.control,
            Some(until),
        )?;
        let due = registry.due_automatic_audits(current_unix_ms()?, 32)?;
        anyhow::ensure!(due.len() < 32, "quiescence audit census truncated");
        let audit_due = due
            .iter()
            .any(|(project, _)| *project == service.project.binding);
        drop(registry);
        if idle
            && !audit_due
            && !service.background_pending_until(service.project.binding, until)?
        {
            // Read the durable accepted-work census after the due-audit check;
            // it includes an audit accepted before its first cached sample.
            for id in &observed_audits {
                let ControlResponse::Job { job } =
                    service.metadata(ControlMethod::Job { id: *id }, until)
                else {
                    anyhow::bail!("observed audit job disappeared");
                };
                anyhow::ensure!(
                    job.project == service.project.binding
                        && job.kind == JobKind::Verify
                        && job.state == JobState::Succeeded
                        && matches!(job.outcome, Some(JobOutcome::Succeeded { .. })),
                    "actual automatic audit did not complete successfully: {job:?}"
                );
            }
            return Ok(());
        }
        // Every iteration observes actual native state under the same deadline.
        std::thread::yield_now();
    }
}

// Only these two direct Status/work clock paths may differ. This is not a
// recursive field-name mask: nested evidence fields with these names survive.
fn current_observation(mut value: Value, started: u64, finished: u64) -> anyhow::Result<Value> {
    fn status(body: &mut Value, started: u64, finished: u64) -> anyhow::Result<()> {
        if body.pointer("/work/basis").and_then(Value::as_str)
            != Some("instance_bound_service_metadata")
        {
            return Ok(());
        }
        anyhow::ensure!(started <= finished, "crossed native call interval");
        let project: airk_core::service::ProjectBinding =
            serde_json::from_value(body["project"].clone())?;
        let work = &mut body["work"];
        anyhow::ensure!(work["schema"] == 1, "unknown native status work schema");
        let now = work["observed_unix_ms"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("native status source clock absent"))?;
        anyhow::ensure!(
            (started..=finished).contains(&now),
            "native status source clock outside actual call interval"
        );
        if !work["progress"].is_null() {
            let sampled = &work["progress"];
            let worker: airk_core::service::WorkerIdentity =
                serde_json::from_value(sampled["worker"].clone())?;
            let job: airk_core::service::JobRecord =
                serde_json::from_value(sampled["job"].clone())?;
            let counter_job: airk_core::identity::Sha256Digest =
                serde_json::from_value(sampled["progress"]["job"].clone())?;
            anyhow::ensure!(
                worker.project == project && job.project == project && counter_job == job.id,
                "native progress worker/job/counters crossed status project"
            );
            let observed = sampled["observed_unix_ms"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("native progress clock absent"))?;
            let age = work["progress_observation_age_ms"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("native progress age absent"))?;
            anyhow::ensure!(
                observed <= now && age == now.saturating_sub(observed),
                "native progress age does not derive from original observation"
            );
            work["progress_observation_age_ms"] = json!("<validated-native-progress-age>");
        } else {
            anyhow::ensure!(
                work["progress_observation_age_ms"].is_null(),
                "absent progress fabricated an age"
            );
        }
        if let Some(sampled) = work["progress_sampled_unix_ms"].as_u64() {
            anyhow::ensure!(
                (started..=now).contains(&sampled),
                "native registry sample clock outside actual call interval"
            );
            work["progress_sampled_unix_ms"] = json!("<validated-native-progress-sample>");
        } else {
            anyhow::ensure!(
                work["progress_sampled_unix_ms"].is_null(),
                "invalid native registry sample clock"
            );
        }
        Ok(())
    }
    status(&mut value, started, finished)?;
    if let Some(contents) = value.get_mut("content").and_then(Value::as_array_mut) {
        for content in contents {
            if let Some(text) = content.get("text").and_then(Value::as_str)
                && let Ok(mut body) = serde_json::from_str::<Value>(text)
            {
                status(&mut body, started, finished)?;
                content["text"] = body;
            }
        }
    }
    if let Some(body) = value.get_mut("structuredContent") {
        status(body, started, finished)?;
    }
    Ok(comparable(value))
}

#[test]
fn current_status_clock_comparison_preserves_state_counters_and_stable_digests()
-> anyhow::Result<()> {
    let original = json!({"project":"1111111111111111111111111111111111111111111111111111111111111111",
        "source_sha256":"3333333333333333333333333333333333333333333333333333333333333333",
        "work":{"schema":1,"basis":"instance_bound_service_metadata","observed_unix_ms":1050,
        "progress_sampled_unix_ms":1040,"progress_observation_age_ms":150,
        "progress":{"worker":{"service":"44444444444444444444444444444444","worker":"55555555555555555555555555555555","project":"1111111111111111111111111111111111111111111111111111111111111111","fencing_generation":1},
        "job":{"id":"2222222222222222222222222222222222222222222222222222222222222222","project":"1111111111111111111111111111111111111111111111111111111111111111","kind":"verify","state":"succeeded","fencing_generation":1,"attempts":1,"outcome":{"state":"succeeded","revision":null}},
        "observed_unix_ms":900,"progress":{"job":"2222222222222222222222222222222222222222222222222222222222222222","committed_changes":4}}}});
    let baseline = current_observation(original.clone(), 1000, 1100)?;
    let mut later = original.clone();
    later["work"]["observed_unix_ms"] = json!(1060);
    later["work"]["progress_sampled_unix_ms"] = json!(1050);
    later["work"]["progress_observation_age_ms"] = json!(160);
    assert_eq!(current_observation(later, 1000, 1100)?, baseline);
    for (pointer, changed) in [
        ("/work/progress/job/state", json!("failed")),
        ("/work/progress/progress/committed_changes", json!(5)),
        (
            "/source_sha256",
            json!("6666666666666666666666666666666666666666666666666666666666666666"),
        ),
    ] {
        let mut mutated = original.clone();
        *mutated.pointer_mut(pointer).unwrap() = changed;
        assert_ne!(current_observation(mutated, 1000, 1100)?, baseline);
    }
    for (pointer, changed) in [
        ("/work/progress_observation_age_ms", json!(999)),
        ("/work/progress_sampled_unix_ms", json!(1051)),
        ("/work/observed_unix_ms", json!(999)),
    ] {
        let mut mutated = original.clone();
        *mutated.pointer_mut(pointer).unwrap() = changed;
        assert!(current_observation(mutated, 1000, 1100).is_err());
    }
    let mut nested = original;
    nested["evidence"] = json!({"progress_observation_age_ms":77,"progress_sampled_unix_ms":88});
    let retained = current_observation(nested, 1000, 1100)?;
    assert_eq!(
        retained["evidence"],
        json!({"progress_observation_age_ms":77,"progress_sampled_unix_ms":88})
    );
    Ok(())
}
fn current_tool(
    control: &Path,
    selection: airk_core::service::ToolSelection,
    tool: &str,
    arguments: Value,
) -> anyhow::Result<(airk_core::service::ControlResponse, u64)> {
    let started = current_unix_ms()?;
    let response = current_native(
        control,
        airk_core::service::ControlMethod::ToolsCall {
            read_attempt: None,
            tool: tool.into(),
            arguments: arguments.as_object().unwrap().clone(),
            selection,
            response_mode: airk_core::service::ToolResponseMode::Full,
            usage_client: airk_core::usage::ClientFamily::Cli,
        },
    )?;
    Ok((response, started))
}

fn current_ctl(control: &Path, selection: &[&str], arguments: &[&str]) -> anyhow::Result<Output> {
    current_ctl_routed(control, None, selection, arguments)
}

/// As `current_ctl`, with reads that name no selection routed through a
/// window as an `airkctl session` routes them.
fn current_ctl_routed(
    control: &Path,
    window: Option<&common::pipe::Window>,
    selection: &[&str],
    arguments: &[&str],
) -> anyhow::Result<Output> {
    let mut command = common::ctl::Ctl::new();
    if let Some(window) = window {
        command.window(window.clone());
    }
    Ok(command
        .arg("--service-runtime-dir")
        .arg(control)
        .args(["--output", "json"])
        .args(selection)
        .args(arguments)
        .env("AIRK_MCP_RESPONSE_MODE", "full")
        .output()?)
}

fn current_assert_tool(
    expected: (airk_core::service::ControlResponse, u64),
    actual: Output,
    local_help: bool,
    label: &str,
) -> anyhow::Result<bool> {
    use airk_core::service::ControlResponse;
    use airkctl::contract::{ErrorLine, ExitClass};
    let finished = current_unix_ms()?;
    let (expected, started) = expected;
    let error = match expected {
        ControlResponse::ToolResult { result } => {
            assert_eq!(
                current_observation(serde_json::from_slice(&actual.stdout)?, started, finished)?,
                current_observation(result.clone(), started, finished)?,
                "{label}: full native envelope"
            );
            if result["isError"] == true {
                Some(ErrorLine::new(
                    ExitClass::Refused,
                    "tool_failed",
                    false,
                    if local_help {
                        "AIRK help request failed"
                    } else {
                        "MCP tool failed; see the result above"
                    },
                ))
            } else {
                None
            }
        }
        ControlResponse::ToolCallFailed { message } => {
            assert!(
                actual.stdout.is_empty(),
                "{label}: failure fabricated stdout"
            );
            let mut line = ErrorLine::new(ExitClass::Refused, "operation_failed", false, &message);
            line.native = Some(serde_json::to_value(ControlResponse::ToolCallFailed {
                message,
            })?);
            Some(line)
        }
        other => {
            assert!(
                actual.stdout.is_empty(),
                "{label}: refusal fabricated stdout"
            );
            Some(airkctl::service::refusal(&other))
        }
    };
    if let Some(error) = error {
        assert_eq!(
            actual.status.code(),
            Some(i32::from(error.class.code())),
            "{label}"
        );
        let text = std::str::from_utf8(&actual.stderr)?;
        assert_eq!(
            text.lines().count(),
            1,
            "{label}: complete failure stream: {text}"
        );
        assert_eq!(
            serde_json::from_str::<Value>(text)?,
            serde_json::to_value(error)?,
            "{label}: full I2 failure"
        );
        Ok(false)
    } else {
        assert!(
            actual.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert!(actual.stderr.is_empty(), "{label}: unexpected stderr");
        Ok(true)
    }
}

#[cfg(windows)]
enum WindowRepairCase {
    FirstRequest,
    Paused,
    Concurrent,
    Timing,
}

#[cfg(windows)]
#[test]
fn first_window_search_returns_verified_fts_repair() -> anyhow::Result<()> {
    window_repair_case(WindowRepairCase::FirstRequest)
}

#[cfg(windows)]
#[test]
fn window_search_defers_paused_verify_and_recovers_after_resume() -> anyhow::Result<()> {
    window_repair_case(WindowRepairCase::Paused)
}

#[cfg(windows)]
#[test]
fn concurrent_window_searches_coalesce_one_fts_repair() -> anyhow::Result<()> {
    window_repair_case(WindowRepairCase::Concurrent)
}

#[cfg(windows)]
#[test]
#[ignore = "fixed synthetic 4/40 repair-triggering stress workload; run in an owned Windows fixture"]
fn window_search_synthetic_repair_latency_stress() -> anyhow::Result<()> {
    window_repair_case(WindowRepairCase::Timing)
}

#[cfg(windows)]
fn window_repair_case(case: WindowRepairCase) -> anyhow::Result<()> {
    use airk_core::{
        DocumentLocation, SearchResponse,
        scoped_reads::ScopeObservation,
        service::{ControlMethod, ControlResponse, JobKind, JobOutcome, JobState},
        service_events::{EventCursor, EventKind},
    };
    use airk_store::{
        current::CurrentToken,
        index::DATABASE_FILE,
        service::{collection::Collection, registry::ServiceRegistry},
    };
    use anyhow::{Context, ensure};
    use rusqlite::{Connection, OpenFlags};
    use sha2::{Digest, Sha256};
    use std::time::{Duration, Instant};

    const COLLECTION: &str = "window-fts-repair";
    const TARGET: &str = "target.txt";
    const CONTROL: &str = "control.txt";
    const TARGET_TEXT: &str = "repairneedle café 界";
    const CONTROL_TEXT: &str = "controlneedle façade";
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join(TARGET), TARGET_TEXT)?;
    fs::write(root.path().join(CONTROL), CONTROL_TEXT)?;
    let service = common::Service::reviewed(root.path(), state.path(), COLLECTION);
    let quiesce = || {
        current_quiescent(
            &service,
            root.path(),
            state.path(),
            Instant::now() + Duration::from_secs(90),
        )
    };
    quiesce()?;
    let window = service.window()?;
    let catalog = service.reviewed_scope();
    assert_eq!(window.selection.index_ids, [catalog.index_id]);
    let project = service.project.binding;
    let store = Collection::existing(
        Path::new(&service.project.data_root),
        &service.project.root,
        &service.project.collection,
    )?
    .selected_store()?;
    let database = store.directory().join(DATABASE_FILE);

    let worker = || -> anyhow::Result<_> {
        let ControlResponse::Inspection {
            observation: Some(observation),
            ..
        } = service.metadata(
            ControlMethod::Inspection { project },
            Instant::now() + Duration::from_secs(90),
        )
        else {
            anyhow::bail!("native worker inspection missing");
        };
        assert_eq!(observation.worker.project, project);
        assert_eq!(observation.worker.service, window.service_instance);
        assert_eq!(
            observation.worker.fencing_generation,
            observation.current_fencing_generation
        );
        Ok(observation.worker)
    };
    let owner = worker()?;

    let fts_count = |connection: &Connection, path: &str| -> anyhow::Result<i64> {
        Ok(connection.query_row(
            "SELECT count(*) FROM fts WHERE rowid IN (
                SELECT c.id FROM chunks c JOIN files f ON c.file_id=f.id WHERE f.path=?1
            )",
            [path],
            |row| row.get(0),
        )?)
    };
    let inject = || -> anyhow::Result<()> {
        let connection = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        assert_eq!(fts_count(&connection, TARGET)?, 1);
        assert_eq!(fts_count(&connection, CONTROL)?, 1);
        assert_eq!(
            connection.execute(
                "DELETE FROM fts WHERE rowid IN (
                    SELECT c.id FROM chunks c JOIN files f ON c.file_id=f.id WHERE f.path=?1
                )",
                [TARGET],
            )?,
            1
        );
        assert_eq!(fts_count(&connection, TARGET)?, 0);
        assert_eq!(fts_count(&connection, CONTROL)?, 1);
        Ok(())
    };

    let cursor_before = || -> anyhow::Result<EventCursor> {
        let deadline = Instant::now() + Duration::from_secs(90);
        let registry = ServiceRegistry::open_saved_before(&service.control, Some(deadline))?;
        assert!(
            !registry
                .due_automatic_audits(current_unix_ms()?, 32)?
                .iter()
                .any(|(due, _)| *due == project)
        );
        let mut cursor = None;
        loop {
            ensure!(Instant::now() < deadline, "Verify baseline event deadline");
            let page = registry.events(Some(project), cursor.as_ref(), 32)?;
            if let Some(next) = page.next {
                cursor = Some(next);
            } else {
                return Ok(page.tail);
            }
        }
    };
    let wait_for_verify = |mut cursor: EventCursor,
                           old: CurrentToken|
     -> anyhow::Result<CurrentToken> {
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut pending = None;
        loop {
            ensure!(Instant::now() < deadline, "native Verify event deadline");
            let registry = ServiceRegistry::open_saved_before(&service.control, Some(deadline))?;
            let page = registry.events(Some(project), Some(&cursor), 32)?;
            for event in &page.events {
                if event.job_kind != Some(JobKind::Verify) {
                    continue;
                }
                let id = event.job.context("Verify event job ID missing")?;
                match event.kind {
                    EventKind::JobPending => assert!(pending.replace(id).is_none()),
                    EventKind::JobFailed | EventKind::JobCanceled => {
                        anyhow::bail!("automatic Verify failed: {event:?}");
                    }
                    EventKind::JobSucceeded => {
                        assert_eq!(pending, Some(id));
                        assert_eq!(event.fencing_generation, Some(owner.fencing_generation));
                        let job = registry.job(id)?;
                        assert_eq!(job.state, JobState::Succeeded);
                        let successor = store.plain_read()?.token();
                        assert_eq!(successor.incarnation, old.incarnation);
                        assert_eq!(successor.sequence, old.sequence + 1);
                        let Some(JobOutcome::Succeeded {
                            revision: Some(revision),
                        }) = job.outcome
                        else {
                            anyhow::bail!("Verify completion did not retain a revision");
                        };
                        assert_eq!(revision.incarnation, successor.incarnation);
                        assert_eq!(revision.generation, successor.sequence);
                        assert_eq!(revision.revision, successor.digest);
                        let connection = Connection::open_with_flags(
                            &database,
                            OpenFlags::SQLITE_OPEN_READ_ONLY,
                        )?;
                        assert_eq!(fts_count(&connection, TARGET)?, 1);
                        assert_eq!(fts_count(&connection, CONTROL)?, 1);
                        return Ok(successor);
                    }
                    _ => {}
                }
            }
            cursor = page.next.unwrap_or(page.tail);
            std::thread::yield_now();
        }
    };

    let search = |literal: &str,
                  path: &str|
     -> anyhow::Result<(SearchResponse, ScopeObservation)> {
        let started = Instant::now();
        let (response, _) = current_tool(
            &service.control,
            window.tool_selection(),
            "airk_search",
            json!({
                "query": literal, "mode": "lexical", "path_prefix": path,
                "limit": 2, "max_bytes": 4096, "full_chunks": true, "verification": "require",
            }),
        )?;
        assert!(started.elapsed() <= airk_core::read_attempt::MAX_READ_BUDGET);
        let ControlResponse::ToolResult { result } = response else {
            anyhow::bail!(
                "external Window Search refused: {}",
                serde_json::to_string(&response)?
            );
        };
        assert_ne!(result["isError"], true, "{result}");
        let body: Value = serde_json::from_str(
            result["content"][0]["text"]
                .as_str()
                .context("full native Search body missing")?,
        )?;
        let scope: ScopeObservation = serde_json::from_value(body["scope_observation"].clone())?;
        assert_eq!(scope.selection, window.source());
        let [member] = scope.members.as_slice() else {
            anyhow::bail!("Window Search did not retain exactly one index");
        };
        assert_eq!(member.index_id, catalog.index_id);
        assert!(
            member
                .memberships
                .iter()
                .any(|membership| membership.project_id == catalog.project_id)
        );
        Ok((serde_json::from_value(body)?, scope))
    };
    let assert_revision = |reply: &SearchResponse, token: CurrentToken| {
        let publication = reply
            .publication
            .as_ref()
            .expect("Search publication missing");
        assert_eq!(
            (
                publication.incarnation,
                publication.generation,
                publication.revision
            ),
            (token.incarnation, token.sequence, token.digest)
        );
        assert!(!reply.truncated);
    };
    let assert_hit = |reply: &SearchResponse, path: &str, text: &str| {
        let [hit] = reply.hits.as_slice() else {
            panic!("expected exactly one cited Search hit for {path}");
        };
        assert!(!hit.truncated);
        assert_eq!(hit.text, text);
        assert_eq!(hit.citation.path, path);
        assert_eq!(
            hit.citation.content_hash,
            format!("{:x}", Sha256::digest(text.as_bytes()))
        );
        assert_eq!(
            hit.citation.location,
            DocumentLocation::Lines { start: 1, end: 1 }
        );
        assert_eq!((hit.citation.start_line, hit.citation.end_line), (1, 1));
    };

    let first_cursor = cursor_before()?;
    let initial = store.plain_read()?.token();
    inject()?;
    let (deferred, scope) = search("repairneedle", TARGET)?;
    assert_revision(&deferred, initial);
    assert!(deferred.hits.is_empty());
    let warm = wait_for_verify(first_cursor, initial)?;
    let (sample, sample_scope) = search("repairneedle", TARGET)?;
    assert_eq!(sample_scope, scope);
    assert_revision(&sample, warm);
    assert_hit(&sample, TARGET, TARGET_TEXT);

    quiesce()?;
    assert_eq!(worker()?, owner);
    let mut latency_gate = None;
    let physical = match case {
        WindowRepairCase::FirstRequest => {
            let final_cursor = cursor_before()?;
            inject()?;
            let pinned = store.plain_read()?;
            assert_eq!(pinned.token(), warm);
            // This is the only request between fault injection and the verified reply.
            let (first_reply, actual_scope) = search("repairneedle", TARGET)?;
            assert_eq!(actual_scope, scope);
            let physical = store.plain_read()?.token();
            assert_eq!(physical.sequence, warm.sequence + 1);
            assert_revision(&first_reply, physical);
            assert_hit(&first_reply, TARGET, TARGET_TEXT);
            assert_eq!(pinned.token(), warm);
            drop(pinned);
            assert_eq!(wait_for_verify(final_cursor, warm)?, physical);
            physical
        }
        WindowRepairCase::Paused | WindowRepairCase::Concurrent => {
            let pause = |paused: bool| -> anyhow::Result<()> {
                let response =
                    current_native(&service.control, ControlMethod::Pause { project, paused })?;
                ensure!(
                    matches!(response, ControlResponse::Registered { project: saved }
                        if saved.registration == service.project && saved.paused == paused),
                    "native pause acknowledgement did not match the requested project state"
                );
                Ok(())
            };
            let verify_jobs_since = |mut cursor: EventCursor| -> anyhow::Result<_> {
                let until = Instant::now() + Duration::from_secs(90);
                let registry = ServiceRegistry::open_saved_before(&service.control, Some(until))?;
                let mut jobs = std::collections::BTreeSet::new();
                loop {
                    ensure!(Instant::now() < until, "Verify job census deadline");
                    let page = registry.events(Some(project), Some(&cursor), 32)?;
                    for event in page.events {
                        if event.job_kind == Some(JobKind::Verify) {
                            jobs.insert(event.job.context("Verify event job ID missing")?);
                        }
                    }
                    match page.next {
                        Some(next) => cursor = next,
                        None => return Ok(jobs),
                    }
                }
            };
            let final_cursor = cursor_before()?;
            pause(true)?;
            inject()?;
            let pinned = store.plain_read()?;
            assert_eq!(pinned.token(), warm);
            let requests = if matches!(case, WindowRepairCase::Concurrent) {
                let barrier = std::sync::Barrier::new(3);
                let run = || {
                    barrier.wait();
                    let started = Instant::now();
                    let result = search("repairneedle", TARGET);
                    (started, Instant::now(), result)
                };
                std::thread::scope(|threads| -> anyhow::Result<_> {
                    let left = threads.spawn(run);
                    let right = threads.spawn(run);
                    barrier.wait();
                    let left = left
                        .join()
                        .map_err(|_| anyhow::anyhow!("first Window call panicked"))?;
                    let right = right
                        .join()
                        .map_err(|_| anyhow::anyhow!("second Window call panicked"))?;
                    assert!(
                        left.0 < right.1 && right.0 < left.1,
                        "Window call intervals did not overlap"
                    );
                    Ok(vec![left.2?, right.2?])
                })?
            } else {
                vec![search("repairneedle", TARGET)?]
            };
            for (fallback, actual_scope) in requests {
                assert_eq!(actual_scope, scope);
                assert_revision(&fallback, warm);
                assert!(fallback.hits.is_empty());
            }
            assert_eq!(store.plain_read()?.token(), warm);
            assert!(verify_jobs_since(final_cursor.clone())?.is_empty());
            let damaged = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            assert_eq!(fts_count(&damaged, TARGET)?, 0);
            assert_eq!(fts_count(&damaged, CONTROL)?, 1);
            drop(damaged);
            pause(false)?;
            let repaired = wait_for_verify(final_cursor.clone(), warm)?;
            assert_eq!(pinned.token(), warm);
            drop(pinned);
            quiesce()?;
            assert_eq!(verify_jobs_since(final_cursor)?.len(), 1);
            let (recovered, actual_scope) = search("repairneedle", TARGET)?;
            assert_eq!(actual_scope, scope);
            assert_revision(&recovered, repaired);
            assert_hit(&recovered, TARGET, TARGET_TEXT);
            repaired
        }
        WindowRepairCase::Timing => {
            const COUNT: usize = 40;
            const FAULT_SLOTS: [usize; 4] = [5, 16, 25, 36];
            let mut publication = warm;
            let mut matched_healthy = Vec::with_capacity(COUNT);
            let mut ordinary = Vec::with_capacity(COUNT - FAULT_SLOTS.len());
            let mut repair_triggering = Vec::with_capacity(FAULT_SLOTS.len());
            let mut mixed = Vec::with_capacity(COUNT);
            let mut triggering_fixed = 0;
            let mut deferred = 0;
            let measured = || {
                let now = Instant::now();
                let result = search("repairneedle", TARGET);
                (now.elapsed().as_nanos(), result)
            };
            let baseline = || -> anyhow::Result<_> {
                let token = store.plain_read()?.token();
                Ok((token, measured()))
            };
            let mut stress = |fault: bool| -> anyhow::Result<(u128, bool)> {
                if !fault {
                    let (elapsed, result) = measured();
                    let (reply, actual_scope) = result?;
                    assert_eq!(actual_scope, scope);
                    assert_revision(&reply, publication);
                    assert_hit(&reply, TARGET, TARGET_TEXT);
                    return Ok((elapsed, false));
                }
                let cursor = cursor_before()?;
                inject()?;
                let (elapsed, result) = measured();
                let (reply, actual_scope) = result?;
                assert_eq!(actual_scope, scope);
                let old = publication;
                publication = wait_for_verify(cursor, old)?;
                let repaired_on_call = !reply.hits.is_empty();
                if repaired_on_call {
                    assert_revision(&reply, publication);
                    assert_hit(&reply, TARGET, TARGET_TEXT);
                } else {
                    assert_revision(&reply, old);
                }
                Ok((elapsed, repaired_on_call))
            };
            // Alternate request order. Finish each automatic repair outside the timed call.
            for slot in 0..COUNT {
                let fault = FAULT_SLOTS.contains(&slot);
                let (baseline, observed) = if slot % 2 == 0 {
                    (baseline()?, stress(fault)?)
                } else {
                    let observed = stress(fault)?;
                    (baseline()?, observed)
                };
                let (baseline_token, (baseline_time, baseline_reply)) = baseline;
                let (reply, actual_scope) = baseline_reply?;
                assert_eq!(actual_scope, scope);
                assert_revision(&reply, baseline_token);
                assert_hit(&reply, TARGET, TARGET_TEXT);
                matched_healthy.push(baseline_time);
                mixed.push(observed.0);
                if fault {
                    repair_triggering.push(observed.0);
                    triggering_fixed += usize::from(observed.1);
                    deferred += usize::from(!observed.1);
                } else {
                    ordinary.push(observed.0);
                }
            }
            assert_eq!(matched_healthy.len(), COUNT);
            assert_eq!(mixed.len(), COUNT);
            assert_eq!(ordinary.len(), COUNT - FAULT_SLOTS.len());
            assert_eq!(repair_triggering.len(), FAULT_SLOTS.len());
            let p95 = |samples: &[u128]| {
                let mut sorted = samples.to_vec();
                sorted.sort_unstable();
                sorted[(95 * sorted.len()).div_ceil(100) - 1]
            };
            let healthy_p95 = p95(&matched_healthy);
            let mixed_p95 = p95(&mixed);
            let passed = mixed_p95 * 10 <= healthy_p95 * 11;
            println!(
                "{}",
                json!({
                    "scope": "synthetic two-file Windows Window Search stress; no repository qualification",
                    "pair_count": COUNT, "fault_slots_zero_based": FAULT_SLOTS,
                    "healthy_count": matched_healthy.len(), "ordinary_count": ordinary.len(),
                    "repair_trigger_count": repair_triggering.len(), "on_call_repaired": triggering_fixed,
                    "deferred_recovered": deferred, "healthy_p95_ns": healthy_p95,
                    "ordinary_p95_ns": p95(&ordinary), "repair_trigger_p95_ns": p95(&repair_triggering),
                    "mixed_p95_ns": mixed_p95, "p95_threshold_numerator": 11,
                    "p95_threshold_denominator": 10, "passed": passed,
                    "matched_healthy_ns": matched_healthy, "mixed_ns": mixed,
                })
            );
            latency_gate = Some(passed);
            publication
        }
    };

    let (positive, _) = search("controlneedle", CONTROL)?;
    assert_revision(&positive, physical);
    assert_hit(&positive, CONTROL, CONTROL_TEXT);
    let (negative, _) = search("unmatchedneedle", TARGET)?;
    assert_revision(&negative, physical);
    assert!(negative.hits.is_empty());
    assert_eq!(fs::read(root.path().join(TARGET))?, TARGET_TEXT.as_bytes());
    assert_eq!(
        fs::read(root.path().join(CONTROL))?,
        CONTROL_TEXT.as_bytes()
    );
    assert_eq!(worker()?, owner);
    service.shutdown_reviewed()?;
    ensure!(
        latency_gate != Some(false),
        "synthetic mixed p95 exceeded 1.10× matched healthy p95"
    );
    Ok(())
}

#[test]
fn control_tool_calls_answer_what_the_cli_printed() -> anyhow::Result<()> {
    use airk_core::service::ToolSelection;
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "tools-call");
    fs::write(
        root.path().join("recovery.md"),
        "# Recovery\n\nA forgotten password is recovered from the account page.\n",
    )?;
    current_quiescent(
        &service,
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let window = service.window()?;
    let binding = service.project.binding.to_string();
    let set = service.reviewed_scope().project_id.to_string();
    let calls = [
        (
            "airk_read_file",
            json!({"path":"recovery.md","start_line":1,"end_line":3,"verification":"off"}),
        ),
        (
            "airk_file_info",
            json!({"path":"recovery.md","verification":"off"}),
        ),
        ("airk_list_files", json!({"verification":"off"})),
        (
            "airk_search",
            json!({"query":"forgotten password","limit":3,"verification":"off"}),
        ),
        (
            "airk_outline",
            json!({"path":"missing.md","verification":"off"}),
        ),
        ("help", json!({"query":"workbench"})),
    ];
    let mut positive = 0;
    for (name, native, flags) in [
        // The `airkctl session` route: reads name no selection and go through
        // the window's W4 grant over the fixture's Index and Project.
        ("window", window.tool_selection(), vec![]),
        (
            "project",
            ToolSelection::Project {
                binding: service.project.binding,
            },
            vec!["--service-project", binding.as_str()],
        ),
        (
            "open index set",
            ToolSelection::Open {
                index_ids: vec![],
                index_set_ids: vec![set.clone()],
                workspace: None,
            },
            vec![],
        ),
    ] {
        for (tool, arguments) in &calls {
            let help = matches!(*tool, "help" | "airk_help");
            // airkctl answers help in process for an unselected call; a
            // session-routed help names no selection.
            let selected = if name == "window" && help {
                ToolSelection::Unselected
            } else {
                native.clone()
            };
            let expected = current_tool(&service.control, selected, tool, arguments.clone())?;
            let text = arguments.to_string();
            let mut line = vec!["tools", "call", *tool, "--args", text.as_str()];
            if name == "open index set" {
                line.extend(["--set", set.as_str()]);
            }
            let routed = (name == "window").then_some(&window);
            let accepted = current_assert_tool(
                expected,
                current_ctl_routed(&service.control, routed, &flags, &line)?,
                help,
                &format!("{name} {tool}"),
            )?;
            if name != "project" && *tool != "help" {
                positive += usize::from(accepted);
            }
        }
    }
    assert!(
        positive >= 8,
        "window and readable index-set authority must return real indexed evidence"
    );
    for arguments in [
        json!({"query":"workbench"}),
        json!({"query":"workbench","detail":"full"}),
    ] {
        let text = arguments.to_string();
        current_assert_tool(
            current_tool(
                &service.control,
                ToolSelection::Unselected,
                "help",
                arguments,
            )?,
            current_ctl(
                &service.control,
                &[],
                &["tools", "call", "help", "--args", &text],
            )?,
            true,
            "unselected help",
        )?;
    }
    Ok(())
}

/// `airkctl` evidence reads and tool calls print what `airk` printed, for both
/// selections, on the reviewed indexed fixture (split step 3a.2c).
#[test]
fn airkctl_reads_print_what_airk_printed() -> anyhow::Result<()> {
    use airk_core::service::ToolSelection;
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-reads");
    fs::write(
        root.path().join("recovery.md"),
        "# Recovery\n\nA forgotten password is recovered from the account page.\n",
    )?;
    fs::write(
        root.path().join("account.rs"),
        "use std::fmt;\n\npub fn recover_password(user: &str) -> String {\n    format!(\"reset {user}\")\n}\n",
    )?;
    current_quiescent(
        &service,
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let window = service.window()?;
    let arguments_file = state.path().join("ctl-args.json");
    fs::write(
        &arguments_file,
        r#"{"query":"recover","limit":2,"verification":"off"}"#,
    )?;
    let arguments_file = arguments_file.to_str().unwrap();
    // Independent CLI-to-native input catalog. Do not derive it from the CLI mapper.
    let reads: Vec<(Vec<&str>, &str, Value)> = vec![
        (vec!["status"], "airk_status", json!({})),
        (
            vec!["parse", "where is the password recovered"],
            "airk_parse",
            json!({"schema":"airk.parse.request/1","text":"where is the password recovered","mode":"prepare","path_prefix":""}),
        ),
        (
            vec!["search", "forgotten password", "--limit", "3"],
            "airk_search",
            json!({"query":"forgotten password","mode":null,"path_prefix":null,"limit":3,"max_bytes":8192}),
        ),
        (
            vec![
                "search",
                "recover",
                "--mode",
                "lexical",
                "--language",
                "rust",
            ],
            "airk_search",
            json!({"query":"recover","mode":"lexical","language":"rust","path_prefix":null,"limit":10,"max_bytes":8192}),
        ),
        (
            vec!["files"],
            "airk_list_files",
            json!({"cursor":null,"prefix":"","limit":10,"max_bytes":8192}),
        ),
        (
            vec![
                "read",
                "recovery.md",
                "--start-line",
                "1",
                "--end-line",
                "2",
            ],
            "airk_read_file",
            json!({"path":"recovery.md","start_line":1,"end_line":2,"max_bytes":8192}),
        ),
        (
            vec!["info", "account.rs"],
            "airk_file_info",
            json!({"path":"account.rs"}),
        ),
        (
            vec!["outline", "account.rs"],
            "airk_outline",
            json!({"path":"account.rs","cursor":null,"compact":null,"limit":50,"max_bytes":8192}),
        ),
        (
            vec!["symbol", "recover_password"],
            "airk_symbols",
            json!({"compact":null,"cursor":null,"name":"recover_password","language":null,"path_prefix":"","limit":10,"max_bytes":8192}),
        ),
        (
            vec!["imports"],
            "airk_imports",
            json!({"compact":null,"cursor":null,"path_prefix":""}),
        ),
        (
            vec!["facts"],
            "airk_document_facts",
            json!({"compact":null,"cursor":null,"document_key":null,"path_prefix":"","name":null,"kind":null,"limit":50,"max_bytes":8192}),
        ),
        (
            vec!["code-facts", "--name", "recover_password"],
            "airk_code_facts",
            json!({"compact":null,"cursor":null,"symbol_key":null,"path_prefix":"","name":"recover_password","kind":null,"language":null,"limit":10,"max_bytes":8192}),
        ),
        (
            vec!["relationships", "recover_password"],
            "airk_relationships",
            json!({"symbol":"recover_password","direction":"incoming","kind":null,"path_prefix":"","hops":1,"limit":10,"max_bytes":8192}),
        ),
        (
            vec!["refs", "recover_password"],
            "airk_references",
            json!({"name":"recover_password","path_prefix":"","limit":10,"max_bytes":8192}),
        ),
        (
            vec![
                "context",
                "account.rs",
                "3",
                "--before",
                "1",
                "--after",
                "1",
            ],
            "airk_context",
            json!({"path":"account.rs","line":3,"before":1,"after":1,"max_bytes":8192}),
        ),
        (
            vec!["changes"],
            "airk_changes",
            json!({"since":null,"selectors":[],"path_prefix":"","include_symbols":false,"include_documents":false,"include_suggestions":false,"limit":10,"max_bytes":8192,"cursor":null}),
        ),
        (
            vec!["read", "missing.md", "--start-line", "1", "--end-line", "2"],
            "airk_read_file",
            json!({"path":"missing.md","start_line":1,"end_line":2,"max_bytes":8192}),
        ),
        (
            vec!["tools", "call", "airk_status"],
            "airk_status",
            json!({}),
        ),
        (
            vec![
                "tools",
                "call",
                "airk_search",
                "--args-file",
                arguments_file,
            ],
            "airk_search",
            json!({"query":"recover","limit":2,"verification":"off"}),
        ),
        (
            vec![
                "tools",
                "call",
                "help",
                "--args",
                r#"{"query":"workbench"}"#,
            ],
            "help",
            json!({"query":"workbench"}),
        ),
    ];
    let binding = service.project.binding.to_string();
    let mut compared = 0;
    let mut accepted = 0;
    for (native, routed, flags) in [
        (window.tool_selection(), Some(&window), vec![]),
        (
            ToolSelection::Project {
                binding: service.project.binding,
            },
            None,
            vec!["--service-project", binding.as_str()],
        ),
    ] {
        for (line, tool, arguments) in &reads {
            let mut line = line.clone();
            let mut arguments = arguments.clone();
            if line[0] != "tools" && *tool != "airk_status" {
                line.extend(["--verification", "off"]);
                arguments["verification"] = json!("off");
            }
            if line[0] != "tools"
                && matches!(
                    *tool,
                    "airk_read_file" | "airk_file_info" | "airk_outline" | "airk_context"
                )
            {
                // The CLI's EvidenceOptions keeps these explicit default fields.
                // Pin the full input, including its encoded budget cost.
                arguments.as_object_mut().unwrap().extend(
                    json!({
                        "revision":null,"if_evidence":null,"follow_moves":false,
                        "known_hash":null,"artifact_id":null
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                );
            }
            let help = matches!(*tool, "help" | "airk_help");
            // airkctl answers help in process for an unselected call; a
            // session-routed help names no selection.
            let selected = if routed.is_some() && help {
                ToolSelection::Unselected
            } else {
                native.clone()
            };
            let label = if routed.is_some() {
                "window"
            } else {
                "project"
            };
            accepted += usize::from(current_assert_tool(
                current_tool(&service.control, selected, tool, arguments)?,
                current_ctl_routed(&service.control, routed, &flags, &line)?,
                help,
                &format!("{label} {line:?}"),
            )?);
            compared += 1;
        }
    }
    assert_eq!(compared, 40, "complete independent read/input catalog");
    assert!(
        accepted >= 18,
        "successful indexed results must accompany refusal cases"
    );
    Ok(())
}

/// Without a binding or session, `airkctl` resolves the project its
/// `--root`/`--data-dir`/`--collection` name through M1 and reads it as
/// `airk` read the bound project (split step 3c.1).
#[test]
fn airkctl_resolves_the_project_its_folder_names() -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse, ResolveTarget, ToolSelection};
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-resolve");
    fs::write(
        root.path().join("notes.md"),
        "# Notes\n\nResolved by folder.\n",
    )?;
    current_quiescent(
        &service,
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let target = ResolveTarget::Registration {
        root: service.project.root.clone(),
        data_root: service.project.data_root.clone(),
        collection: "ctl-resolve".into(),
    };
    let ControlResponse::Resolved { registration } =
        current_native(&service.control, ControlMethod::ResolveProject { target })?
    else {
        panic!("canonical folder resolution failed");
    };
    assert_eq!(
        serde_json::to_value(&registration)?,
        serde_json::to_value(&service.project)?
    );
    let flags = [
        "--root",
        registration.root.as_str(),
        "--data-dir",
        registration.data_root.as_str(),
        "--collection",
        "ctl-resolve",
    ];
    for (line, tool, arguments) in [
        (vec!["status"], "airk_status", json!({})),
        (
            vec!["files", "--verification", "off"],
            "airk_list_files",
            json!({"cursor":null,"prefix":"","limit":10,"max_bytes":8192,"verification":"off"}),
        ),
        (
            vec![
                "read",
                "notes.md",
                "--start-line",
                "1",
                "--end-line",
                "3",
                "--verification",
                "off",
            ],
            "airk_read_file",
            json!({"path":"notes.md","start_line":1,"end_line":3,"max_bytes":8192,"verification":"off"}),
        ),
    ] {
        let mut arguments = arguments;
        if tool == "airk_read_file" {
            arguments.as_object_mut().unwrap().extend(
                json!({
                    "revision":null,"if_evidence":null,"follow_moves":false,
                    "known_hash":null,"artifact_id":null
                })
                .as_object()
                .unwrap()
                .clone(),
            );
        }
        current_assert_tool(
            current_tool(
                &service.control,
                ToolSelection::Project {
                    binding: registration.binding,
                },
                tool,
                arguments,
            )?,
            current_ctl(&service.control, &flags, &line)?,
            false,
            &format!("folder resolution {line:?}"),
        )?;
    }
    let elsewhere = tempfile::tempdir()?;
    let missing = elsewhere.path().to_str().unwrap();
    let expected = current_native(
        &service.control,
        ControlMethod::ResolveProject {
            target: ResolveTarget::Registration {
                root: missing.into(),
                data_root: registration.data_root.clone(),
                collection: "ctl-resolve".into(),
            },
        },
    )?;
    assert_eq!(
        serde_json::to_value(expected)?,
        json!({"status":"failure","code":"project_not_registered"}),
        "native registration lookup must preserve its current M1 refusal"
    );
    let actual = current_ctl(
        &service.control,
        &[
            "--root",
            missing,
            "--data-dir",
            &registration.data_root,
            "--collection",
            "ctl-resolve",
        ],
        &["status"],
    )?;
    assert_eq!(actual.status.code(), Some(1));
    assert!(actual.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&actual.stderr)?,
        json!({
            "airkctl_error":1,
            "class":"refused",
            "code":"project_not_registered",
            "retryable":false,
            "message":"project is not registered with this service; review registration before indexing or connecting"
        }),
        "complete I2 registration refusal has no native field"
    );
    Ok(())
}

/// `airkctl pause|resume|watch|projects` print what `airk` printed for the
/// same project (split step 3c.2). Each `airk` call precedes the `airkctl`
/// one, so both observe the same state.
#[test]
fn airkctl_project_commands_print_what_airk_printed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-admin");
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let control = service.control.to_str().unwrap().to_owned();
    let binding = service.project.binding.to_string();
    let mut parity = Parity::new("airkctl_project_commands_print_what_airk_printed");
    // The default project name is the folder's name.
    parity.substitute(
        root.path().file_name().unwrap().to_string_lossy(),
        "<ROOT_NAME>",
    );
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    for command in [
        vec!["pause"],
        vec!["resume"],
        vec!["watch", "off"],
        vec!["watch", "on"],
        vec!["projects", "--runtime-dir", &control],
    ] {
        let name = parity.name(&command.join(" "));
        let old = parity.old(&name, || {
            common::legacy_airk_command(root.path(), state.path(), "ctl-admin")
                .args(&command)
                .output()
                .unwrap()
        });
        let mut line = vec![
            "airkctl",
            "--service-runtime-dir",
            &control,
            "--service-project",
            &binding,
        ];
        line.extend(command.iter().copied());
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: None,
        };
        let code = airkctl::cli::run(line.into_iter().map(Into::into).collect(), &mut io);
        assert_eq!(
            (
                Some(i32::from(code)),
                parity.canonical(&String::from_utf8(stdout)?)
            ),
            (old.code, old.stdout),
            "{command:?}: {}",
            String::from_utf8_lossy(&stderr)
        );
    }
    parity.finish();
    Ok(())
}

/// M5-M8 on a live service follow the journal rules of contracts §2.4
/// (split step 3c.3b): duplicate, conflict, expiry, key mismatch and capacity
/// for M5; listing; key-idempotent submission; forgetting.
#[test]
fn the_request_journal_follows_its_contract_rules() -> anyhow::Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::{ControlMethod as Method, ControlResponse as Response, JobKind, RequestKey},
    };
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-journal");
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let project = service.project.binding;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let request = |method| {
        runtime
            .block_on(airk_cli::service_runtime::request(&service.control, method))
            .unwrap()
    };
    let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch) else {
        panic!("no request epoch");
    };
    let id = |byte: u8| Incarnation::from_bytes([byte; 16]);
    let key = |nonce: Incarnation, epoch: u64| RequestKey {
        namespace,
        epoch,
        nonce,
    };
    let prepare =
        |caller: Incarnation, key: RequestKey, kind: JobKind| Method::ClientRequestPrepare {
            caller,
            key: key.clone(),
            request: Box::new(Method::Enqueue {
                project,
                caller,
                key,
                kind,
            }),
        };
    let code = |response: Response| match response {
        Response::Failure { code } => code,
        other => panic!(
            "expected a refusal: {}",
            serde_json::to_string(&other).unwrap()
        ),
    };
    let (caller, nonce) = (id(0x0c), id(0x0b));
    let first = request(prepare(caller, key(nonce, epoch), JobKind::Index));
    assert!(matches!(
        first,
        Response::ClientRequestPrepared {
            duplicate: false,
            ..
        }
    ));
    let again = request(prepare(caller, key(nonce, epoch), JobKind::Index));
    assert!(
        matches!(again, Response::ClientRequestPrepared { duplicate: true, nonce: n, .. } if n == nonce)
    );
    assert_eq!(
        code(request(prepare(
            caller,
            key(nonce, epoch),
            JobKind::Inspect
        ))),
        "client_request_key_conflict"
    );
    assert_eq!(
        code(request(prepare(
            caller,
            key(id(0x1b), epoch + 1000),
            JobKind::Index
        ))),
        "client_request_key_expired"
    );
    let mismatched = Method::ClientRequestPrepare {
        caller: id(0x0d),
        key: key(id(0x2b), epoch),
        request: Box::new(Method::Enqueue {
            project,
            caller,
            key: key(id(0x2b), epoch),
            kind: JobKind::Index,
        }),
    };
    assert_eq!(code(request(mismatched)), "client_request_key_mismatch");
    let Response::ClientRequests { pending, capacity } = request(Method::ClientRequestList {})
    else {
        panic!("no journal list");
    };
    assert_eq!(capacity, 64);
    assert_eq!(pending.len(), 1);
    assert_eq!((pending[0].nonce, pending[0].kind), (nonce, JobKind::Index));
    let Response::Job { job } = request(Method::ClientRequestSubmit { nonce }) else {
        panic!("submission refused");
    };
    let Response::Job { job: replayed } = request(Method::ClientRequestSubmit { nonce }) else {
        panic!("resubmission refused");
    };
    assert_eq!((job.id, job.kind), (replayed.id, JobKind::Index));
    for filler in 1..64u8 {
        let filled = request(prepare(
            caller,
            key(id(0x40 + filler), epoch),
            JobKind::Inspect,
        ));
        assert!(
            matches!(
                filled,
                Response::ClientRequestPrepared {
                    duplicate: false,
                    ..
                }
            ),
            "{filler}"
        );
    }
    assert_eq!(
        code(request(prepare(
            caller,
            key(id(0x3f), epoch),
            JobKind::Inspect
        ))),
        "client_request_journal_full"
    );
    assert!(matches!(
        request(Method::ClientRequestForget { nonce }),
        Response::ClientRequestForgotten { existed: true, .. }
    ));
    assert!(matches!(
        request(Method::ClientRequestForget { nonce }),
        Response::ClientRequestForgotten { existed: false, .. }
    ));
    Ok(())
}

/// `airkctl index|inspect` and `service requests …` run the journal as
/// `airk` did (split step 3c.3c): the same job kind and project, the same
/// guidance line (nonce normalized, `airk` → `airkctl`), the same listing.
#[test]
fn airkctl_jobs_and_requests_match_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-jobs");
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let control = service.control.to_str().unwrap().to_owned();
    let binding = service.project.binding.to_string();
    let ctl = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let mut parity = Parity::new("airkctl_jobs_and_requests_match_airk");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // A saved request's caller and key carry this run's journal namespace and
    // epoch (the recorded run also spent epochs on its own `airk` jobs).
    parity.mask_json_fields(&["caller", "namespace", "epoch"]);
    for command in ["inspect", "index"] {
        let old = parity.old(command, || {
            common::legacy_airk_command(root.path(), state.path(), "ctl-jobs")
                .arg(command)
                .output()
                .unwrap()
        });
        assert!(old.success, "{}", old.stderr);
        let (code, stdout, stderr) = ctl(&[
            "--service-runtime-dir",
            &control,
            "--service-project",
            &binding,
            command,
        ]);
        assert_eq!(code, 0, "{stderr}");
        let old_job: Value = serde_json::from_str(&old.stdout)?;
        let new_job: Value = serde_json::from_str(&parity.canonical(&stdout))?;
        assert_eq!(new_job["status"], "job");
        assert_eq!(
            (&new_job["job"]["project"], &new_job["job"]["kind"]),
            (&old_job["job"]["project"], &old_job["job"]["kind"]),
            "{command}"
        );
        // The request nonce is a renamed hex token in canonical space.
        let old_line = old
            .stderr
            .lines()
            .last()
            .unwrap_or_default()
            .replace("'airk service", "'airkctl service");
        assert_eq!(
            parity.canonical(&stderr).lines().last().unwrap_or_default(),
            old_line,
            "{command}"
        );
    }
    let (code, prepared, stderr) = ctl(&[
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "prepare",
        "--project",
        &binding,
        "--kind",
        "inspect",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let prepared: Value = serde_json::from_str(&prepared)?;
    assert_eq!(prepared["submitted"], false);
    let saved = prepared["prepared"].as_str().unwrap().to_owned();
    parity.substitute(&saved, "<PREPARED>");
    let old_list = parity.old("requests list", || {
        command_in(
            &control,
            &["service", "requests", "--runtime-dir", &control, "list"],
        )
    });
    assert!(old_list.success, "{}", old_list.stderr);
    let old_list = old_list.stdout;
    let (code, new_list, _) = ctl(&["service", "requests", "--runtime-dir", &control, "list"]);
    assert_eq!(
        (code, parity.canonical(&new_list).as_str()),
        (0, old_list.as_str())
    );
    assert!(new_list.contains(&saved), "{new_list}");
    let (code, retried, stderr) = ctl(&[
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "retry",
        &saved,
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&retried)?["job"]["kind"],
        "inspect"
    );
    let (code, forgotten, _) = ctl(&[
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "forget",
        &saved,
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        serde_json::from_str::<Value>(&forgotten)?,
        json!({"forgotten": saved, "native_work_canceled": false})
    );
    let (_, emptied, _) = ctl(&["service", "requests", "--runtime-dir", &control, "list"]);
    assert_eq!(emptied.trim(), "[]");
    parity.finish();
    Ok(())
}

/// `airk` with only these arguments.
fn command_in(_control: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_airk"))
        .args(arguments)
        .output()
        .unwrap()
}

/// The last stderr line without its program prefix (`Error: ` for `airk`,
/// `airkctl: ` for `airkctl`): the message both print for a refusal.
fn refusal_message(stderr: &[u8], prefix: &str) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text.lines().last().unwrap_or_default();
    line.strip_prefix(prefix).unwrap_or(line).to_owned()
}

/// `init`, `service register`, `service prepare-registration` and `service
/// prepare-storage` (split step 3c.4b): `airkctl` prints what `airk` printed,
/// through M4a/M4b and the existing `ReserveProject` and `Register`.
#[test]
fn airkctl_registration_matches_airk() -> anyhow::Result<()> {
    use airkctl::registration::Init;
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.md"),
        b"# Registration parity\n",
    )?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-init");
    let control = service.control.to_str().unwrap().to_owned();
    let version = env!("CARGO_PKG_VERSION");
    let ctl = |arguments: &[&str]| -> (u8, String, Vec<u8>) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (code, String::from_utf8(stdout).unwrap(), stderr)
    };
    let old = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_airk"))
            .args(arguments)
            .output()
            .unwrap()
    };
    let unreviewed = tempfile::tempdir()?;
    let absent = tempfile::tempdir()?;
    let fresh = tempfile::tempdir()?;
    let fresh_parent = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let other_data = tempfile::tempdir()?;
    let mut parity = Parity::new("airkctl_registration_matches_airk");
    parity.substitute_path(source.path(), "<SOURCE>");
    parity.substitute_path(data.path(), "<DATA>");
    parity.substitute_path(unreviewed.path(), "<UNREVIEWED>");
    parity.substitute_path(absent.path(), "<ABSENT>");
    parity.substitute_path(fresh.path(), "<FRESH>");
    parity.substitute_path(fresh_parent.path(), "<FRESH_PARENT>");
    parity.substitute_path(other.path(), "<OTHER>");
    parity.substitute_path(other_data.path(), "<OTHER_DATA>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);

    // The reviewed service admits the project's first worker (fencing
    // generation 1) asynchronously after it starts, and `init` answers the
    // registered project with that generation. The recorded old output was
    // captured after the admission. Compare both sides in that settled state
    // (change-log entry 68 part 5): wait for it under a deadline, once.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let airk_core::service::ControlResponse::Projects { page } =
            service.request(airk_core::service::ControlMethod::Projects {
                after: None,
                revision: None,
                limit: 32,
            })
        else {
            anyhow::bail!("the service did not list its projects");
        };
        let generation = page
            .projects
            .iter()
            .find(|project| project.registration.binding == service.project.binding)
            .map(|project| project.fencing_generation);
        if generation.is_some_and(|generation| generation >= 1) {
            break;
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "the reviewed project's first worker was never admitted: {generation:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // init on the reviewed mapping: the same acknowledged project.
    let initialized = parity.old("init reviewed", || {
        service
            .installed_command(source.path(), data.path(), "ctl-init")
            .arg("init")
            .output()
            .unwrap()
    });
    assert!(initialized.success, "{}", initialized.stderr);
    let Init::Registered(project) = airkctl::registration::init(
        &service.control,
        version,
        source.path(),
        data.path(),
        "ctl-init",
        None,
    )
    .map_err(|line| anyhow::anyhow!(line.message))?
    else {
        anyhow::bail!("airkctl init was not acknowledged");
    };
    assert_eq!(
        parity.canonical(&(serde_json::to_string(&project)? + "\n")),
        initialized.stdout
    );
    assert_eq!(project.registration, service.project);

    // init on an unmapped folder: the same refusal; nothing is created.
    let absent_data = absent.path().join("not-created/nested");
    fs::write(unreviewed.path().join("original.md"), b"# Unreviewed\n")?;
    let rejected = parity.old("init unmapped", || {
        service
            .installed_command(unreviewed.path(), &absent_data, "ctl-unmapped")
            .arg("init")
            .output()
            .unwrap()
    });
    assert!(!rejected.success);
    match airkctl::registration::init(
        &service.control,
        version,
        unreviewed.path(),
        &absent_data,
        "ctl-unmapped",
        None,
    ) {
        Ok(Init::NotAcknowledged(response)) => assert_eq!(
            parity.canonical(&(serde_json::to_string(&response)? + "\n")),
            rejected.stdout
        ),
        Ok(Init::Registered(_)) => anyhow::bail!("airkctl init registered an unmapped folder"),
        Err(line) => assert_eq!(
            parity.canonical(&line.message),
            refusal_message(rejected.stderr.as_bytes(), "Error: ")
        ),
    }
    assert!(!absent.path().join("not-created").exists());

    // prepare-registration: offline in `airk`, M4a in `airkctl`.
    let planned = fresh_parent.path().join("planned");
    let inner = fresh.path().join("inner");
    for (case, data_dir, collection) in [
        ("planned", Some(planned.to_str().unwrap()), "ctl-plan"),
        ("inner", Some(inner.to_str().unwrap()), "ctl-plan"),
        ("default", None, "ctl-plan"),
    ] {
        let mut arguments = vec!["--root", fresh.path().to_str().unwrap()];
        if let Some(data_dir) = data_dir {
            arguments.extend(["--data-dir", data_dir]);
        }
        arguments.extend([
            "--collection",
            collection,
            "service",
            "prepare-registration",
        ]);
        let before = parity.old(&format!("prepare-registration {case}"), || old(&arguments));
        let (code, stdout, stderr) = ctl(&[
            &["--service-runtime-dir", control.as_str()][..],
            &arguments[..],
        ]
        .concat());
        assert_eq!(
            (code == 0, parity.canonical(&stdout)),
            (before.success, before.stdout),
            "{arguments:?}"
        );
        if code != 0 {
            assert_eq!(
                refusal_message(
                    parity
                        .canonical(&String::from_utf8_lossy(&stderr))
                        .as_bytes(),
                    "airkctl: "
                ),
                refusal_message(before.stderr.as_bytes(), "Error: "),
                "{arguments:?}"
            );
        }
    }
    assert!(!planned.exists());

    // register: storage must exist; the service's answer is printed alike.
    let register = [
        "--root",
        other.path().to_str().unwrap(),
        "--data-dir",
        other_data.path().to_str().unwrap(),
        "--collection",
        "ctl-register",
        "service",
        "register",
        "--runtime-dir",
        &control,
    ];
    let before = parity.old("register", || old(&register));
    let (code, stdout, stderr) = ctl(&register);
    assert_eq!(
        (code == 0, parity.canonical(&stdout)),
        (before.success, before.stdout),
        "{}",
        String::from_utf8_lossy(&stderr)
    );

    // prepare-storage: the registry's registration, or the same refusal.
    let unknown = airkctl::registration::plan(
        &service.control,
        version,
        fresh.path(),
        &planned,
        "ctl-plan",
        airk_core::service::RegistrationStorage::Planned,
    )
    .map_err(|line| anyhow::anyhow!(line.message))?;
    parity.substitute(unknown.binding, "<PLANNED_BINDING>");
    for (case, binding) in [
        ("reviewed", service.project.binding),
        ("planned", unknown.binding),
    ] {
        let binding = binding.to_string();
        let arguments = [
            "service",
            "prepare-storage",
            "--runtime-dir",
            &control,
            "--project",
            &binding,
        ];
        let before = parity.old(&format!("prepare-storage {case}"), || old(&arguments));
        let (code, stdout, stderr) = ctl(&arguments);
        assert_eq!(
            (code == 0, parity.canonical(&stdout)),
            (before.success, before.stdout),
            "{binding}"
        );
        if code != 0 {
            assert_eq!(
                refusal_message(
                    parity
                        .canonical(&String::from_utf8_lossy(&stderr))
                        .as_bytes(),
                    "airkctl: "
                ),
                refusal_message(before.stderr.as_bytes(), "Error: "),
                "{binding}"
            );
        }
    }
    assert!(!planned.exists());
    service.shutdown_reviewed()?;
    parity.finish();
    Ok(())
}

/// `tools shell` (split step 3c.4c): the same console input through `airk`
/// and `airkctl`, by project binding, prints the same
/// banner, help, prompts, results and `Error:` lines.
#[test]
fn airkctl_tools_shell_matches_airk() -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse, ToolSelection, ToolsCatalogDetail};
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-shell");
    fs::write(
        root.path().join("recovery.md"),
        "# Recovery\n\nRecover a forgotten password.\n",
    )?;
    current_quiescent(
        &service,
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let arguments_file = state.path().join("shell-args.json");
    fs::write(
        &arguments_file,
        r#"{"query":"recover","limit":2,"verification":"off"}"#,
    )?;
    let calls = [
        ("airk_status", json!({})),
        (
            "airk_read_file",
            json!({"path":"recovery.md","start_line":1,"end_line":3,"verification":"off"}),
        ),
        (
            "airk_search",
            json!({"query":"recover","limit":2,"verification":"off"}),
        ),
        (
            "airk_search",
            json!({"query":"recover","limit":2,"verification":"off"}),
        ),
        (
            "airk_read_file",
            json!({"path":"missing.md","start_line":1,"end_line":3,"verification":"off"}),
        ),
    ];
    let script = format!(
        "list\nhelp airk_search\ncall airk_status\ncall airk_read_file {}\ncall airk_search {}\ncall airk_search --args-file \"{}\"\ncall airk_read_file {}\ncall airk_search [1]\nbogus\nexit\ncall airk_status\n",
        calls[1].1,
        calls[2].1,
        arguments_file.display(),
        calls[4].1
    );
    // `tools shell` has only its project form now; the window (session)
    // route serves `tools call`, not the console.
    let binding = service.project.binding.to_string();
    let ControlResponse::ToolsCatalog { document } = current_native(
        &service.control,
        ControlMethod::ToolsCatalog {
            detail: ToolsCatalogDetail::List { schemas: true },
        },
    )?
    else {
        panic!("native tool catalog refused");
    };
    let native_catalog: Value = serde_json::from_str(&document)?;
    let search = native_catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "airk_search" || tool["name"] == "search")
        .unwrap();
    for (name, native, flags) in [(
        "project",
        ToolSelection::Project {
            binding: service.project.binding,
        },
        vec!["--service-project", binding.as_str()],
    )] {
        let started = current_unix_ms()?;
        let mut expected = Vec::new();
        let mut errors = String::new();
        for (tool, args) in &calls {
            match current_tool(&service.control, native.clone(), tool, args.clone())?.0 {
                ControlResponse::ToolResult { result } => {
                    let failed = result["isError"] == true;
                    for content in result["content"].as_array().unwrap() {
                        let shown = if content["type"] == "text" {
                            serde_json::from_str::<Value>(content["text"].as_str().unwrap())
                                .unwrap_or_else(|_| content["text"].clone())
                        } else {
                            content.clone()
                        };
                        expected.push((failed, shown));
                    }
                    if failed {
                        errors.push_str("Error: MCP tool failed; see the result above\n");
                    }
                }
                ControlResponse::ToolCallFailed { message } => {
                    errors.push_str(&format!("Error: {message}\n"))
                }
                other => errors.push_str(&format!(
                    "Error: {}\n",
                    airkctl::service::refusal(&other).message
                )),
            }
        }
        errors.push_str("Error: MCP arguments must be a JSON object\nError: Unknown command. Type help for instructions.\n");
        let actual = common::ctl::Ctl::new()
            .arg("--service-runtime-dir")
            .arg(&service.control)
            .args(&flags)
            .args(["tools", "shell"])
            .env("AIRK_MCP_RESPONSE_MODE", "full")
            .input(script.as_bytes().to_vec())
            .output()?;
        assert!(
            actual.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(
            String::from_utf8(actual.stderr)?,
            errors,
            "{name}: complete shell failure stream"
        );
        let finished = current_unix_ms()?;
        let expected = expected
            .into_iter()
            .map(|(failed, body)| Ok((failed, current_observation(body, started, finished)?)))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let stdout = String::from_utf8(actual.stdout)?;
        let banner = format!(
            "AIRK Tools\nCollection: {:?}\nWorkspace: {:?}\nStorage: {:?}\n\n",
            service.project.collection,
            Path::new(&service.project.root),
            Path::new(&service.project.data_root)
        );
        assert!(stdout.starts_with(&banner), "{name}: exact resolved banner");
        for tool in native_catalog["tools"].as_array().unwrap() {
            assert!(
                stdout.contains(&format!(
                    "{}\n  {}\n",
                    tool["name"].as_str().unwrap(),
                    tool["description"].as_str().unwrap_or_default()
                )),
                "{name}: omitted catalog row"
            );
        }
        assert!(
            stdout.contains(&serde_json::to_string_pretty(&search["inputSchema"])?),
            "{name}: complete native search input schema"
        );
        let mut observed = Vec::new();
        for segment in stdout.split("airk> ").skip(1) {
            let (failed, body) = if let Some(body) = segment.strip_prefix("Result:\n") {
                (false, body)
            } else if let Some(body) = segment.strip_prefix("MCP error:\n") {
                (true, body)
            } else {
                continue;
            };
            let mut values = serde_json::Deserializer::from_str(body).into_iter::<Value>();
            observed.push((
                failed,
                current_observation(
                    values.next().expect("complete shell JSON body")?,
                    started,
                    finished,
                )?,
            ));
            assert!(
                values.next().is_none(),
                "{name}: shell omitted or added native content"
            );
        }
        assert_eq!(
            observed, expected,
            "{name}: every complete native result body, stable digest bytes retained"
        );
        assert_eq!(
            stdout.matches("airk> ").count(),
            10,
            "exit must leave the trailing call unread"
        );
    }
    Ok(())
}

/// `configure`, the `usage` mutations, `usage import --preview` and both
/// `prepare-configuration` commands (split step 3c.5b): `airkctl` prints what
/// `airk` printed through M9 and the journal (jobs compared by project and
/// kind; nonces normalized).
#[test]
fn airkctl_configuration_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-config");
    // Configuration requires the project's first inspection: wait for the
    // fixture's initial Index work (observed, deadline-bounded) before the
    // first prepare-configuration.
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let control = service.control.to_str().unwrap().to_owned();
    let binding = service.project.binding.to_string();
    let ctl = |arguments: &[&str], stdin: &str| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut input = stdin.as_bytes();
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut input,
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain([
                "--service-runtime-dir",
                &control,
                "--service-project",
                &binding,
            ])
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let measurement = state.path().join("measurement.json");
    let draft_file = state.path().join("draft.json");
    let mut parity = Parity::new("airkctl_configuration_matches_airk");
    parity.substitute_path(&measurement, "<MEASUREMENT>");
    parity.substitute_path(&draft_file, "<DRAFT_FILE>");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // The recorded `airk` run (split U95): new output is masked alike.
    let old = |parity: &mut Parity,
               case: &str,
               arguments: &[&str],
               stdin: &str|
     -> anyhow::Result<(bool, String, String)> {
        let recorded = parity.old(case, || {
            let mut child = common::legacy_airk_command(root.path(), state.path(), "ctl-config")
                .args(arguments)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        });
        Ok((recorded.success, recorded.stdout, recorded.stderr))
    };
    let message = |stderr: &str, prefix: &str| {
        let line = stderr.lines().last().unwrap_or_default();
        line.strip_prefix(prefix).unwrap_or(line).to_owned()
    };

    // A prepared request is a read: the same request for the same state.
    let draft = r#"{"operation":"vectors","enabled":false}"#;
    let prepare = [
        "service",
        "prepare-configuration",
        "--runtime-dir",
        &control,
        "--project",
        &binding,
    ];
    let (succeeded, before, stderr) = old(&mut parity, "prepare-configuration", &prepare, draft)?;
    assert!(succeeded, "{stderr}");
    let (code, after, stderr) = ctl(&prepare, draft);
    assert_eq!(
        (code, parity.canonical(&after).as_str()),
        (0, before.as_str()),
        "{stderr}"
    );

    // Refusals print the same message: a malformed draft, an unknown mode
    // value, a measurement that does not parse.
    fs::write(&measurement, "{}")?;
    let measurement = measurement.to_str().unwrap().to_owned();
    for (case, arguments, stdin) in [
        (
            "configure bogus",
            vec!["configure", "--stdin"],
            "{\"operation\":\"bogus\"}",
        ),
        (
            "usage import preview",
            vec!["usage", "import", &measurement, "--preview"],
            "",
        ),
    ] {
        let (succeeded, stdout, old_stderr) = old(&mut parity, case, &arguments, stdin)?;
        let (code, new_stdout, new_stderr) = ctl(&arguments, stdin);
        assert!(!succeeded && code != 0, "{arguments:?}");
        assert_eq!(parity.canonical(&new_stdout), stdout, "{arguments:?}");
        assert_eq!(
            message(&parity.canonical(&new_stderr), "airkctl: "),
            message(&old_stderr, "Error: "),
            "{arguments:?}"
        );
    }

    // Mutations: the same job, and the same saved-request line.
    fs::write(&draft_file, draft)?;
    let draft_file = draft_file.to_str().unwrap().to_owned();
    for (case, arguments) in [
        ("configure file", vec!["configure", "--file", &draft_file]),
        ("usage clear", vec!["usage", "clear"]),
        (
            "usage set aggregates_only",
            vec!["usage", "set", "--mode", "aggregates_only"],
        ),
    ] {
        let (succeeded, stdout, old_stderr) = old(&mut parity, case, &arguments, "")?;
        let (code, new_stdout, new_stderr) = ctl(&arguments, "");
        assert!(succeeded, "{arguments:?}: {old_stderr}");
        assert_eq!(code, 0, "{arguments:?}: {new_stderr}");
        let (before, after): (Value, Value) = (
            serde_json::from_str(&stdout)?,
            serde_json::from_str(&parity.canonical(&new_stdout))?,
        );
        assert_eq!(after["status"], "job", "{arguments:?}");
        assert_eq!(
            (&after["job"]["project"], &after["job"]["kind"]),
            (&before["job"]["project"], &before["job"]["kind"]),
            "{arguments:?}"
        );
        assert_eq!(
            regex_free_configuration_nonce(&parity.canonical(&new_stderr)),
            regex_free_configuration_nonce(&old_stderr),
            "{arguments:?}"
        );
    }

    // A saved configuration request: prepared, listed, forgotten.
    let saved = [
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "prepare-configuration",
        "--project",
        &binding,
        "--file",
        &draft_file,
    ];
    let (code, prepared, stderr) = ctl(&saved, "");
    assert_eq!(code, 0, "{stderr}");
    let prepared: Value = serde_json::from_str(&prepared)?;
    assert_eq!(prepared["submitted"], false);
    let nonce = prepared["prepared"].as_str().unwrap().to_owned();
    let (_, listed, _) = ctl(
        &["service", "requests", "--runtime-dir", &control, "list"],
        "",
    );
    assert!(
        listed.contains(&nonce) && listed.contains("\"kind\":\"configure\""),
        "{listed}"
    );
    let (code, _, _) = ctl(
        &[
            "service",
            "requests",
            "--runtime-dir",
            &control,
            "forget",
            &nonce,
        ],
        "",
    );
    assert_eq!(code, 0);
    parity.finish();
    Ok(())
}

/// The saved-configuration-request lines with their nonces replaced.
fn regex_free_configuration_nonce(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|line| line.starts_with("Saved CLI configuration request "))
        .map(|line| {
            let rest = &line["Saved CLI configuration request ".len()..];
            let end = rest.find(';').unwrap_or(rest.len());
            format!("Saved CLI configuration request <nonce>{}", &rest[end..])
        })
        .collect()
}

/// `settings` and `usage report|policy` (split step 3c.5c): the document
/// `airk` printed, by project binding (N1). JSON
/// documents compare after `stable`; text reports compare as printed.
#[test]
fn airkctl_usage_documents_match_airk() -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse, UsageDocumentRead};
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-usage");
    current_quiescent(
        &service,
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    // Freeze a real empty usage population instead of hiding measured fields.
    for line in [
        vec!["usage", "configure", "--mode", "disabled"],
        vec!["usage", "clear"],
    ] {
        let output = common::command(root.path(), state.path(), "ctl-usage")
            .args(line)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let binding = service.project.binding.to_string();
    let reads = [
        (
            vec!["settings", "roots"],
            json!({"settings":{"view":"roots"}}),
        ),
        (
            vec!["settings", "vector_status"],
            json!({"settings":{"view":"vector_status"}}),
        ),
        (vec!["usage", "policy"], json!("policy")),
        (
            vec!["usage", "report"],
            json!({"report":{"days":7,"format":"json"}}),
        ),
        (
            vec!["usage", "report", "--format", "markdown"],
            json!({"report":{"days":7,"format":"markdown"}}),
        ),
        (
            vec!["usage", "report", "--days", "0"],
            json!({"report":{"days":0,"format":"json"}}),
        ),
    ];
    let mut compared = 0;
    {
        let flags = ["--service-project", binding.as_str()];
        for (line, read) in &reads {
            let read: UsageDocumentRead = serde_json::from_value(read.clone())?;
            let expected = current_native(
                &service.control,
                ControlMethod::UsageDocument {
                    project: service.project.binding,
                    read,
                },
            )?;
            let actual = current_ctl(&service.control, &flags, line)?;
            match expected {
                ControlResponse::UsageDocument { document } => {
                    assert!(
                        actual.status.success(),
                        "{line:?}: {}",
                        String::from_utf8_lossy(&actual.stderr)
                    );
                    assert!(actual.stderr.is_empty());
                    if let Some(text) = document.as_str() {
                        assert_eq!(
                            std::str::from_utf8(&actual.stdout)?,
                            text,
                            "complete native text report"
                        );
                    } else {
                        assert_eq!(
                            comparable(serde_json::from_slice(&actual.stdout)?),
                            comparable(document),
                            "complete native usage document; no metric masking"
                        );
                    }
                }
                ControlResponse::UsageReadFailed { message, native } => {
                    assert_eq!(actual.status.code(), Some(1));
                    if let Some(native) = native {
                        assert_eq!(
                            comparable(serde_json::from_slice(&actual.stdout)?),
                            comparable(native)
                        );
                    } else {
                        assert!(actual.stdout.is_empty());
                    }
                    let error: airkctl::contract::ErrorLine =
                        serde_json::from_slice(&actual.stderr)?;
                    assert_eq!(error.message, message);
                    assert_eq!(error.code, "operation_failed");
                }
                other => anyhow::bail!(
                    "unexpected native usage response: {}",
                    serde_json::to_string(&other)?
                ),
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 6, "complete usage input catalog");
    Ok(())
}

/// The complete current service model catalog reaches the CLI unchanged.
/// The U95 catalog remains historical evidence for its original model/runtime.
#[test]
fn airkctl_model_catalog_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-models");
    let airk_core::service::ControlResponse::ModelCatalog { document } =
        service.request(airk_core::service::ControlMethod::ModelCatalog {})
    else {
        anyhow::bail!("missing current native model catalog");
    };
    let output = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .arg("model-catalog")
        .output()?;
    assert_eq!(success(output), document);
    // Never recapture an old product catalog to make a current comparison pass.
    let historical: Value = serde_json::from_str(include_str!(
        "../../../fixtures/split/parity/airkctl_model_catalog_matches_airk.json"
    ))?;
    let historical: Value = serde_json::from_str(
        historical["cases"]["model-catalog"]["stdout"]
            .as_str()
            .unwrap(),
    )?;
    assert_eq!(
        historical["catalog"]["default_profile"],
        "minilm-l6-v2-u8-v1"
    );
    assert_eq!(
        historical["capabilities"]["runtime"],
        "onnxruntime-cpu/1.22.1/airk-int8-b1-1"
    );
    Ok(())
}

/// `explorer …` (split step 3d): the same operation through `airk` (into
/// `--output`) and `airkctl` (into `--output-file`, U79, through M11) writes
/// the same result and prints the same `{written, ok}` line, apart from the
/// file name and the drafts' clock fields.
#[test]
fn airkctl_explorer_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-explorer");
    let control = service.control.to_str().unwrap().to_owned();
    let out = tempfile::tempdir()?;
    let ctl = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = ["airkctl", "--service-runtime-dir", control.as_str()]
            .into_iter()
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    // Drafts carry the clock that wrote them.
    fn timeless(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.retain(|key, _| !key.ends_with("_ms") && !key.ends_with("_at"));
                fields.values_mut().for_each(timeless);
            }
            Value::Array(rows) => rows.iter_mut().for_each(timeless),
            _ => {}
        }
    }
    // A written result (canonical text) without its clock fields.
    fn read(text: &str) -> anyhow::Result<Value> {
        let mut value: Value = serde_json::from_str(text)?;
        timeless(&mut value);
        Ok(value)
    }
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("list-templates", vec![]),
        (
            "get-template",
            vec!["--args", r#"{"id":"no-such-template"}"#],
        ),
        (
            "draft-search",
            vec!["--args", r#"{"query":"recover a password"}"#],
        ),
        ("draft-search", vec!["--args", r#"{"query":1}"#]),
        (
            "validate-draft",
            vec!["--args", r#"{"kind":"search","draft":{}}"#],
        ),
        ("draft-plan", vec!["--args", "[1]"]),
    ];
    let mut parity = Parity::new("airkctl_explorer_matches_airk");
    parity.mask_json_fields(CLOCK_FIELDS);
    parity.substitute_path(out.path(), "<OUT>");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    for (index, (command, extra)) in cases.iter().enumerate() {
        let old_path = out.path().join(format!("old-{index}.json"));
        let new_path = out.path().join(format!("new-{index}.json"));
        let before = parity.old(&format!("{index} {command}"), || {
            Command::new(env!("CARGO_BIN_EXE_airk"))
                .args(["explorer", command])
                .args(extra)
                .arg("--output")
                .arg(&old_path)
                .output()
                .unwrap()
        });
        let written = parity.old_file(&format!("{index} {command} output"), &old_path);
        let mut line = vec!["explorer", command];
        line.extend(extra.iter().copied());
        line.extend(["--output-file", new_path.to_str().unwrap()]);
        let (code, stdout, stderr) = ctl(&line);
        assert_eq!(code == 0, before.success, "{command} {extra:?}: {stderr}");
        assert_eq!(
            parity.canonical(&stdout.replace(&format!("new-{index}"), &format!("old-{index}"))),
            before.stdout,
            "{command} {extra:?}"
        );
        assert_eq!(written.success, new_path.exists(), "{command} {extra:?}");
        if new_path.exists() {
            assert_eq!(
                read(&parity.canonical(&fs::read_to_string(&new_path)?))?,
                read(&written.stdout)?,
                "{command} {extra:?}"
            );
        } else {
            assert_eq!(
                parity
                    .canonical(&stderr)
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .strip_prefix("airkctl: "),
                before
                    .stderr
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .strip_prefix("Error: "),
                "{command} {extra:?}"
            );
        }
    }
    // proposal-status reads a written draft: the one `airk` wrote while
    // capturing; in replay (no `airk` run) the one airkctl wrote.
    let draft = out.path().join(if parity.capturing() {
        "old-2.json"
    } else {
        "new-2.json"
    });
    let old_status = out.path().join("old-status.json");
    let new_status = out.path().join("new-status.json");
    let before = parity.old("proposal-status", || {
        Command::new(env!("CARGO_BIN_EXE_airk"))
            .args(["explorer", "proposal-status", "--draft"])
            .arg(&draft)
            .arg("--output")
            .arg(&old_status)
            .output()
            .unwrap()
    });
    let old_status = parity.old_file("proposal-status output", &old_status);
    let (code, _, stderr) = ctl(&[
        "explorer",
        "proposal-status",
        "--draft",
        draft.to_str().unwrap(),
        "--output-file",
        new_status.to_str().unwrap(),
    ]);
    assert_eq!(code == 0, before.success, "{stderr}");
    let status = |parity: &Parity| -> anyhow::Result<Value> {
        read(&parity.canonical(&fs::read_to_string(&new_status)?))
    };
    assert_eq!(status(&parity)?, read(&old_status.stdout)?);
    // An existing output file is refused alike and left unchanged.
    let (code, _, _) = ctl(&[
        "explorer",
        "list-templates",
        "--output-file",
        new_status.to_str().unwrap(),
    ]);
    assert_ne!(code, 0);
    assert_eq!(status(&parity)?, read(&old_status.stdout)?);
    parity.finish();
    Ok(())
}

/// Modern endpoint output carries the entire checked native descriptor and
/// all three canonical manual entries, outside the persisted descriptor.
#[test]
fn airkctl_agent_connections_match_airk() -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse};
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-agents");
    let ControlResponse::AgentEndpoint { endpoint, manual } =
        service.request(ControlMethod::AgentEndpoint {})
    else {
        anyhow::bail!("missing current native agent endpoint");
    };
    endpoint.validate()?;
    assert_eq!(
        manual,
        airk_core::agent_manual::project_entries(endpoint.url)
    );
    let mut expected = serde_json::to_value(endpoint)?;
    expected["manual"] = serde_json::to_value(manual)?;
    let output = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .args(["agents", "endpoint"])
        .output()?;
    assert_eq!(success(output), expected);
    Ok(())
}

/// Current URL-only configuration on the real service, including all three
/// native manual entries, retained repair/remove and refused legacy sources.
#[test]
fn airkctl_agents_connect_list_repair_remove_on_a_live_service() -> anyhow::Result<()> {
    use airk_core::agent_manual::{ManualClient, ManualScope};
    use airk_process::bounded_command::{Invocation, execute_with_timeout};
    use std::time::Duration;
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-agentcfg");
    let inputs = tempfile::tempdir()?;
    let project = inputs.path().canonicalize()?.join("private");
    airk_cli::local_ipc::prepare_root(&project)?;
    let project = std::path::PathBuf::from(
        project
            .to_str()
            .unwrap()
            .strip_prefix(r"\\?\")
            .unwrap_or(project.to_str().unwrap()),
    );
    // Eligibility is actual isolated Git/npm configuration, never an override
    // of the user's environment or the production whole-file retention fence.
    let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|candidate| candidate.is_absolute() && candidate.is_file())
        .ok_or_else(|| anyhow::anyhow!("Git is required for the retained-edit fixture"))?
        .canonicalize()?;
    let git_run = |arguments: &[&str]| -> anyhow::Result<()> {
        let mut args = vec![
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-C",
            project.to_str().unwrap(),
        ];
        args.extend_from_slice(arguments);
        let output = execute_with_timeout(&Invocation::new(&git, &args), Duration::from_secs(5))?;
        anyhow::ensure!(output.success, "fixture Git setup failed: {}", output.text);
        Ok(())
    };
    git_run(&["init", "--quiet", "--template="])?;
    let excludes = project.join("empty-excludes");
    fs::write(&excludes, b"")?;
    git_run(&["config", "core.excludesFile", excludes.to_str().unwrap()])?;
    fs::write(project.join(".gitignore"), b".airk/\n")?;
    fs::write(project.join("package.json"), br#"{"name":"airk-agent-integration-fixture","version":"0.0.0","packageManager":"npm@11.12.1","files":["payload"],"main":"payload/main.js"}"#)?;
    fs::create_dir(project.join("payload"))?;
    fs::write(project.join("payload/main.js"), b"fixture\n")?;
    for directory in [".codex", ".vscode"] {
        airk_cli::local_ipc::prepare_root(&project.join(directory))?;
    }
    let originals = [
        (
            ManualClient::Claude,
            "claude",
            "claude",
            ".mcp.json",
            "{\n  \"mcpServers\": {\n    \"other\": {\"command\": \"node\", \"args\": [\"x\"]}\n  }\n}\n",
        ),
        (
            ManualClient::Codex,
            "codex",
            "codex",
            ".codex/config.toml",
            "# Preserve the other server.\n[mcp_servers.other]\ncommand = \"node\"\nargs = [\"x\"]\n",
        ),
        (
            ManualClient::Vscode,
            "vscode",
            "copilot",
            ".vscode/mcp.json",
            "{\n  \"servers\": {\n    \"other\": {\"command\": \"node\", \"args\": [\"x\"]}\n  }\n}\n",
        ),
    ];
    // Native targets join each path component; preserve the host's spelling
    // in the complete manual DTO as well as the actual file operations.
    let config_file = |relative: &str| {
        Path::new(relative)
            .components()
            .fold(project.clone(), |path, part| path.join(part.as_os_str()))
    };
    for (_, _, _, relative, original) in originals {
        fs::write(config_file(relative), original)?;
    }
    let ctl = |arguments: &[&str]| -> std::io::Result<Output> {
        common::ctl::Ctl::new()
            .args(["--output", "json", "--service-runtime-dir"])
            .arg(&service.control)
            .arg("agents")
            .args(arguments)
            .output()
    };
    let endpoint = success(ctl(&["endpoint"])?);
    let url = endpoint["url"].as_str().unwrap();
    let folder = project.to_str().unwrap();
    let mut connected_files = Vec::new();
    for (client, name, report_name, relative, original) in originals {
        let file = config_file(relative);
        let manual = airk_core::agent_manual::entry(
            client,
            ManualScope::Project,
            file.to_str().unwrap().into(),
            "airk",
            url.parse()?,
        )?;
        let requested = success(ctl(&["manual", "--client", name, "--project", folder])?);
        assert_eq!(requested, serde_json::to_value(&manual)?);
        assert_eq!(
            fs::read_to_string(&file)?,
            original,
            "manual must be read-only"
        );
        let connected = success(ctl(&["connect", "--client", name, "--project", folder])?);
        assert_eq!(
            connected,
            json!({"schema":1,"client":report_name,"scope":"project", "file":file,
            "serverKey":"airk","url":url,"changed":true,"manual":manual})
        );
        let text = fs::read_to_string(&file)?;
        assert!(text.contains("other") && text.contains(url), "{text}");
        for retired in [
            "Authorization",
            "headersHelper",
            "http_headers_helper",
            "bearer_token_env_var",
            "agents credential",
            "airk1.",
        ] {
            assert!(
                !text.contains(retired),
                "unexpected credential marker {retired}: {text}"
            );
        }
        let unchanged = success(ctl(&["connect", "--client", name, "--project", folder])?);
        let mut expected = connected;
        expected["changed"] = json!(false);
        assert_eq!(unchanged, expected);
        assert_eq!(fs::read_to_string(&file)?, text);
        connected_files.push((client, file, text));
    }
    let list = || -> anyhow::Result<Value> {
        Ok(success(ctl(&["list", "--project", folder, "--no-user"])?))
    };
    let listed = list()?;
    assert_eq!(listed["findings"], json!([]), "{listed}");
    assert_eq!(listed["blockers"], json!([]), "{listed}");
    let entries = listed["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{listed}");
    for entry in entries {
        assert!(entry.get("connectionId").is_none(), "{entry}");
        assert_eq!(entry["url"], url);
        // The wire name of the URL transport is unchanged; the URL is http
        // (local trust plan C) and no trust observation exists any more.
        assert_eq!(entry["transport"], "https");
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");
        assert_eq!(entry["scope"], "project");
        assert_eq!(entry["status"], json!("ok"), "{entry}");
    }
    // Every retired source form refuses and preserves all complete files.
    let id = "0123456789abcdef0123456789abcdef";
    for retired in [
        vec!["--connection-id", id],
        vec!["--selection-file", "missing-selection.json"],
        vec![
            "--connection-id",
            id,
            "--selection-file",
            "missing-selection.json",
        ],
    ] {
        let mut arguments = vec!["connect", "--client", "claude", "--project", folder];
        arguments.extend(retired);
        let output = ctl(&arguments)?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        // The retired selectors are no longer arguments: a usage error.
        let refusal: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(refusal["class"], "usage", "{refusal}");
        assert_eq!(refusal["retryable"], false, "{refusal}");
        for (_, file, bytes) in &connected_files {
            assert_eq!(fs::read_to_string(file)?, *bytes);
        }
    }
    // Stale endpoint and credential fields are recognized; repair preserves
    // every other field and removes the retired authority rather than re-enrolling.
    for (client, file, text) in &connected_files {
        let stale = if *client == ManualClient::Codex {
            text.replace(url, "https://127.0.0.1:1/mcp").replace(
                "[mcp_servers.airk]\n",
                "[mcp_servers.airk]\nbearer_token_env_var = \"RETIRED_TOKEN\"\n",
            )
        } else {
            let mut value: Value = serde_json::from_str(text)?;
            let container = if *client == ManualClient::Claude {
                "mcpServers"
            } else {
                "servers"
            };
            value[container]["airk"]["url"] = json!("https://127.0.0.1:1/mcp");
            value[container]["airk"]["headers"]["Authorization"] =
                json!("Bearer retired-placeholder");
            value[container]["airk"]["headersHelper"] = json!(
                "retired airkctl agents credential --connection-id <connection-id> --format header-json"
            );
            serde_json::to_string_pretty(&value)?
        };
        fs::write(file, stale)?;
    }
    let stale = list()?;
    let findings = stale["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 3, "{stale}");
    assert!(
        findings
            .iter()
            .all(|finding| finding["problem"] == "https_unknown_connection"
                && finding["detail"].as_str().unwrap().contains("U110")),
        "{stale}"
    );
    let repaired = success(ctl(&["repair", "--project", folder, "--no-user"])?);
    assert_eq!(repaired["schema"], 1);
    assert_eq!(repaired["blockers"], json!([]));
    let actions = repaired["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 3, "{repaired}");
    for action in actions {
        assert_eq!(action["changed"], true, "{action}");
        assert_eq!(action["error"], Value::Null, "{action}");
        assert_eq!(action["url"], url);
        assert_eq!(action["problem"], "https_unknown_connection");
    }
    for (client, file, text) in &connected_files {
        let repaired = fs::read_to_string(file)?;
        if *client == ManualClient::Codex {
            assert_eq!(
                toml::from_str::<toml::Value>(&repaired)?,
                toml::from_str::<toml::Value>(text)?
            );
            assert!(repaired.contains("# Preserve the other server."));
        } else {
            assert_eq!(
                serde_json::from_str::<Value>(&repaired)?,
                serde_json::from_str::<Value>(text)?
            );
        }
    }
    assert_eq!(list()?["findings"], json!([]));
    // Local trust plan C: an entry an HTTPS build wrote (the same port, the
    // https:// URL, no credential) is AIRK's, an endpoint mismatch, and repair
    // rewrites it to the live http:// endpoint.
    let legacy = url.replacen("http://", "https://", 1);
    for (_, file, text) in &connected_files {
        fs::write(file, text.replace(url, &legacy))?;
    }
    let old = list()?;
    let findings = old["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 3, "{old}");
    assert!(
        findings
            .iter()
            .all(|finding| finding["problem"] == "https_endpoint_mismatch"
                && finding["url"] == legacy.as_str()),
        "{old}"
    );
    let repaired = success(ctl(&["repair", "--project", folder, "--no-user"])?);
    let actions = repaired["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 3, "{repaired}");
    for action in actions {
        assert_eq!(action["changed"], true, "{action}");
        assert_eq!(action["url"], url);
        assert_eq!(action["problem"], "https_endpoint_mismatch");
    }
    for (_, file, text) in &connected_files {
        assert_eq!(fs::read_to_string(file)?, *text);
    }
    assert_eq!(list()?["findings"], json!([]));
    let no_op = success(ctl(&["repair", "--project", folder, "--no-user"])?);
    assert_eq!(no_op, json!({"schema":1,"actions":[],"blockers":[]}));
    let removed = success(ctl(&["remove", "--project", folder, "--no-user"])?);
    assert_eq!(removed["actions"].as_array().unwrap().len(), 3, "{removed}");
    assert!(
        removed["actions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|action| action["changed"] == true && action["error"].is_null()),
        "{removed}"
    );
    assert_eq!(removed["blockers"], json!([]));
    assert_eq!(list()?["entries"], json!([]));
    for (client, _, _, relative, original) in originals {
        let final_text = fs::read_to_string(config_file(relative))?;
        if client == ManualClient::Codex {
            assert_eq!(
                toml::from_str::<toml::Value>(&final_text)?,
                toml::from_str::<toml::Value>(original)?
            );
            assert!(final_text.contains("# Preserve the other server."));
        } else {
            assert_eq!(
                serde_json::from_str::<Value>(&final_text)?,
                serde_json::from_str::<Value>(original)?
            );
        }
    }
    // A normal shared/unreviewed whole-file target remains refused, with the
    // exact complete manual entry available even though automatic editing failed.
    let unsafe_project = inputs.path().join("unreviewed");
    fs::create_dir(&unsafe_project)?;
    let unsafe_file = unsafe_project.join(".mcp.json");
    let original = b"{\"mcpServers\":{\"other\":{\"command\":\"node\"}}}\n";
    fs::write(&unsafe_file, original)?;
    let unsafe_folder = unsafe_project.to_str().unwrap();
    let manual = success(ctl(&[
        "manual",
        "--client",
        "claude",
        "--project",
        unsafe_folder,
    ])?);
    let refused = ctl(&["connect", "--client", "claude", "--project", unsafe_folder])?;
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    let refused: Value = serde_json::from_slice(&refused.stderr)?;
    assert_eq!(refused["code"], "agent_config_edit_refused");
    let typed: airk_core::agent_manual::AgentConfigRefusal =
        serde_json::from_value(refused["native"].clone())?;
    assert_eq!(
        typed.reason,
        airk_core::agent_manual::AgentConfigRefusalReason::AutomaticPolicy
    );
    assert_eq!(serde_json::to_value(typed.manual)?, manual);
    assert_eq!(typed.file, manual["file"].as_str().unwrap());
    assert_eq!(fs::read(&unsafe_file)?, original);
    assert!(!unsafe_project.join(".airk").exists());
    Ok(())
}

/// `airkctl explorer get-handoff` (M12): a handoff a window published is
/// written to a new file; an unknown id is refused and writes nothing.
#[test]
fn airkctl_explorer_get_handoff_reads_a_published_handoff() -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse};
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-handoff");
    let control = service.control.to_str().unwrap().to_owned();
    let ControlResponse::WindowOpened {
        window_id, lease, ..
    } = service.request(ControlMethod::WindowOpen {
        label: "handoff test".into(),
        owner_pid: std::process::id(),
    })
    else {
        anyhow::bail!("window did not open");
    };
    let handoff = serde_json::json!({"kind": "search", "query": "parity"});
    let ControlResponse::HandoffPublished { handoff_id, .. } =
        service.request(ControlMethod::HandoffPublish {
            window_id,
            lease,
            handoff: handoff.clone(),
            ttl_ms: 60_000,
        })
    else {
        anyhow::bail!("handoff was not published");
    };
    let out = tempfile::tempdir()?;
    let get = |id: String, file: &std::path::Path| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = [
            "airkctl",
            "--service-runtime-dir",
            control.as_str(),
            "explorer",
            "get-handoff",
            "--handoff-id",
            id.as_str(),
            "--output-file",
            file.to_str().unwrap(),
        ]
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let written = out.path().join("handoff.json");
    let (code, stdout, stderr) = get(handoff_id.to_string(), &written);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(serde_json::from_str::<Value>(&stdout)?["ok"], true);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&written)?)?,
        handoff
    );
    let missing = out.path().join("missing.json");
    let unknown = airk_kernel::service_broker::new_instance_id()?.to_string();
    let (code, _, _) = get(unknown, &missing);
    assert_ne!(code, 0);
    assert!(!missing.exists());
    Ok(())
}

/// `airkctl session` on a real service (split step 3f.3): `hello` opens a
/// window the service lists (W1, W5), an `invoke` prints what the one-shot
/// command printed, and end of input closes the window (W3). No frame carries
/// the lease.
#[test]
fn airkctl_session_opens_and_closes_its_window_on_a_live_service() -> anyhow::Result<()> {
    use std::{
        io::{Cursor, Read},
        sync::mpsc,
        time::{Duration, Instant},
    };
    struct Input {
        pending: Cursor<Vec<u8>>,
        incoming: mpsc::Receiver<Vec<u8>>,
    }
    impl Read for Input {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            loop {
                let count = self.pending.read(bytes)?;
                if count != 0 {
                    return Ok(count);
                }
                match self.incoming.recv() {
                    Ok(next) => self.pending = Cursor::new(next),
                    Err(_) => return Ok(0),
                }
            }
        }
    }
    struct Output(mpsc::Sender<Vec<u8>>);
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .send(bytes.to_vec())
                .map_err(|_| std::io::ErrorKind::BrokenPipe)?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let frame = |value: Value| {
        let body = serde_json::to_vec(&value).unwrap();
        let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
        bytes.extend(body);
        bytes
    };
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-session");
    let airk_core::service::ControlResponse::Health { observation } =
        service.request(airk_core::service::ControlMethod::Health)
    else {
        anyhow::bail!("missing service health");
    };
    let (send, incoming) = mpsc::channel();
    let (outgoing, receive) = mpsc::channel();
    let mut options =
        airkctl::cli::session::Options::new(env!("CARGO_PKG_VERSION"), "0".repeat(64));
    options.control_root = Some(service.control.clone());
    options.label = "live window".into();
    let actor = std::thread::spawn(move || {
        airkctl::cli::session::serve_streams(
            Input {
                pending: Cursor::new(Vec::new()),
                incoming,
            },
            Output(outgoing),
            options,
        )
    });
    send.send(frame(json!({"id": "01".repeat(16), "request": {"hello": {
        "protocol": 2, "client": {"name": "airk-vscode", "version": "1.9.0"}, "i2_contracts": [1]}}})))?;
    send.send(frame(json!({"id": "02".repeat(16), "request": {"invoke": {
        "argv": ["projects"], "deadline_ms": 30000}}})))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut bytes = Vec::new();
    let mut parsed = 0;
    let mut frames: Vec<Value> = Vec::new();
    while !frames.iter().any(|frame| frame["id"] == "02".repeat(16)) {
        bytes.extend(receive.recv_timeout(deadline.saturating_duration_since(Instant::now()))?);
        while bytes.len() >= parsed + 4 {
            let length = u32::from_be_bytes(bytes[parsed..parsed + 4].try_into()?) as usize;
            if bytes.len() < parsed + 4 + length {
                break;
            }
            frames.push(serde_json::from_slice(
                &bytes[parsed + 4..parsed + 4 + length],
            )?);
            parsed += 4 + length;
        }
    }
    let reply = |id: &str| {
        frames
            .iter()
            .find(|frame| frame["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("no reply {id}: {frames:?}"))
    };
    let hello = reply(&"01".repeat(16));
    assert_eq!(hello["outcome"]["status"], "hello", "{hello}");
    assert_eq!(
        hello["outcome"]["service_instance"],
        observation.instance.to_string()
    );
    assert!(hello["outcome"]["window_id"].is_string(), "{hello}");
    let airk_core::service::ControlResponse::AssistanceWindows { windows } =
        service.request(airk_core::service::ControlMethod::AssistanceWindows {})
    else {
        anyhow::bail!("missing active window listing");
    };
    assert_eq!(windows.len(), 1, "{windows:?}");
    assert_eq!(
        serde_json::to_value(&windows)?[0]["window_id"],
        hello["outcome"]["window_id"]
    );
    let invoked = reply(&"02".repeat(16));
    let one_shot = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .arg("projects")
        .output()?;
    assert!(
        one_shot.status.success(),
        "{}",
        String::from_utf8_lossy(&one_shot.stderr)
    );
    assert_eq!(invoked["outcome"]["status"], "invoked", "{invoked}");
    assert_eq!(invoked["outcome"]["exit_code"], 0, "{invoked}");
    assert_eq!(
        invoked["outcome"]["stdout"],
        String::from_utf8(one_shot.stdout)?
    );
    assert_eq!(
        invoked["outcome"]["stderr"],
        String::from_utf8(one_shot.stderr)?
    );
    // EOF is cancellation by contract; deliver it only after the response is observed.
    drop(send);
    assert_eq!(actor.join().expect("session actor panicked"), 0);
    bytes.extend(receive.try_iter().flatten());
    assert!(!String::from_utf8_lossy(&bytes).contains("lease"));
    let airk_core::service::ControlResponse::AssistanceWindows { windows } =
        service.request(airk_core::service::ControlMethod::AssistanceWindows {})
    else {
        anyhow::bail!("missing final window listing");
    };
    assert!(windows.is_empty(), "{windows:?}");
    Ok(())
}
