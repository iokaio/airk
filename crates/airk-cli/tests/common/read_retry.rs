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
    if envelope
        .get("structuredContent")
        .is_some_and(|structured| structured != &hint)
    {
        return None;
    }
    if hint.get("schema").and_then(Value::as_str) == Some("airk.combined-read/1") {
        let retry = airk_core::combined_reads::read_retry::validated_retry(&hint)?;
        return Some(Retry {
            delay: Duration::from_millis(retry.retry_after_ms),
            fresh_brokerage: retry.fresh_brokerage_required,
            publication: matches!(retry.code.as_str(), "publication_busy" | "selection_busy"),
        });
    }
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

fn combined_body(envelope: &Value) -> Option<Value> {
    if envelope.get("isError")? != true {
        return None;
    }
    let content = envelope.get("content")?.as_array()?;
    if content.len() != 1 || content[0].get("type")? != "text" {
        return None;
    }
    let body: Value = serde_json::from_str(content[0].get("text")?.as_str()?).ok()?;
    if body.get("schema")?.as_str()? != "airk.combined-read/1"
        || envelope
            .get("structuredContent")
            .is_some_and(|structured| structured != &body)
    {
        return None;
    }
    Some(body)
}

fn bound_retry(retry: airk_core::combined_reads::read_retry::SingleRequiredMemberRetry) -> Retry {
    Retry {
        delay: Duration::from_millis(retry.retry_after_ms),
        fresh_brokerage: retry.fresh_brokerage_required,
        publication: matches!(retry.code.as_str(), "publication_busy" | "selection_busy"),
    }
}

pub fn classify_for_scope_with_arguments(
    envelope: &Value,
    expected: &airk_core::service::ScopedConnectRequest,
    arguments: &Map<String, Value>,
) -> Option<Retry> {
    if let Some(body) = combined_body(envelope) {
        return airk_core::combined_reads::read_retry::validated_retry_for_catalog(
            &body, expected, arguments,
        )
        .map(bound_retry);
    }
    classify_for_scope(envelope, expected)
}

pub fn classify_for_combined_scope(
    envelope: &Value,
    expected: &airk_core::service::CombinedConnectRequest,
    arguments: &Map<String, Value>,
) -> Option<Retry> {
    expected.validate().ok()?;
    if let Some(body) = combined_body(envelope) {
        return airk_core::combined_reads::read_retry::validated_retry_for_combined_catalog(
            &body, expected, arguments,
        )
        .map(bound_retry);
    }
    classify(envelope)
}

pub fn classify_for_selection_with_arguments(
    envelope: &Value,
    expected: &airk_core::service::WindowScopeSelection,
    source: &airk_core::scoped_reads::ScopeSelectionObservation,
    arguments: &Map<String, Value>,
) -> Option<Retry> {
    if let Some(body) = combined_body(envelope) {
        return airk_core::combined_reads::read_retry::validated_retry_for_selection(
            &body, expected, source, arguments,
        )
        .map(bound_retry);
    }
    if source.catalog_revision != expected.expected_catalog_revision {
        return None;
    }
    classify_for_selection(envelope, expected, source.scope_identity)
}

fn evidence_read_tool(name: &str) -> bool {
    matches!(
        name,
        "airk_status"
            | "airk_diagnostics"
            | "airk_capabilities"
            | "airk_search"
            | "airk_list_files"
            | "airk_read_file"
            | "airk_file_info"
            | "airk_symbols"
            | "airk_code_facts"
            | "airk_document_facts"
            | "airk_references"
            | "airk_relationships"
            | "airk_context"
            | "airk_outline"
            | "airk_imports"
            | "airk_changes"
            | "airk_query_scope"
            | "airk_recommendations"
            | "airk_help"
    )
}

