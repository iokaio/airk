// SPDX-License-Identifier: Apache-2.0
//! Actual current-image CLI/HTTPS MCP reads retain policy-qualified provider
//! outage receipts. The public helper_unavailable code is retained for compatibility.
#![cfg(feature = "local-inference")]
mod common;
#[allow(dead_code)] // Shared fixture adapter; this binary uses only the pause boundary.
#[path = "mcp/source_runtime.rs"]
mod source_runtime;

use airk_core::{
    identity::Sha256Digest,
    readiness::{DefaultSearchPolicy, SemanticReadiness, SemanticState},
    service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, RequestKey,
    },
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const COLLECTION: &str = "outage-parity";
const SOURCE: &[u8] = b"authenticate stable original evidence\n";

struct Fixture {
    service: Arc<common::Service>,
    owner: tempfile::TempDir,
    root: PathBuf,
    data: PathBuf,
    control: PathBuf,
    project: airk_core::service::ProjectRegistration,
}

impl Fixture {
    fn start(policy: DefaultSearchPolicy) -> Result<Self> {
        let owner = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
        let root = owner.path().join("source");
        let data = owner.path().join("data");
        fs::create_dir(&root)?;
        fs::create_dir(&data)?;
        fs::write(root.join("evidence.txt"), SOURCE)?;
        let service = common::Service::reviewed(&root, &data, COLLECTION);
        service.settle_reviewed_index(&root, &data, Instant::now() + Duration::from_secs(90))?;
        // The saved default search policy has no public mutation endpoint.
        // Use its canonical API only under verified service/worker exclusion.
        service.with_stopped_owner(|project| {
            let kernel = airk_cli::bundled_assets::bind_service_models(
                airk_kernel::Kernel::open_service_collection(
                    std::path::Path::new(&project.root),
                    Some(project.data_root.clone().into()),
                    &project.collection,
                )?,
                &service.control,
            )?;
            kernel.set_default_semantic_search_policy(policy)
        })?;
        let project = service.project.clone();
        let control = service.control.clone();
        let data = PathBuf::from(&project.data_root);
        let fixture = Self {
            service,
            owner,
            root,
            data,
            control,
            project,
        };
        let response = fixture.rpc(Method::ConfigureWatch {
            project: fixture.project.binding,
            enabled: false,
        })?;
        ensure!(
            matches!(&response, Response::Watch { .. }),
            "{}",
            json!(response)
        );
        fixture.enqueue(JobKind::Inspect)?;
        let model = fixture.owner.path().join("model");
        fs::create_dir(&model)?;
        fs::write(
            model.join("vocabulary.json"),
            br#"{"authenticate":[1.0,0.0],"login":[0.99,0.01]}"#,
        )?;
        fs::write(
            model.join("model.json"),
            serde_json::to_vec(&json!({
                "schema":1,"name":"native-outage-parity","license":"Apache-2.0","dimensions":2,
                "sha256":airk_cli::runtime_pair::digest_file(&model.join("vocabulary.json"))?
            }))?,
        )?;
        let draft = fixture.owner.path().join("model-draft.json");
        fs::write(
            &draft,
            serde_json::to_vec(&json!({"operation":"install_model","directory":model}))?,
        )?;
        let output = fixture
            .admin_command()
            .args(["configure", "--file"])
            .arg(draft)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let accepted: Value = serde_json::from_slice(&output.stdout)?;
        fixture.wait_job(serde_json::from_value(accepted["job"]["id"].clone())?)?;
        fixture.enqueue(JobKind::Index)?;
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let observed = fixture.tool("airk_diagnostics", json!({"response_mode":"full"}))?;
            let readiness: SemanticReadiness =
                serde_json::from_value(observed["semantic_readiness"].clone())?;
            ensure!(readiness.state != SemanticState::Failed, "{observed}");
            if readiness.state == SemanticState::Ready {
                ensure!(
                    readiness.default_search_policy == policy,
                    "saved policy changed: {observed}"
                );
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "semantic fixture did not settle: {observed}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        source_runtime::pause_and_wait(&fixture.service)?;
        Ok(fixture)
    }

    fn rpc(&self, method: Method) -> Result<Response> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(airk_cli::service_runtime::request(&self.control, method))
    }

