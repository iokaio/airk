// SPDX-License-Identifier: Apache-2.0
//! Test clients repeat only the public, typed read-rejection contract.
#![allow(dead_code)]
use anyhow::Result;
use serde_json::{Map, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    pub delay: Duration,
    pub fresh_brokerage: bool,
    pub publication: bool,
}

pub fn classify(envelope: &Value) -> Option<Retry> {
    if envelope.get("isError")? != true {
        return None;
    }
    let content = envelope.get("content")?.as_array()?;
    if content.len() != 1 || content[0].get("type")? != "text" {
        return None;
    }
    let hint: Value = serde_json::from_str(content[0].get("text")?.as_str()?).ok()?;
    let (delay, fresh_brokerage, publication) = if hint.get("status") == Some(&Value::from("retry"))
    {
        if hint.get("retryable")? != true
            || hint.get("action")? != "repeat_same_request"
            || !matches!(
                hint.get("code")?.as_str()?,
                "publication_busy" | "selection_busy"
            )
        {
            return None;
        }
        (hint.get("retry_after_ms")?.as_u64()?, false, true)
    } else {
        let error = hint.get("error")?;
        let fresh = match error.get("code")?.as_str()? {
            "direct_query_admission" => false,
            "direct_query_setup" => true,
            _ => return None,
        };
        if error.get("reason")? != "busy"
            || error.get("query_submitted")? != false
            || error.get("retryable")? != true
            || error.get("fresh_brokerage_required")?.as_bool()? != fresh
        {
            return None;
        }
        (error.get("retry_after_ms")?.as_u64()?, fresh, false)
    };
    (1..=1000).contains(&delay).then_some(Retry {
        delay: Duration::from_millis(delay),
        fresh_brokerage,
        publication,
    })
}

pub fn within(envelope: &Value, deadline: Instant) -> Option<Retry> {
    classify(envelope).filter(|retry| Instant::now() + retry.delay < deadline)
}

pub fn from_output(output: &std::process::Output, deadline: Instant) -> Option<Retry> {
    // Tool rejection exits 1. A killed or otherwise failed process is not a reply.
    if !output.status.success() && output.status.code() != Some(1) {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .ok()
        .and_then(|value| within(&value, deadline))
}

pub fn output(command: &mut std::process::Command) -> std::io::Result<std::process::Output> {
    let deadline = Instant::now() + Duration::from_secs(60);
    output_until(command, deadline)
}

pub fn output_until(
    command: &mut std::process::Command,
    deadline: Instant,
) -> std::io::Result<std::process::Output> {
    loop {
        let output = once_until(command, deadline)?;
        let retry = from_output(&output, deadline);
        let Some(retry) = retry else {
            return Ok(output);
        };
        std::thread::sleep(retry.delay);
    }
}

/// A fixture-owned leaf CLI read, preserving its arguments, explicit environment
/// overrides and working directory (callers do not use env_clear/raw_arg/stdio).
/// Timeout kills and awaits only this client, never its separately owned service.
pub fn once_until(
    command: &std::process::Command,
    deadline: Instant,
) -> std::io::Result<std::process::Output> {
    if Instant::now() >= deadline {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "CLI read deadline exhausted",
        ));
    }
    let mut bounded = tokio::process::Command::new(command.get_program());
    bounded.args(command.get_args());
    if let Some(directory) = command.get_current_dir() {
        bounded.current_dir(directory);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            bounded.env(key, value);
        } else {
            bounded.env_remove(key);
        }
    }
    #[cfg(windows)]
    bounded.creation_flags(0x08000000); // CREATE_NO_WINDOW, like native_command.
    bounded
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            use tokio::io::AsyncReadExt;
            let mut child = bounded.spawn()?;
            let mut stdout = child.stdout.take().expect("piped stdout");
            let mut stderr = child.stderr.take().expect("piped stderr");
            let mut out = Vec::new();
            let mut err = Vec::new();
            let completed = tokio::time::timeout_at(deadline.into(), async {
                tokio::try_join!(
                    child.wait(),
                    stdout.read_to_end(&mut out),
                    stderr.read_to_end(&mut err)
                )
            })
            .await;
            match completed {
                Ok(result) => {
                    let (status, _, _) = result?;
                    Ok(std::process::Output {
                        status,
                        stdout: out,
                        stderr: err,
                    })
                }
                Err(_) => {
                    child.kill().await?;
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "CLI read deadline exhausted",
                    ))
                }
            }
        })
}

pub fn status_summary(value: &Value) -> Value {
    let runtime = &value["runtime"];
    let work = &value["work"];
    let state = match value["index_state"].as_str() {
        Some("available") => "available",
        Some("unavailable") => "unavailable",
        _ => "invalid",
    };
    let code = match value["index_error"]["code"].as_str() {
        Some("index_status_unavailable") => "index_status_unavailable",
        Some("index_worker_unavailable") => "index_worker_unavailable",
        _ => "unknown",
    };
    serde_json::json!({"index_state":state,"code":code,
        "service_responsive":runtime["service"]["responsive"].as_bool(),
        "service_accepting":runtime["service"]["accepting"].as_bool(),
        "automatic_start_blocked":work["worker"]["automatic_start_blocked"].as_bool(),
        "metadata_samples_complete":work["failed_observations"] == serde_json::json!([])})
}