/// A known setup refusal is safe only when tied to the caller's original,
/// validated frozen selection. Generic callers do not infer this selection.
pub fn classify_for_scope(
    envelope: &Value,
    expected: &airk_core::service::ScopedConnectRequest,
) -> Option<Retry> {
    expected.validate().ok()?;
    if combined_body(envelope).is_some() {
        return None;
    }
    if let Some(retry) = classify(envelope) {
        return Some(retry);
    }
    if !envelope.get("isError")?.as_bool()? {
        return None;
    }
    let content = envelope.get("content")?.as_array()?;
    if content.len() != 1 || content[0].get("type")?.as_str()? != "text" {
        return None;
    }
    let body: Value = serde_json::from_str(content[0].get("text")?.as_str()?).ok()?;
    if envelope.get("structuredContent")? != &body {
        return None;
    }
    let fields = body.as_object()?;
    if fields
        .keys()
        .any(|key| !matches!(key.as_str(), "error" | "requested_scope"))
    {
        return None;
    }
    let requested = body.get("requested_scope")?;
    if requested
        .as_object()?
        .keys()
        .any(|key| !matches!(key.as_str(), "confirmed" | "selection"))
        || requested.get("confirmed")?.as_bool()?
    {
        return None;
    }
    let mut observed: airk_core::service::ScopedConnectRequest =
        serde_json::from_value(requested.get("selection")?.clone()).ok()?;
    observed.validate().ok()?;
    let mut expected = expected.clone();
    // Native resolve_scoped may advance this metadata clock; every authority
    // field and the caller's original command remain unchanged.
    if observed.expected_catalog_revision < expected.expected_catalog_revision {
        return None;
    }
    observed.expected_catalog_revision = expected.expected_catalog_revision;
    expected.project_ids.sort();
    observed.project_ids.sort();
    if observed != expected {
        return None;
    }
    let error = body.get("error")?;
    if error.as_object()?.keys().any(|key| {
        !matches!(
            key.as_str(),
            "code"
                | "retryable"
                | "query_submitted"
                | "retry_after_ms"
                | "fresh_brokerage_required"
                | "query_replay_safe"
                | "native_control_response"
        )
    }) || error.get("query_submitted")?.as_bool()?
        || !error.get("retryable")?.as_bool()?
        || !error.get("fresh_brokerage_required")?.as_bool()?
        || error
            .get("query_replay_safe")
            .is_some_and(|value| value != true)
    {
        return None;
    }
    match error.get("code")?.as_str()? {
        "scoped_status_prepare_timeout" => {
            if error.get("native_control_response").is_some() {
                return None;
            }
        }
        "scoped_connection_unavailable" => {
            let native = error.get("native_control_response")?;
            if native
                .as_object()?
                .keys()
                .any(|key| !matches!(key.as_str(), "status" | "code"))
                || native.get("status")?.as_str()? != "failure"
                || !matches!(
                    native.get("code")?.as_str()?,
                    "broker_busy"
                        | "project_starting_or_unavailable"
                        | "project_unavailable_or_busy"
                        | "previous_workers_draining"
                        | "catalog_revision_changed"
                        | "catalog_clock_pending"
                        | "catalog_connection_busy"
                        | "project_capacity_busy"
                        | "project_restart_backoff"
                )
            {
                return None;
            }
        }
        _ => return None,
    }
    let delay = error.get("retry_after_ms")?.as_u64()?;
    (1..=1000).contains(&delay).then_some(Retry {
        delay: Duration::from_millis(delay),
        fresh_brokerage: true,
        publication: false,
    })
}

pub fn within_for_scope(
    envelope: &Value,
    expected: &airk_core::service::ScopedConnectRequest,
    deadline: Instant,
) -> Option<Retry> {
    classify_for_scope(envelope, expected).filter(|retry| Instant::now() + retry.delay < deadline)
}