    fn command(&self) -> common::ctl::Ctl {
        common::scoped_command(&self.root, &self.data, COLLECTION)
    }
    fn admin_command(&self) -> common::ctl::Ctl {
        common::command(&self.root, &self.data, COLLECTION)
    }

    fn enqueue(&self, kind: JobKind) -> Result<()> {
        let Response::RequestEpoch { namespace, epoch } = self.rpc(Method::RequestEpoch)? else {
            anyhow::bail!("request epoch unavailable")
        };
        let nonce = airk_kernel::service_broker::new_instance_id()?;
        let response = self.rpc(Method::Enqueue {
            project: self.project.binding,
            caller: nonce,
            key: RequestKey {
                namespace,
                epoch,
                nonce,
            },
            kind,
        })?;
        let Response::Job { job } = response else {
            anyhow::bail!("job refused: {}", json!(response))
        };
        self.wait_job(job.id)
    }

    fn wait_job(&self, id: Sha256Digest) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let response = self.rpc(Method::Job { id })?;
            match response {
                Response::Job { job } if job.outcome.is_some() => {
                    if !matches!(job.outcome, Some(JobOutcome::Succeeded { .. })) {
                        anyhow::bail!(
                            "{job:?}; native evidence: {}",
                            common::job_failure_diagnostics(&self.control, &job)
                        );
                    }
                    return Ok(());
                }
                Response::Job { .. } => {}
                other => anyhow::bail!("job observation refused: {}", json!(other)),
            }
            ensure!(Instant::now() < deadline, "owned job deadline expired");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let output = common::read_retry::output(self.command().args([
            "tools",
            "call",
            name,
            "--args",
            &arguments.to_string(),
        ]))?;
        let envelope: Value = serde_json::from_slice(&output.stdout)?;
        ensure!(
            output.status.success() && envelope["isError"] != true,
            "{envelope}"
        );
        payload(&envelope)
    }

    fn stop_helper(&self) -> Result<()> {
        let health = self.rpc(Method::Health)?;
        let Response::Health {
            observation: before,
        } = &health
        else {
            anyhow::bail!("missing native health: {}", json!(health));
        };
        let response = self.rpc(Method::EmbeddingStatus)?;
        let Response::EmbeddingStatus {
            available: true,
            worker: Some(worker),
            process_id: None,
            provider: Some(provider),
            ..
        } = response
        else {
            anyhow::bail!("in-process provider not ready: {}", json!(response));
        };
        ensure!(
            provider.provider.service_instance == before.instance
                && provider.provider.provider_generation == worker,
            "crossed native provider identity"
        );
        provider.validate()?;
        let stopped = self.rpc(Method::StopEmbedding {
            worker,
            force: true,
        })?;
        ensure!(
            matches!(&stopped,Response::EmbeddingStopped{worker:exact} if *exact==worker)
                || matches!(&stopped,Response::Failure{code} if code=="embedding_stop_outcome_unknown"),
            "{}",
            json!(stopped)
        );
        fs::write(
            self.owner.path().join("native-stop-once.json"),
            serde_json::to_vec(&json!({"before":health,"provider":provider,"response":stopped}))?,
        )?;
        // Observe native completion; a pending stop never authorizes a second
        // force/stop mutation, a numerical PID signal, or an owner restart.
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let observed = self.rpc(Method::EmbeddingStatus)?;
            let settled = matches!(
                &observed,
                Response::EmbeddingStatus {
                    available: false,
                    worker: None,
                    process_id: None,
                    provider: None,
                    ..
                }
            );
            fs::write(
                self.owner.path().join("native-stop-observed.json"),
                serde_json::to_vec(&observed)?,
            )?;
            if settled {
                break;
            }
            ensure!(
                Instant::now() < end,
                "native provider drain did not complete: {}",
                json!(observed)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let Response::Health { observation: after } = self.rpc(Method::Health)? else {
            anyhow::bail!("service exited during provider stop")
        };
        ensure!(
            after.instance == before.instance
                && after.runtime_sha256 == before.runtime_sha256
                && after.executable_sha256 == before.executable_sha256,
            "provider stop restarted/replaced service"
        );
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = self.service.shutdown_reviewed() {
            self.owner.disable_cleanup(true);
            eprintln!(
                "outage fixture Stop uncertain; retained {}: {error:#}",
                self.owner.path().display()
            );
        }
    }
}