/// A successful diagnostic observation is not a typed retry hint. These opaque
/// unavailable codes can conceal permanent causes; callers keep one deadline.
pub fn publication_pending(value: &Value, collection: &str, project: &str) -> Result<bool> {
    let summary = status_summary(value);
    if value["index_state"] == "available" {
        anyhow::ensure!(
            value["collection"] == collection
                && value["publication"]["collection"] == collection
                && value["publication"]["token"]
                    .as_str()
                    .is_some_and(|token| !token.is_empty())
                && value["files"].is_u64(),
            "invalid published status: {summary}"
        );
        return Ok(false);
    }
    anyhow::ensure!(
        value["index_state"] == "unavailable"
            && ["publication", "files", "chunks"]
                .iter()
                .all(|key| value.get(key) == Some(&Value::Null))
            && value["index_error"]["message"].is_string(),
        "invalid status observation: {summary}"
    );
    let runtime = &value["runtime"];
    let work = &value["work"];
    let identity = &work["worker"]["identity"];
    let scoped = match value["index_error"]["code"].as_str() {
        Some("index_status_unavailable") => value["collection"] == collection,
        Some("index_worker_unavailable") => {
            value["project"] == project
                && runtime["index_worker"].get("process") == Some(&Value::Null)
                && work["serving_worker_unavailable"] == true
        }
        _ => false,
    };
    anyhow::ensure!(
        scoped
            && runtime.get("service_error") == Some(&Value::Null)
            && runtime["service"]["responsive"] == true
            && runtime["service"]["accepting"] == true
            && work["basis"] == "instance_bound_service_metadata"
            && work["failed_observations"] == serde_json::json!([])
            && work["worker"]["automatic_start_blocked"] == false
            && (identity.is_null()
                || (identity["project"] == project
                    && runtime["service"]["instance"].is_string()
                    && identity["service"] == runtime["service"]["instance"])),
        "unhealthy or foreign status observation: {summary}"
    );
    Ok(true)
}

pub(crate) trait ReadRetry {
    async fn read_retry(&mut self, name: String, arguments: Map<String, Value>) -> Result<Value>;
}
impl ReadRetry for airk_cli::project_client::Connection {
    async fn read_retry(&mut self, name: String, arguments: Map<String, Value>) -> Result<Value> {
        direct(
            self,
            &name,
            &arguments,
            Instant::now() + Duration::from_secs(5),
            |_| {},
        )
        .await
    }
}

pub async fn direct(
    connection: &mut airk_cli::project_client::Connection,
    name: &str,
    arguments: &Map<String, Value>,
    deadline: Instant,
    mut rejected: impl FnMut(&Value),
) -> Result<Value> {
    loop {
        let receipt = connection.status_receipt();
        // Neither a transport error nor a successful query authorizes replay.
        let reply = connection.call(name.into(), arguments.clone()).await?;
        let Some(retry) = within(&reply, deadline) else {
            return Ok(reply);
        };
        // A raw connection cannot satisfy a request for fresh brokerage.
        if retry.fresh_brokerage {
            return Ok(reply);
        }
        if retry.publication {
            assert_eq!(
                connection.status_receipt(),
                receipt,
                "rejected publication read changed the client's observation receipt"
            );
        }
        rejected(&reply);
        tokio::time::sleep(retry.delay).await;
    }
}

/// Backend calls can obtain fresh brokerage; the same strict receipt classifier
/// still authorizes only a bounded repetition of the unchanged read.
pub async fn backend(
    backend: &dyn airk_mcp::remote::RemoteTools,
    name: &str,
    arguments: &Map<String, Value>,
    deadline: Instant,
    mut rejected: impl FnMut(&Value),
) -> Result<Value> {
    loop {
        let reply = backend.call(name.into(), arguments.clone()).await?;
        let Some(retry) = within(&reply, deadline) else {
            return Ok(reply);
        };
        rejected(&reply);
        tokio::time::sleep(retry.delay).await;
    }
}