/// Selection sessions bind setup retries to the exact active epoch. They do
/// not inherit a catalog grant's monotonic metadata-clock exception.
pub fn classify_for_selection(
    envelope: &Value,
    expected: &airk_core::service::WindowScopeSelection,
    scope_identity: airk_core::identity::Sha256Digest,
) -> Option<Retry> {
    use airk_core::index_catalog::{MAX_SELECTION, unique_bounded};
    let valid = |selection: &airk_core::service::WindowScopeSelection| {
        !selection.index_ids.is_empty()
            && !selection.project_ids.is_empty()
            && unique_bounded(&selection.index_ids, MAX_SELECTION).is_ok()
            && unique_bounded(&selection.project_ids, MAX_SELECTION).is_ok()
    };
    if !valid(expected) {
        return None;
    }
    if combined_body(envelope).is_some() {
        return None;
    }
    if let Some(retry) = classify(envelope) {
        return Some(retry);
    }
    if envelope.get("isError")? != true {
        return None;
    }
    let content = envelope.get("content")?.as_array()?;
    if content.len() != 1 || content[0].get("type")? != "text" {
        return None;
    }
    let body: Value = serde_json::from_str(content[0].get("text")?.as_str()?).ok()?;
    if envelope.get("structuredContent")? != &body
        || body
            .as_object()?
            .keys()
            .any(|key| !matches!(key.as_str(), "error" | "requested_scope"))
    {
        return None;
    }
    let requested = body.get("requested_scope")?;
    if requested
        .as_object()?
        .keys()
        .any(|key| !matches!(key.as_str(), "confirmed" | "selection"))
        || requested.get("confirmed")? != false
    {
        return None;
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Observed {
        window_scope_selection: airk_core::service::WindowScopeSelection,
        scope_identity: airk_core::identity::Sha256Digest,
    }
    let observed: Observed = serde_json::from_value(requested.get("selection")?.clone()).ok()?;
    let mut observed_selection = observed.window_scope_selection;
    if observed.scope_identity != scope_identity || !valid(&observed_selection) {
        return None;
    }
    let mut expected = expected.clone();
    observed_selection.index_ids.sort();
    observed_selection.project_ids.sort();
    expected.index_ids.sort();
    expected.project_ids.sort();
    if observed_selection != expected {
        return None;
    }
    let error = body.get("error")?;
    if error.as_object()?.keys().any(|key| {
        !matches!(
            key.as_str(),
            "code"
                | "retryable"
                | "query_submitted"
                | "retry_after_ms"
                | "fresh_brokerage_required"
                | "query_replay_safe"
        )
    }) || !matches!(
        error.get("code")?.as_str()?,
        "scoped_connection_unavailable" | "scoped_status_prepare_timeout"
    ) || error.get("query_submitted")? != false
        || error.get("retryable")? != true
        || error.get("fresh_brokerage_required")? != true
        || error
            .get("query_replay_safe")
            .is_some_and(|value| value != true)
    {
        return None;
    }
    let delay = error.get("retry_after_ms")?.as_u64()?;
    (1..=1000).contains(&delay).then_some(Retry {
        delay: Duration::from_millis(delay),
        fresh_brokerage: true,
        publication: false,
    })
}

/// Only the test-owned `tools call` evidence-read form supplies an implicit
/// expected scope. Mutation commands, mixed selectors and duplicate flags do not.
pub fn original_cli_scope<C: Invocation + ?Sized>(
    command: &C,
) -> Option<airk_core::service::ScopedConnectRequest> {
    let selection: airk_core::service::ScopedConnectRequest =
        original_cli_selector(command, "--catalog-session")?;
    selection.validate().ok()?;
    Some(selection)
}

pub fn original_cli_combined_scope<C: Invocation + ?Sized>(
    command: &C,
) -> Option<airk_core::service::CombinedConnectRequest> {
    let selection: airk_core::service::CombinedConnectRequest =
        original_cli_selector(command, "--combined-session")?;
    selection.validate().ok()?;
    Some(selection)
}

pub fn original_cli_arguments<C: Invocation + ?Sized>(command: &C) -> Option<Map<String, Value>> {
    let values = command.invocation_args();
    match values
        .iter()
        .position(|value| value.to_str() == Some("--args"))
    {
        Some(offset) => serde_json::from_str(values.get(offset + 1)?.to_str()?).ok(),
        None => Some(Map::new()),
    }
}

fn original_cli_selector<C: Invocation + ?Sized, T: serde::de::DeserializeOwned>(
    command: &C,
    flag: &str,
) -> Option<T> {
    let assigned = format!("{flag}=");
    let arguments = command
        .invocation_args()
        .into_iter()
        .map(|value| value.to_str())
        .collect::<Option<Vec<_>>>()?;
    let mut offset = 0;
    let mut selection = None;
    let mut runtime = false;
    while offset < arguments.len() && arguments[offset] != "tools" {
        let value = arguments[offset];
        if value == flag {
            if selection.is_some() {
                return None;
            }
            offset += 1;
            selection = Some(serde_json::from_str::<T>(arguments.get(offset)?).ok()?);
        } else if let Some(value) = value.strip_prefix(assigned.as_str()) {
            if selection.is_some() {
                return None;
            }
            selection = Some(serde_json::from_str::<T>(value).ok()?);
        } else if value == "--service-runtime-dir" {
            if runtime {
                return None;
            }
            runtime = true;
            offset += 1;
            if arguments.get(offset)?.is_empty() {
                return None;
            }
        } else {
            return None;
        }
        offset += 1;
    }
    let suffix = arguments.get(offset..)?;
    if suffix.len() != 3 && suffix.len() != 5 {
        return None;
    }
    if suffix[0] != "tools" || suffix[1] != "call" || !evidence_read_tool(suffix[2]) {
        return None;
    }
    if suffix.len() == 5
        && (suffix[3] != "--args" || !serde_json::from_str::<Value>(suffix[4]).ok()?.is_object())
    {
        return None;
    }
    selection
}

pub fn from_output(output: &std::process::Output, deadline: Instant) -> Option<Retry> {
    // Tool rejection exits 1. A killed or otherwise failed process is not a reply.
    if !output.status.success() && output.status.code() != Some(1) {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .ok()
        .and_then(|value| {
            if combined_body(&value).is_some() {
                None
            } else {
                within(&value, deadline)
            }
        })
}

/// One CLI invocation a deadline-bounded read can repeat unchanged: a child
/// process (`std::process::Command`) or, for the split's `airkctl` reads, the
/// in-process `common::ctl::Ctl` (which implements this trait in its module).
pub trait Invocation {
    /// The arguments after the program name.
    fn invocation_args(&self) -> Vec<&std::ffi::OsStr>;
    /// One run that gives up at `deadline` (`ErrorKind::TimedOut`).
    fn run_until(&self, deadline: Instant) -> std::io::Result<std::process::Output>;
}

impl Invocation for std::process::Command {
    fn invocation_args(&self) -> Vec<&std::ffi::OsStr> {
        self.get_args().collect()
    }
    fn run_until(&self, deadline: Instant) -> std::io::Result<std::process::Output> {
        process_once_until(self, deadline)
    }
}

pub fn output<C: Invocation + ?Sized>(command: &mut C) -> std::io::Result<std::process::Output> {
    let deadline = Instant::now() + Duration::from_secs(60);
    output_until(command, deadline)
}

pub fn output_until<C: Invocation + ?Sized>(
    command: &mut C,
    deadline: Instant,
) -> std::io::Result<std::process::Output> {
    let expected = original_cli_scope(&*command);
    let combined = original_cli_combined_scope(&*command);
    let arguments = original_cli_arguments(&*command);
    loop {
        let output = once_until(&*command, deadline)?;
        let retry = if expected.is_some() || combined.is_some() {
            (|| {
                if !output.status.success() && output.status.code() != Some(1) {
                    return None;
                }
                let envelope: Value = serde_json::from_slice(&output.stdout).ok()?;
                let arguments = arguments.as_ref()?;
                let decision = match (&expected, &combined) {
                    (Some(expected), None) => {
                        classify_for_scope_with_arguments(&envelope, expected, arguments)
                    }
                    (None, Some(expected)) => {
                        classify_for_combined_scope(&envelope, expected, arguments)
                    }
                    _ => None,
                };
                decision.filter(|retry| Instant::now() + retry.delay < deadline)
            })()
        } else {
            from_output(&output, deadline)
        };
        let Some(retry) = retry else {
            return Ok(output);
        };
        std::thread::sleep(retry.delay);
    }
}

/// One deadline-bounded run of a fixture-owned CLI read (see `Invocation`).
pub fn once_until<C: Invocation + ?Sized>(
    command: &C,
    deadline: Instant,
) -> std::io::Result<std::process::Output> {
    command.run_until(deadline)
}

/// A fixture-owned leaf CLI read, preserving its arguments, explicit environment
/// overrides and working directory (callers do not use env_clear/raw_arg/stdio).
/// Timeout kills and awaits only this client, never its separately owned service.
#[cfg(not(unix))]
fn process_once_until(
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

/// Unix native owners require SIGCHLD's default disposition. Tokio's process
/// driver installs a process-wide handler, so even a blocking-thread wrapper
/// cannot coexist with GroupChild. Keep this leaf under the same native owner.
#[cfg(unix)]
fn process_once_until(
    command: &std::process::Command,
    deadline: Instant,
) -> std::io::Result<std::process::Output> {
    use std::{
        io::{self, Read},
        process::{Command, Stdio},
        sync::mpsc,
        thread,
    };
    const CLEANUP_ALLOWANCE: Duration = Duration::from_secs(2);
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "CLI read deadline exhausted",
        ));
    }
    let mut bounded = Command::new(command.get_program());
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
    bounded
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = airk_process::GroupChild::spawn(&mut bounded)?;
    let (failed_sender, failed_receiver) = mpsc::channel();
    let mut stdout_reader = None;
    let mut stderr_reader = None;
    let mut reader_failed = false;
    let completed = (|| -> io::Result<()> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("piped CLI stdout missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("piped CLI stderr missing"))?;
        let spawn_reader = |mut stream: std::fs::File, name: &str| {
            let failed = failed_sender.clone();
            thread::Builder::new()
                .name(name.into())
                .spawn(move || -> io::Result<Vec<u8>> {
                    let mut bytes = Vec::new();
                    let result = stream.read_to_end(&mut bytes).map(|_| bytes);
                    if result.is_err() {
                        let _ = failed.send(());
                    }
                    result
                })
        };
        stdout_reader = Some(spawn_reader(stdout, "airk-read-stdout")?);
        stderr_reader = Some(spawn_reader(stderr, "airk-read-stderr")?);
        loop {
            if failed_receiver.try_recv().is_ok() {
                reader_failed = true;
                return Err(io::Error::other("CLI output reader failed"));
            }
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "CLI read deadline exhausted",
                ));
            }
            thread::sleep(Duration::from_millis(5).min(deadline.saturating_duration_since(now)));
        }
    })();
    // A terminal leader does not prove inherited pipes are closed. Drain its
    // exact private group on success, timeout and errors BEFORE joining readers.
    // Cleanup gets its own nonzero allowance after the request deadline. If the
    // native proof fails, return that error without joining blocked readers;
    // GroupChild's existing RAII owner retains/drains the group on drop.
    let status = child.terminate_and_wait(CLEANUP_ALLOWANCE)?;
    let join_reader =
        |reader: Option<thread::JoinHandle<io::Result<Vec<u8>>>>| -> io::Result<Vec<u8>> {
            match reader {
                Some(reader) => reader
                    .join()
                    .map_err(|_| io::Error::other("CLI output reader panicked"))?,
                None => Ok(Vec::new()),
            }
        };
    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    if reader_failed {
        // Preserve the actual read error after the owned group has drained.
        stdout?;
        stderr?;
        return Err(io::Error::other("CLI output reader failed"));
    }
    completed?;
    Ok(std::process::Output {
        status,
        stdout: stdout?,
        stderr: stderr?,
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
        let retry = if !evidence_read_tool(name) {
            None
        } else if combined_body(&reply).is_some() {
            match (backend.scoped_selection(), backend.combined_selection()) {
                (Some(expected), None) => {
                    classify_for_scope_with_arguments(&reply, expected, arguments)
                }
                (None, Some(expected)) => classify_for_combined_scope(&reply, expected, arguments),
                _ => None,
            }
            .filter(|retry| Instant::now() + retry.delay < deadline)
        } else {
            within(&reply, deadline)
        };
        let Some(retry) = retry else {
            return Ok(reply);
        };
        rejected(&reply);
        tokio::time::sleep(retry.delay).await;
    }
}

/// Explicit scope-bearing callers may repeat a known pre-query setup refusal;
/// an uncertain exchange or a crossed/revoked scope remains terminal.
pub async fn backend_for_scope(
    backend: &dyn airk_mcp::remote::RemoteTools,
    name: &str,
    arguments: &Map<String, Value>,
    expected: &airk_core::service::ScopedConnectRequest,
    deadline: Instant,
    mut rejected: impl FnMut(&Value),
) -> Result<Value> {
    expected.validate()?;
    loop {
        anyhow::ensure!(
            Instant::now() < deadline,
            "scoped read retry deadline exhausted"
        );
        let reply = tokio::time::timeout_at(
            deadline.into(),
            backend.call(name.into(), arguments.clone()),
        )
        .await??;
        let retry = evidence_read_tool(name)
            .then(|| {
                classify_for_scope_with_arguments(&reply, expected, arguments)
                    .filter(|retry| Instant::now() + retry.delay < deadline)
            })
            .flatten();
        let Some(retry) = retry else {
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