struct Session(common::http::HttpMcpSession);
impl Session {
    fn start(fixture: &Fixture, terse: bool) -> Result<Self> {
        let mut inner = common::http::HttpMcpSession::start_with_mode(
            &fixture.control,
            fixture.service.http_selection(),
            airk_core::usage::ClientFamily::Unknown,
            Some(if terse { "terse" } else { "full" }),
        )?;
        inner.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"provider-outage-parity","version":"1"}}}))?;
        let initialized = inner.receive(1)?;
        ensure!(initialized.get("error").is_none(), "{initialized}");
        inner.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        Ok(Self(inner))
    }
    fn read(&mut self, id: u64, name: &str, args: Value) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.0.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name.trim_start_matches("airk_"),"arguments":args}}))?;
            let reply = self.0.receive(i64::try_from(id)?)?;
            ensure!(reply.get("error").is_none(), "{reply}");
            if let Some(retry) = common::read_retry::within(&reply["result"], deadline) {
                std::thread::sleep(retry.delay);
            } else {
                return Ok(reply["result"].clone());
            }
        }
    }
}

fn payload(envelope: &Value) -> Result<Value> {
    if envelope.get("content").is_some() {
        Ok(serde_json::from_str(
            envelope["content"][0]["text"]
                .as_str()
                .context("native JSON missing")?,
        )?)
    } else {
        Ok(envelope.clone())
    }
}

fn comparable(mut value: Value) -> Result<Value> {
    common::read_retry::evidence_for_comparison(&mut value)?;
    Ok(value)
}

fn plan_comparable(mut value: Value) -> Value {
    fn dated(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                // Only transport identities and dated observations differ;
                // retain complete steps, receipts, budgets and completeness.
                for key in [
                    "request_id",
                    "elapsed_ms",
                    "observed_unix_ms",
                    "observed_from_unix_ms",
                    "observed_to_unix_ms",
                    "cache_age_ms",
                    "cached",
                ] {
                    fields.remove(key);
                }
                fields.values_mut().for_each(dated);
            }
            Value::Array(values) => values.iter_mut().for_each(dated),
            _ => {}
        }
    }
    dated(&mut value);
    value
}

fn command_value(fixture: &Fixture, args: &[&str]) -> Result<(bool, Value)> {
    let output = common::read_retry::output(fixture.command().args(args))?;
    ensure!(
        matches!(output.status.code(), Some(0 | 1)),
        "native client exited unexpectedly: {output:?}"
    );
    let value: Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("native output: {}", String::from_utf8_lossy(&output.stderr)))?;
    Ok((output.status.success(), payload(&value)?))
}