/// Independently observed readiness may be unavailable after an otherwise complete
/// read. Validate that precise fixture state and each original reply's derived
/// receipt allocation before comparing their unchanged evidence and work budgets.
pub fn normalize_dated_readiness_pair(
    a: &mut Value,
    b: &mut Value,
    expected_artifacts: u64,
) -> Result<()> {
    use airk_core::readiness::{
        DefaultSearchPolicy, DiscoveryReadiness, DiscoveryState, SemanticReadiness, SemanticState,
    };
    use anyhow::ensure;
    if (a["discovery_observation"] == "unavailable_after_read")
        == (b["discovery_observation"] == "unavailable_after_read")
    {
        return Ok(());
    }
    // Validate both projections before mutating either of them.
    for value in [&*a, &*b] {
        if value["discovery_observation"] == "unavailable_after_read" {
            ensure!(value.get("discovery_readiness").is_none());
            ensure!(value.get("semantic_readiness").is_none());
        } else {
            ensure!(value.get("discovery_observation").is_none());
            let discovery: DiscoveryReadiness =
                serde_json::from_value(value["discovery_readiness"].clone())?;
            let semantic: SemanticReadiness =
                serde_json::from_value(value["semantic_readiness"].clone())?;
            ensure!(serde_json::to_value(&discovery)? == value["discovery_readiness"]);
            ensure!(serde_json::to_value(&semantic)? == value["semantic_readiness"]);
            ensure!(
                discovery.version == 1
                    && discovery.state == DiscoveryState::Ready
                    && discovery.epoch.is_some()
                    && discovery.processed_artifacts == expected_artifacts
                    && discovery.committed_artifacts == expected_artifacts
                    && discovery.total_artifacts == Some(expected_artifacts)
                    && discovery.failure.is_none()
            );
            ensure!(
                semantic.version == 1
                    && semantic.state == SemanticState::Disabled
                    && semantic.default_search_policy == DefaultSearchPolicy::PartialHybrid
                    && semantic.selected_artifacts == 0
                    && semantic.completed_artifacts == 0
                    && semantic.selected_units.is_none()
                    && semantic.completed_units == 0
                    && semantic.first_ready_unix_ms.is_none()
                    && semantic.fully_ready_unix_ms.is_none()
                    && semantic.selection_digest
                        == Some(
                            airk_core::vector_selection::VectorSelectionPolicy::default()
                                .digest()?
                        )
                    && semantic.pipeline_digest.is_none()
                    && semantic.failure.is_none()
            );
        }
        if let Some(receipt) = value.get("source_verification") {
            let _: airk_core::verification::VerificationReceipt =
                serde_json::from_value(receipt.clone())?;
            let mut projection = value.clone();
            projection
                .as_object_mut()
                .unwrap()
                .remove("source_verification");
            let expected = airk_core::verification::receipt_budget_for_reply(&projection)?;
            ensure!(receipt["limits"]["receipt_bytes_budget"] == expected);
        }
    }
    for value in [a, b] {
        if let Some(receipt) = value.get_mut("source_verification") {
            receipt["limits"]["receipt_bytes_budget"] = 0.into();
        }
        let fields = value.as_object_mut().unwrap();
        fields.remove("discovery_readiness");
        fields.remove("semantic_readiness");
        fields.remove("discovery_observation");
    }
    Ok(())
}

pub fn evidence_for_comparison(value: &mut Value) -> Result<()> {
    let Some(fields) = value.as_object_mut() else {
        anyhow::bail!("expected evidence object")
    };
    fields.remove("request_id");
    if let Some(freshness) = fields.get("freshness").cloned() {
        let observation: airk_core::changes::FreshnessObservation =
            serde_json::from_value(freshness)?;
        assert!(observation.observed_unix_ms > 0);
        assert!(!observation.live_source_verified);
        if observation.state == airk_core::changes::FreshnessState::Unavailable {
            assert!(observation.error.is_some());
        }
        // Preserve states, watermarks, failure and work identity. Only the
        // transport observation time varies between equivalent complete reads.
        fields
            .get_mut("freshness")
            .and_then(Value::as_object_mut)
            .unwrap()
            .remove("observed_unix_ms");
    }
    if let Some(readiness) = fields.get("semantic_readiness").cloned() {
        let _: airk_core::readiness::SemanticReadiness = serde_json::from_value(readiness)?;
    }
    if let Some(readiness) = fields.get("discovery_readiness").cloned() {
        let _: airk_core::readiness::DiscoveryReadiness = serde_json::from_value(readiness)?;
    }
    if let Some(observation) = fields.get("discovery_observation") {
        assert_eq!(observation, "unavailable_after_read");
    }
    if let Some(receipt) = fields.get_mut("source_verification") {
        let typed: airk_core::verification::VerificationReceipt =
            serde_json::from_value(receipt.clone())?;
        assert!(typed.observed_from_unix_ms > 0);
        assert!(typed.observed_to_unix_ms >= typed.observed_from_unix_ms);
        // Normalize only dated observations; keep all source identities,
        // associations, budgets, failures and denominators in parity checks.
        let fields = receipt.as_object_mut().expect("typed receipt");
        fields.remove("observed_from_unix_ms");
        fields.remove("observed_to_unix_ms");
        for source in fields
            .get_mut("sources")
            .and_then(Value::as_array_mut)
            .expect("typed source rows")
        {
            let fields = source.as_object_mut().expect("typed source");
            fields.remove("observed_from_unix_ms");
            fields.remove("observed_to_unix_ms");
        }
    }
    Ok(())
}