fn shell_value(fixture: &Fixture, args: &Value) -> Result<Value> {
    let input = format!("call airk_search {args}\nexit\n");
    ensure!(
        input.len() < 4096,
        "fixture input must fit the bounded pipe write"
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    let output = fixture
        .command()
        .args(["tools", "shell"])
        .input(input)
        .output_until(deadline)
        .context("owned shell read deadline expired")?;
    ensure!(
        output.status.success() && output.stdout.len() <= 1_048_576,
        "{output:?}"
    );
    let text = std::str::from_utf8(&output.stdout)?;
    let marker = if text.contains("MCP error:\n") {
        "MCP error:\n"
    } else {
        "Result:\n"
    };
    let body = text
        .split_once(marker)
        .context("shell did not render the complete result")?
        .1;
    serde_json::Deserializer::from_str(body)
        .into_iter::<Value>()
        .next()
        .context("shell JSON missing")?
        .map_err(Into::into)
}

#[test]
fn helper_outage_policy_results_and_receipts_match_actual_read_transports() -> Result<()> {
    for policy in [
        DefaultSearchPolicy::PartialHybrid,
        DefaultSearchPolicy::LexicalWhilePending,
        DefaultSearchPolicy::RequireComplete,
    ] {
        let fixture = Fixture::start(policy)?;
        let mut mcp = Session::start(&fixture, false)?;
        let mut terse = Session::start(&fixture, true)?;
        let warm = fixture.tool("airk_search", json!({"query":"authenticate","mode":"vector","response_mode":"full","verification":"off"}))?;
        ensure!(
            !warm["hits"].as_array().context("warm hits")?.is_empty(),
            "{warm}"
        );
        let publication = warm["publication"].clone();
        let terse_warm = terse.read(
            2,
            "search",
            json!({"query":"authenticate","verification":"off"}),
        )?;
        ensure!(terse_warm["isError"] != true, "{terse_warm}");
        let terse_warm = payload(&terse_warm)?;
        assert_eq!(terse_warm["publication"]["token"], publication["token"]);
        assert_eq!(terse_warm["execution"]["served_mode"], "hybrid");
        assert_eq!(terse_warm["execution"]["incomplete"], false);
        let lexical = fixture.tool("airk_search", json!({"query":"authenticate","mode":"lexical","response_mode":"full","verification":"off"}))?;
        fixture.stop_helper()?;
        let allowed = policy != DefaultSearchPolicy::RequireComplete;
        for (offset, verification) in ["off", "report", "require"].into_iter().enumerate() {
            let args =
                json!({"query":"authenticate","verification":verification,"response_mode":"full"});
            let wire = mcp.read(10 + offset as u64, "airk_search", args.clone())?;
            ensure!((wire["isError"] != true) == allowed, "{wire}");
            let expected = payload(&wire)?;
            let (success, convenience) = command_value(
                &fixture,
                &["search", "authenticate", "--verification", verification],
            )?;
            ensure!(success == allowed, "{convenience}");
            let (success, tools) = command_value(
                &fixture,
                &["tools", "call", "airk_search", "--args", &args.to_string()],
            )?;
            ensure!(success == allowed, "{tools}");
            for actual in [convenience, tools, shell_value(&fixture, &args)?] {
                assert_eq!(comparable(actual)?, comparable(expected.clone())?);
            }
            assert_eq!(expected["publication"], publication);
            assert_eq!(expected["semantic_readiness"]["state"], "ready");
            if allowed {
                assert_eq!(
                    expected["execution"],
                    json!({"version":1,"requested_mode":"default","served_mode":"lexical","incomplete":true,"incomplete_reason":"helper_unavailable"})
                );
                assert_eq!(expected["hits"], lexical["hits"]);
                if verification == "off" {
                    assert!(expected.get("source_verification").is_none());
                } else {
                    assert_eq!(expected["source_verification"]["status"], "verified");
                }
            } else {
                assert_eq!(expected["error"]["code"], "helper_unavailable");
                assert_eq!(expected["error"]["retryable"], false);
                assert_eq!(expected["requested_mode"], "default");
                assert!(common::read_retry::classify(&wire).is_none());
            }
        }
        for (offset, mode) in ["vector", "hybrid"].into_iter().enumerate() {
            let args = json!({"query":"authenticate","mode":mode,"verification":"off","response_mode":"full"});
            let envelope = mcp.read(20 + offset as u64, "airk_search", args.clone())?;
            ensure!(envelope["isError"] == true, "{envelope}");
            let expected = payload(&envelope)?;
            assert_eq!(expected["error"]["code"], "helper_unavailable");
            assert_eq!(expected["requested_mode"], mode);
            assert_eq!(expected["publication"], publication);
            assert!(common::read_retry::classify(&envelope).is_none());
            let (success, actual) = command_value(
                &fixture,
                &[
                    "search",
                    "authenticate",
                    "--mode",
                    mode,
                    "--verification",
                    "off",
                ],
            )?;
            ensure!(!success, "{actual}");
            assert_eq!(comparable(actual)?, comparable(expected.clone())?);
            let (success, actual) = command_value(
                &fixture,
                &["tools", "call", "airk_search", "--args", &args.to_string()],
            )?;
            ensure!(!success, "{actual}");
            assert_eq!(comparable(actual)?, comparable(expected)?);
        }
        let semantic = mcp.read(25, "search", json!({"query":"authenticate","semantic":true,"verification":"off","response_mode":"full"}))?;
        ensure!(semantic["isError"] == true, "{semantic}");
        let semantic = payload(&semantic)?;
        let (success, cli_semantic) = command_value(
            &fixture,
            &[
                "search",
                "authenticate",
                "--semantic",
                "--verification",
                "off",
            ],
        )?;
        ensure!(!success, "{cli_semantic}");
        assert_eq!(semantic["requested_mode"], "vector");
        assert_eq!(comparable(cli_semantic)?, comparable(semantic)?);
        // native read: a whole plan executes once; helper outages never authorize replay.
        let plan_args = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","response_mode":"full",
            "plan":{"steps":[{"id":"search","tool":"search","args":{"query":"authenticate","verification":"off"}}]}});
        let plan_envelope = mcp.read(30, "parse", plan_args.clone())?;
        let plan = payload(&plan_envelope)?;
        assert_eq!(plan["publication"], publication["token"]);
        assert_eq!(
            plan["status"],
            if allowed { "partial" } else { "failed" },
            "{plan}"
        );
        assert!(
            !plan["completeness"]
                .as_array()
                .context("plan completeness")?
                .is_empty()
        );
        let request = fixture.owner.path().join("plan.json");
        fs::write(&request, serde_json::to_vec(&plan_args)?)?;
        let (_, cli_plan) = command_value(
            &fixture,
            &[
                "parse",
                "--request",
                request.to_str().context("plan path")?,
                "--execute",
            ],
        )?;
        assert_eq!(cli_plan["status"], plan["status"]);
        assert_eq!(cli_plan["completeness"], plan["completeness"]);
        assert_eq!(cli_plan["evidence"], plan["evidence"]);
        assert_eq!(plan_comparable(cli_plan), plan_comparable(plan.clone()));
        let step = &plan["steps"][0];
        if allowed {
            assert_eq!(
                step["result"]["execution"]["incomplete_reason"],
                "helper_unavailable"
            );
            for id in [40, 41] {
                let result = terse.read(
                    id,
                    "search",
                    json!({"query":"authenticate","verification":"off"}),
                )?;
                ensure!(result["isError"] != true, "{result}");
                let value = payload(&result)?;
                assert_eq!(value["execution"], step["result"]["execution"]);
                assert_eq!(value["publication"]["token"], publication["token"]);
                assert_eq!(value["hits"], lexical["hits"]);
            }
            fs::write(
                fixture.root.join("evidence.txt"),
                b"intentionally changed fixture bytes\n",
            )?;
            let required =
                json!({"query":"authenticate","verification":"require","response_mode":"full"});
            let failed = mcp.read(50, "search", required.clone())?;
            ensure!(failed["isError"] == true, "{failed}");
            let failed = payload(&failed)?;
            assert_eq!(failed["error"]["code"], "verification_required_unmet");
            assert_eq!(
                failed["execution"]["incomplete_reason"],
                "helper_unavailable"
            );
            assert_eq!(failed["publication"], publication);
            assert_eq!(failed["hits"], lexical["hits"]);
            let required_json = required.to_string();
            for args in [
                vec!["search", "authenticate", "--verification", "require"],
                vec!["tools", "call", "airk_search", "--args", &required_json],
            ] {
                let (success, actual) = command_value(&fixture, &args)?;
                ensure!(!success, "{actual}");
                assert_eq!(comparable(actual)?, comparable(failed.clone())?);
            }
            assert_eq!(
                comparable(shell_value(&fixture, &required)?)?,
                comparable(failed)?
            );
            assert_eq!(
                fs::read(fixture.root.join("evidence.txt"))?,
                b"intentionally changed fixture bytes\n"
            );
            fs::write(fixture.root.join("evidence.txt"), SOURCE)?;
        } else {
            let failed = payload(&step["result"])?;
            assert_eq!(failed["error"]["code"], "helper_unavailable");
            assert_eq!(failed["requested_mode"], "default");
        }
        let observed = fixture.rpc(Method::EmbeddingStatus)?;
        ensure!(
            matches!(
                &observed,
                Response::EmbeddingStatus {
                    available: false,
                    ..
                }
            ),
            "reads restarted helper: {}",
            json!(observed)
        );
        assert_eq!(fs::read(fixture.root.join("evidence.txt"))?, SOURCE);
    }
    Ok(())
}
