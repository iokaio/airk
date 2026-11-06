// SPDX-License-Identifier: Apache-2.0
//! Real native policy admission and evidence transport regression. Reuses the
//! enclosing isolated process owner; no installed runtime or source is modified.
use super::*;
use airk_core::{
    catalog_host::GrantDraftSelection,
    catalog_management::CatalogOperation,
    catalog_source_draft::{
        SourceMutationDraft, SourceMutationDraftSelection, SourceMutationSelection,
    },
    index_catalog::PathScope,
    service::{JobOutcome, RequestKey},
};

pub(super) async fn policy_key(f: &Fixture, nonce: u8) -> Result<RequestKey> {
    let response =
        airk_cli::service_runtime::request(&f.control, ControlMethod::RequestEpoch).await?;
    let ControlResponse::RequestEpoch { namespace, epoch } = response else {
        anyhow::bail!(
            "policy request epoch refused: {}",
            serde_json::to_string(&response)?
        );
    };
    Ok(RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([nonce; 16]),
    })
}

pub(super) fn policy_revision(f: &Fixture) -> Result<CatalogRevision> {
    Ok(ServiceRegistry::open_saved(&f.control)?
        .catalog_state()?
        .context("policy catalog missing")?
        .revision)
}

pub(super) async fn policy_rpc_cli(f: &Fixture, method: ControlMethod) -> Result<Value> {
    let request = tempfile::NamedTempFile::new_in(f.owner.path())?;
    fs::write(request.path(), serde_json::to_vec(&method)?)?;
    let mut command = Command::new(&f.candidate);
    command
        .args(["service", "rpc", "--runtime-dir"])
        .arg(&f.control)
        .arg("--request-file")
        .arg(request.path());
    let output = tokio::task::spawn_blocking(move || {
        let output = read_retry::output(&mut command);
        drop(request);
        output
    })
    .await??;
    ensure!(
        output.status.success(),
        "native metadata CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("native metadata CLI omitted complete response")
}

async fn policy_draft(
    f: &Fixture,
    nonce: u8,
    change: SourceMutationSelection,
) -> Result<SourceMutationDraft> {
    let selection = SourceMutationDraftSelection {
        key: policy_key(f, nonce).await?,
        expected_catalog_revision: policy_revision(f)?,
        index_id: f.indexes[2],
        change,
    };
    let deadline = Instant::now() + Duration::from_secs(90);
    let response = loop {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::CatalogSourceMutationDraft {
                selection: selection.clone(),
            },
        )
        .await?;
        if (matches!(response, ControlResponse::SourceChangeBusy { .. })
            || matches!(&response,ControlResponse::Failure{code} if code=="registry_busy"))
            && Instant::now() < deadline
        {
            // Read-only preparation may wait for accepted setup work. Never
            // replay or bypass a mutating admission.
            tokio::time::sleep(Duration::from_millis(25)).await;
            continue;
        }
        break response;
    };
    let ControlResponse::CatalogSourceMutationDraft { draft } = response else {
        anyhow::bail!(
            "native policy draft refused: {}; fixture diagnostics: {}",
            serde_json::to_string(&response)?,
            fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default()
        );
    };
    draft.validate()?;
    Ok(draft)
}

async fn apply_policy(f: &Fixture, draft: &SourceMutationDraft) -> Result<()> {
    // Mutating IPC is submitted once. Its saved request/preview are retained for
    // exact receipt recovery; transport uncertainty does not allocate another key.
    let mut response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogApply {
            request: draft.request.clone(),
            preview_digest: draft.preview_digest,
        },
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        match &response {
            ControlResponse::CatalogApplied { .. } => break,
            ControlResponse::CatalogReceipt {
                receipt: Some(receipt),
                fence: Some(airk_core::catalog_fences::FenceObservation::Applied(fenced)),
            } => {
                response = ControlResponse::CatalogApplied {
                    receipt: receipt.clone(),
                    fenced_receipt: Some(fenced.clone()),
                };
                break;
            }
            ControlResponse::CatalogOutcomeUnknown {
                caller,
                key,
                request_digest,
                ..
            } => {
                ensure!(
                    *caller == draft.request.caller
                        && *key == draft.request.key
                        && *request_digest == draft.request.identity()?,
                    "unknown policy outcome changed request identity"
                );
            }
            ControlResponse::CatalogReceipt {
                receipt: None,
                fence: Some(airk_core::catalog_fences::FenceObservation::Pending(_)),
            } => (),
            _ => anyhow::bail!(
                "native policy application refused: {}",
                serde_json::to_string(&response)?
            ),
        }
        ensure!(
            Instant::now() < deadline,
            "policy fence did not settle: {}",
            serde_json::to_string(&response)?
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
        // Observe the same accepted request; never resubmit CatalogApply.
        response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::CatalogReceipt {
                caller: draft.request.caller,
                key: draft.request.key.clone(),
            },
        )
        .await?;
    }
    let ControlResponse::CatalogApplied {
        receipt,
        fenced_receipt,
    } = response
    else {
        anyhow::bail!(
            "native policy application refused: {}",
            serde_json::to_string(&response)?
        );
    };
    assert_eq!(receipt.request_digest, draft.request.identity()?);
    let fenced = fenced_receipt.context("policy application omitted native worker fence")?;
    fenced.validate()?;
    Ok(())
}

fn policy_parity(label: &str, left: &Value, right: &Value) -> Result<()> {
    // Authenticate each actual transport body before normalizing its documented
    // request/timing identities. Source text and complete logical data stay literal.
    assert_parity(label, &body(left)?, &body(right)?)
}

async fn wait_policy(f: &Fixture) -> Result<airk_core::catalog_policy::PolicySnapshot> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::CatalogPolicyStatus {
                index_id: f.indexes[2],
            },
        )
        .await?;
        let ControlResponse::CatalogPolicyStatus {
            snapshot: Some(snapshot),
            ..
        } = response
        else {
            anyhow::bail!(
                "native policy observation omitted durable intent: {}",
                serde_json::to_string(&response)?
            );
        };
        snapshot.validate()?;
        let response =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Job { id: snapshot.job })
                .await?;
        let ControlResponse::Job { job } = response else {
            anyhow::bail!(
                "native policy job observation refused: {}",
                serde_json::to_string(&response)?
            );
        };
        if let Some(outcome) = job.outcome {
            ensure!(
                matches!(outcome, JobOutcome::Succeeded { .. }),
                "native policy job failed: {outcome:?}; policy: {snapshot:?}"
            );
            if snapshot.application.is_some() {
                return Ok(snapshot);
            }
        }
        ensure!(
            Instant::now() < deadline,
            "native policy removal remained pending: {snapshot:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn renew_policy_grant(f: &mut Fixture, nonce: u8) -> Result<()> {
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogGrantDraft {
            context_key: f.session.context_key,
            selection: GrantDraftSelection {
                key: policy_key(f, nonce).await?,
                expected_catalog_revision: policy_revision(f)?,
                project_ids: vec![f.projects[2]],
                index_ids: Some(vec![f.indexes[2]]),
            },
        },
    )
    .await?;
    let ControlResponse::CatalogGrantDraft { draft } = response else {
        anyhow::bail!(
            "policy-qualified grant review refused: {}",
            serde_json::to_string(&response)?
        );
    };
    let CatalogOperation::CreateGrant { grant } = &draft.request.operation else {
        anyhow::bail!("native grant review omitted its operation");
    };
    let grant = grant.clone();
    assert!(
        grant
            .memberships
            .iter()
            .all(|member| member.policy_epoch > 0 && member.policy_digest.is_some())
    );
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogApply {
            request: draft.request,
            preview_digest: draft.preview_digest,
        },
    )
    .await?;
    let ControlResponse::CatalogApplied { receipt, .. } = response else {
        anyhow::bail!(
            "policy-qualified grant apply refused: {}",
            serde_json::to_string(&response)?
        );
    };
    f.session.grant_id = grant.id;
    f.session.expected_grant_revision = grant.revision;
    f.session.expected_catalog_revision = receipt.catalog_revision;
    Ok(())
}

fn trim_selector() -> SourceMutationSelection {
    SourceMutationSelection::TrimPath {
        source_root_id: SourceRootId::from_identity(Incarnation::from_bytes([52; 16])),
        kind: PathScope::Subtree,
        path: "private".into(),
    }
}

fn forbid_trimmed_bytes(value: &Value) -> Result<()> {
    ensure!(
        !serde_json::to_string(value)?.contains("P9_BLOCKED_SENTINEL"),
        "blocked retained source escaped: {value}"
    );
    fn visit(value: &Value) -> Result<()> {
        match value {
            Value::Array(values) => {
                for value in values {
                    visit(value)?;
                }
            }
            Value::Object(object) => {
                ensure!(
                    object.get("status") != Some(&json!("not_modified")),
                    "blocked representation validator was renewed: {value}"
                );
                if let Some(paths) = object.get("paths").and_then(Value::as_array) {
                    ensure!(
                        !paths.iter().any(|path| path
                            .as_str()
                            .is_some_and(|path| path.starts_with("private/"))),
                        "blocked path escaped inventory: {value}"
                    );
                }
                if object.contains_key("source_hash")
                    || object.contains_key("content_hash")
                    || object.contains_key("start_line")
                {
                    ensure!(
                        !object
                            .get("path")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.starts_with("private/")),
                        "blocked retained location escaped: {value}"
                    );
                }
                for value in object.values() {
                    visit(value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(value)?;
    if let Ok(logical) = body(value) {
        visit(&logical)?;
    }
    Ok(())
}

async fn retire_policy_index(
    f: &Fixture,
    nonce: u8,
    expected: &airk_core::catalog_policy::PolicySnapshot,
) -> Result<()> {
    let draft = policy_draft(f, nonce, SourceMutationSelection::RetireIndex).await?;
    apply_policy(f, &draft).await?;
    let registration = &f.registrations[2];
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if ServiceRegistry::open_saved(&f.control)?.project_was_removed(registration.binding)? {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "retired policy cleanup did not complete; diagnostics: {}",
            fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let method = ControlMethod::CatalogPolicyStatus {
        index_id: f.indexes[2],
    };
    let response = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
    assert_eq!(
        policy_rpc_cli(f, method).await?,
        serde_json::to_value(&response)?,
        "retired policy status lost canonical fields"
    );
    let ControlResponse::CatalogPolicyStatus {
        snapshot: Some(snapshot),
        job: Some(job),
        ..
    } = response
    else {
        anyhow::bail!(
            "retirement lost permanent policy status: {}",
            serde_json::to_string(&response)?
        );
    };
    assert_eq!(snapshot.policy, expected.policy);
    assert_eq!(snapshot.job, expected.job);
    assert_eq!(job.id, expected.job);
    assert!(
        matches!(
            job.state,
            airk_core::service::JobState::Succeeded
                | airk_core::service::JobState::Failed
                | airk_core::service::JobState::Canceled
        ),
        "retirement left active policy work: {job:?}"
    );
    let capsule = airk_store::current::policy_capsule::PolicyCapsule::load(
        &Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection),
    )?
    .context("retirement lost enforcement copy")?;
    assert_eq!(capsule.policy, expected.policy);
    let registry = ServiceRegistry::open_saved(&f.control)?;
    assert!(
        !registry
            .pending_project_removals(None, 32)?
            .iter()
            .any(|item| item.binding == registration.binding)
    );
    drop(registry);
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogPolicyResume {
            index_id: f.indexes[2],
            operation_id: snapshot.operation_id,
            caller: draft.request.caller,
            key: policy_key(f, nonce + 1).await?,
        },
    )
    .await?;
    assert!(
        matches!(response, ControlResponse::Failure { .. }),
        "retired index admitted fresh policy work: {}",
        serde_json::to_string(&response)?
    );
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Register {
            project: registration.clone(),
        },
    )
    .await?;
    assert!(
        matches!(response, ControlResponse::Failure { .. }),
        "registration bypassed retired policy: {}",
        serde_json::to_string(&response)?
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires source-bound pre-policy Store helper in AIRK_PRE_POLICY_STORE_HELPER; run serially"]
async fn permanent_trim_rejects_actual_pre_policy_store_handlers_after_retirement() -> Result<()> {
    let helper = PathBuf::from(
        std::env::var_os("AIRK_PRE_POLICY_STORE_HELPER")
            .context("select the independently built entry-revision Store test helper")?,
    )
    .canonicalize()?;
    ensure!(helper.is_file(), "pre-policy test helper is unavailable");
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let registration = &f.registrations[2];
    let probe = |expect: &str| {
        let mut command = Command::new(&helper);
        command
            .args([
                "--exact",
                "p9_pre_policy_probe::p9_pre_policy_owner_probe",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("AIRK_PRE_POLICY_ROOT", &registration.root)
            .env("AIRK_PRE_POLICY_DATA", &registration.data_root)
            .env("AIRK_PRE_POLICY_COLLECTION", &registration.collection)
            .env("AIRK_PRE_POLICY_CONTROL", &f.control)
            .env("AIRK_PRE_POLICY_EXPECT", expect)
            .stdin(Stdio::null());
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "entry-revision native {expect} probe failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
            "entry-revision helper did not execute its exact native probe"
        );
        Ok::<_, anyhow::Error>(())
    };
    probe("read_works")?;
    let draft = policy_draft(&f, 101, trim_selector()).await?;
    apply_policy(&f, &draft).await?;
    let snapshot = wait_policy(&f).await?;
    retire_policy_index(&f, 102, &snapshot).await?;
    let directory = Path::new(&registration.data_root)
        .join("collections-1.4")
        .join(&registration.collection);
    let before_owner = fs::read(directory.join("service-owner.json"))?;
    let before_capsule = fs::read(directory.join("source-policy.json"))?;
    probe("read_refused")?;
    probe("purge_refused")?;
    assert_eq!(
        fs::read(directory.join("service-owner.json"))?,
        before_owner
    );
    assert_eq!(
        fs::read(directory.join("source-policy.json"))?,
        before_capsule
    );
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn permanent_trim_fences_retained_grants_and_preserves_native_cli_mcp_results() -> Result<()>
{
    let mut f = Fixture::new(true)?;
    f.session.project_ids = vec![f.projects[2]];
    f.session.index_ids = Some(vec![f.indexes[2]]);
    let registration = f.registrations[2].clone();
    let blocked = Path::new(&registration.root).join("private/hidden.rs");
    let allowed = Path::new(&registration.root).join("api/types.rs");
    let blocked_source =
        "// P9_BLOCKED_SENTINEL\npub struct HiddenTrimEvidence;\npub fn hiddenneedle() {}\n";
    let allowed_source =
        "// SPDX-License-Identifier: Apache-2.0\npub struct AllowedTrimEvidence;\n";
    fs::write(&blocked, blocked_source)?;
    fs::write(&allowed, allowed_source)?;
    airk_kernel::Kernel::open_service_collection(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?
    .index()?;
    let service = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let mut old_mcp = McpSession::start(&f)?;
    let query = json!({"query":"hiddenneedle","mode":"lexical","limit":16,"max_bytes":8192,"verification":"off","response_mode":"full"});
    let before = native_read(backend.as_ref(), "airk_search", &query).await?;
    assert!(
        serde_json::to_string(&before)?.contains("P9_BLOCKED_SENTINEL"),
        "fixture did not expose pre-trim evidence: {before}"
    );
    let mcp_before = old_mcp.call(10, "airk_search", query.clone())?;
    assert!(
        serde_json::to_string(&mcp_before["result"])?.contains("P9_BLOCKED_SENTINEL"),
        "MCP did not retain a pre-trim evidence connection: {mcp_before}"
    );
    let old_info = native_read(
        backend.as_ref(),
        "airk_file_info",
        &json!({"path":"private/hidden.rs","verification":"off","response_mode":"full"}),
    )
    .await?;
    let old_info = body(&old_info)?;
    let old_info = member_body(&old_info, &old_info["members"][0])?;
    let old_validator = old_info["read"]["validator"]
        .as_str()
        .context("pre-trim representation validator missing")?
        .to_owned();
    let old_revision = old_info["read"]["publication"]["token"]
        .as_str()
        .context("pre-trim publication missing")?
        .to_owned();

    let draft = policy_draft(&f, 81, trim_selector()).await?;
    assert!(matches!(
        draft.request.operation,
        CatalogOperation::ChangeIndexPolicy { .. }
    ));
    apply_policy(&f, &draft).await?;
    let snapshot = wait_policy(&f).await?;
    assert_eq!(snapshot.policy.denies.len(), 1);
    assert_eq!(snapshot.policy.denies[0].path, "private");
    let applied = snapshot
        .application
        .as_ref()
        .context("policy application absent")?;
    assert!(applied.removal_publication.is_some());
    assert!(!applied.no_index);
    assert!(applied.removed_artifacts >= 2);
    let method = ControlMethod::CatalogPolicyStatus {
        index_id: f.indexes[2],
    };
    let native_status = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
    assert_eq!(
        policy_rpc_cli(&f, method).await?,
        serde_json::to_value(native_status)?,
        "CLI policy status dropped canonical fields"
    );
    assert!(
        ServiceRegistry::open_saved(&f.control)?
            .get(registration.binding)?
            .paused,
        "explicit retained removal changed the user's pause setting"
    );

    // native read: retained-grant rejection is a one-shot negative test.
    if let Ok(reply) = backend
        .call("airk_search".into(), query.as_object().unwrap().clone())
        .await
    {
        assert_eq!(
            reply["isError"], true,
            "old grant survived policy activation: {reply}"
        );
        forbid_trimmed_bytes(&reply)?;
    }
    // native read: old MCP session must not deliver its cached pre-trim evidence.
    if let Ok(reply) = old_mcp.call(40, "airk_search", query.clone()) {
        assert_eq!(
            reply["result"]["isError"], true,
            "old MCP grant survived trim: {reply}"
        );
        forbid_trimmed_bytes(&reply)?;
    }
    backend.close();
    backend.shutdown().await?;
    drop(old_mcp);
    assert!(
        airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session)
            .await
            .is_err(),
        "unchanged epoch-zero grant was silently renewed"
    );
    renew_policy_grant(&mut f, 82).await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let allowed_args = json!({"path":"api/types.rs","start_line":1,"end_line":2,"max_bytes":4096,"verification":"off","response_mode":"full"});
    let native = native_read(backend.as_ref(), "airk_read_file", &allowed_args).await?;
    ensure!(
        native["isError"] != true,
        "allowed retained evidence refused after trim: {native}"
    );
    assert!(serde_json::to_string(&native)?.contains("AllowedTrimEvidence"));
    policy_parity(
        "trim allowed CLI",
        &native,
        &f.cli("airk_read_file", &allowed_args).await?,
    )?;
    policy_parity(
        "trim allowed MCP",
        &native,
        &mcp.call(50, "airk_read_file", allowed_args)?["result"],
    )?;
    let report_args = json!({"path":"api/types.rs","start_line":1,"end_line":2,"max_bytes":4096,"verification":"report","response_mode":"full"});
    let report = native_read(backend.as_ref(), "airk_read_file", &report_args).await?;
    let report_body = body(&report)?;
    let verified_member =
        airk_core::combined_reads::shared_values::materialize_combined_member_result(
            &report_body,
            f.indexes[2],
        )?;
    assert_eq!(
        verified_member["source_verification"]["status"], "verified",
        "{verified_member}"
    );
    assert_eq!(
        verified_member["source_verification"]["counts"]["matched_sources"],
        1
    );
    policy_parity(
        "trim verified CLI",
        &report,
        &f.cli("airk_read_file", &report_args).await?,
    )?;
    policy_parity(
        "trim verified MCP",
        &report,
        &mcp.call(51, "airk_read_file", report_args)?["result"],
    )?;

    let pin_method = |path: &Path| ControlMethod::NativeRead {
        project: registration.binding,
        request: airk_core::native_reads::NativeRead::PrepareInputFile {
            path: path.to_str().unwrap().into(),
            max_source_bytes: 1 << 20,
        },
        usage_client: airk_core::usage::ClientFamily::Unknown,
    };
    let allowed_pin = airk_cli::service_runtime::request(&f.control, pin_method(&allowed)).await?;
    let ControlResponse::NativeRead { result } = allowed_pin else {
        anyhow::bail!(
            "allowed native source pin refused: {}",
            serde_json::to_string(&allowed_pin)?
        );
    };
    let input: airk_core::service_configuration::InputFile = serde_json::from_value(result)?;
    input.validate()?;
    {
        use sha2::{Digest, Sha256};
        assert_eq!(
            input.sha256.to_string(),
            format!("{:x}", Sha256::digest(allowed_source.as_bytes()))
        );
    }
    // native read: a blocked native management pin must refuse before hashing.
    let blocked_pin = airk_cli::service_runtime::request(&f.control, pin_method(&blocked)).await?;
    let blocked_pin = serde_json::to_value(blocked_pin)?;
    assert_eq!(
        blocked_pin["result"]["error"]["code"], "source_policy_unavailable",
        "blocked native pin did not preserve its typed refusal: {blocked_pin}"
    );
    assert!(blocked_pin["result"].get("sha256").is_none());
    forbid_trimmed_bytes(&blocked_pin)?;

    let cases = [
        ("airk_search", query.clone()),
        (
            "airk_list_files",
            json!({"prefix":"private/","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_symbols",
            json!({"name":"HiddenTrimEvidence","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_code_facts",
            json!({"name":"HiddenTrimEvidence","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_document_facts",
            json!({"path":"private/policy.md","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_references",
            json!({"name":"hiddenneedle","limit":16,"max_bytes":8192}),
        ),
        ("airk_file_info", json!({"path":"private/hidden.rs"})),
        (
            "airk_read_file",
            json!({"path":"private/hidden.rs","start_line":1,"end_line":3,"max_bytes":4096}),
        ),
        (
            "airk_context",
            json!({"path":"private/hidden.rs","line":2,"before":1,"after":1,"max_bytes":4096}),
        ),
        (
            "airk_imports",
            json!({"path":"private/hidden.rs","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_outline",
            json!({"path":"private/hidden.rs","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_relationships",
            json!({"symbol":"file:private/hidden.rs","direction":"outgoing","limit":16,"max_bytes":8192}),
        ),
        (
            "airk_changes",
            json!({"selectors":[{"kind":"path","path":"private/hidden.rs"}],"limit":16,"max_bytes":8192}),
        ),
        (
            "airk_file_info",
            json!({"path":"private/hidden.rs","if_evidence":old_validator}),
        ),
        (
            "airk_read_file",
            json!({"path":"private/hidden.rs","start_line":1,"end_line":3,"revision":old_revision,"max_bytes":4096}),
        ),
        ("airk_status", json!({})),
        ("airk_diagnostics", json!({})),
    ];
    for (offset, (tool, mut args)) in cases.into_iter().enumerate() {
        if !matches!(tool, "airk_status" | "airk_diagnostics") {
            args["verification"] = json!("off");
        }
        args["response_mode"] = json!("full");
        // native read: blocked exact selectors deliberately exercise refusal once.
        let native = backend
            .call(tool.into(), args.as_object().unwrap().clone())
            .await?;
        if tool == "airk_status" {
            let status = body(&native)?;
            let member =
                airk_core::combined_reads::shared_values::materialize_combined_member_result(
                    &status,
                    f.indexes[2],
                )?;
            assert_eq!(
                member["source_policy"]["control_dependencies"],
                serde_json::to_value(airk_ingest::permanent_policy::control_dependencies())?,
                "complete native status member: {member}; full reply: {status}"
            );
        }
        forbid_trimmed_bytes(&native)?;
        let cli = f.cli(tool, &args).await?;
        forbid_trimmed_bytes(&cli)?;
        let mcp = mcp.call(100 + offset as u64 * 8, tool, args)?;
        forbid_trimmed_bytes(&mcp["result"])?;
        policy_parity(&format!("trim {tool} CLI"), &native, &cli)?;
        policy_parity(&format!("trim {tool} MCP"), &native, &mcp["result"])?;
    }
    backend.close();
    backend.shutdown().await?;
    drop(mcp);
    drop(service);
    let kernel = airk_kernel::Kernel::open_service_collection(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?;
    kernel.index()?;
    assert!(
        !kernel
            .list_files("private/", 16, 8192)?
            .paths
            .iter()
            .any(|path| path.starts_with("private/")),
        "native rebuild resurrected denied source"
    );
    drop(kernel);
    let restarted = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let result = native_read(
        backend.as_ref(),
        "airk_list_files",
        &json!({"prefix":"private/","limit":16,"max_bytes":8192,"verification":"off"}),
    )
    .await?;
    forbid_trimmed_bytes(&result)?;
    backend.close();
    backend.shutdown().await?;
    let unblock = policy_draft(
        &f,
        83,
        SourceMutationSelection::UnblockPath {
            rule_id: snapshot.policy.denies[0].rule_id,
        },
    )
    .await?;
    apply_policy(&f, &unblock).await?;
    let unblocked = wait_policy(&f).await?;
    assert!(unblocked.policy.denies.is_empty());
    assert!(unblocked.policy.epoch > snapshot.policy.epoch);
    renew_policy_grant(&mut f, 84).await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let before_reindex = native_read(
        backend.as_ref(),
        "airk_list_files",
        &json!({"prefix":"private/","limit":16,"max_bytes":8192,"verification":"off"}),
    )
    .await?;
    forbid_trimmed_bytes(&before_reindex)?;
    let status_args =
        json!({"project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"response_mode":"full"});
    let status = native_read(backend.as_ref(), "airk_status", &status_args).await?;
    let status_body = body(&status)?;
    let member = airk_core::combined_reads::shared_values::materialize_combined_member_result(
        &status_body,
        f.indexes[2],
    )?;
    assert_eq!(member["source_policy"]["blocked"], false, "{member}");
    assert_eq!(member["source_policy"]["removal_pending"], false);
    let cli_status = f.cli("airk_status", &status_args).await?;
    let mut status_mcp = McpSession::start(&f)?;
    let mcp_status = status_mcp.call(900, "airk_status", status_args)?;
    policy_parity("unblocked status CLI", &status, &cli_status)?;
    policy_parity("unblocked status MCP", &status, &mcp_status["result"])?;
    drop(status_mcp);
    backend.close();
    backend.shutdown().await?;
    drop(restarted);
    let kernel = airk_kernel::Kernel::open_service_collection(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?;
    kernel.index()?;
    assert!(
        kernel
            .list_files("private/", 16, 8192)?
            .paths
            .iter()
            .any(|path| path == "private/hidden.rs"),
        "explicit indexing after reviewed Unblock did not restore eligible evidence"
    );
    drop(kernel);
    let _reopened = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let restored = native_read(backend.as_ref(), "airk_search", &query).await?;
    assert!(
        serde_json::to_string(&restored)?.contains("P9_BLOCKED_SENTINEL"),
        "reviewed Unblock did not survive native restart: {restored}; fixture diagnostics: {}",
        fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default()
    );
    backend.close();
    backend.shutdown().await?;
    let retrim = policy_draft(&f, 85, trim_selector()).await?;
    apply_policy(&f, &retrim).await?;
    let retrimmed = wait_policy(&f).await?;
    retire_policy_index(&f, 86, &retrimmed).await?;
    f.assert_sources_unchanged()?;
    assert_eq!(fs::read_to_string(blocked)?, blocked_source);
    assert_eq!(fs::read_to_string(allowed)?, allowed_source);
    Ok(())
}

#[tokio::test]
async fn permanent_trim_quota_failure_supports_exact_resume_or_retirement() -> Result<()> {
    for resume in [false, true] {
        let mut f = Fixture::new(true)?;
        f.session.project_ids = vec![f.projects[2]];
        f.session.index_ids = Some(vec![f.indexes[2]]);
        let registration = &f.registrations[2];
        let quota = airk_store::quota::Quota::open_service(Path::new(&registration.data_root))?;
        let status = quota.status()?;
        let original = quota.policy()?;
        let mut limited = original.clone();
        limited.collections.insert(
            registration.collection.clone(),
            status.collections[&registration.collection].used_bytes
                + status.collections[&registration.collection].reserved_bytes
                + 131_072,
        );
        quota.configure(limited)?;
        let _service = f.start().await?;
        let draft = policy_draft(&f, 91, trim_selector()).await?;
        apply_policy(&f, &draft).await?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let failed = loop {
            let response = airk_cli::service_runtime::request(
                &f.control,
                ControlMethod::CatalogPolicyStatus {
                    index_id: f.indexes[2],
                },
            )
            .await?;
            let ControlResponse::CatalogPolicyStatus {
                snapshot: Some(snapshot),
                job: Some(job),
                ..
            } = response
            else {
                anyhow::bail!(
                    "failed policy status was unavailable: {}",
                    serde_json::to_string(&response)?
                );
            };
            if matches!(
                job.state,
                airk_core::service::JobState::Failed
                    | airk_core::service::JobState::Canceled
                    | airk_core::service::JobState::Succeeded
            ) {
                assert_eq!(
                    job.state,
                    airk_core::service::JobState::Failed,
                    "quota fixture did not fail native admission: {job:?}"
                );
                assert!(snapshot.application.is_none());
                break snapshot;
            }
            ensure!(
                Instant::now() < deadline,
                "quota-failed trim did not settle: {job:?}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let directory = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        assert!(
            airk_store::current::policy_capsule::PolicyCapsule::load(&directory)?.is_none(),
            "fixture must fail before capsule write"
        );
        if resume {
            // Restore the deliberately injected quota condition through its owning
            // Store adapter, then exercise the actual saved native resume protocol.
            quota.configure(original)?;
            let method = ControlMethod::CatalogPolicyResume {
                index_id: f.indexes[2],
                operation_id: failed.operation_id,
                caller: draft.request.caller,
                key: policy_key(&f, 94).await?,
            };
            let first = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
            let ControlResponse::CatalogPolicyResumed {
                accepted_job_id,
                snapshot,
                ..
            } = first
            else {
                anyhow::bail!(
                    "native resume refused returned capacity: {}",
                    serde_json::to_string(&first)?
                );
            };
            assert_ne!(accepted_job_id, failed.job);
            assert_eq!(snapshot.policy, failed.policy);
            let repeated = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
            let ControlResponse::CatalogPolicyResumed {
                accepted_job_id: replayed,
                ..
            } = repeated
            else {
                anyhow::bail!(
                    "saved resume observation failed: {}",
                    serde_json::to_string(&repeated)?
                );
            };
            assert_eq!(replayed, accepted_job_id);
            let applied = wait_policy(&f).await?;
            assert_eq!(applied.operation_id, failed.operation_id);
            assert_eq!(applied.policy, failed.policy);
            assert_eq!(applied.job, accepted_job_id);
            assert_eq!(
                ServiceRegistry::open_saved(&f.control)?
                    .job(failed.job)?
                    .state,
                airk_core::service::JobState::Failed
            );
            renew_policy_grant(&mut f, 95).await?;
            let backend =
                airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
            let allowed=native_read(backend.as_ref(),"airk_read_file",&json!({"path":"api/policy.md","start_line":1,"end_line":2,"max_bytes":8192,"verification":"off"})).await?;
            assert!(
                serde_json::to_string(&allowed)?.contains("Original isolated source."),
                "resumed policy did not deliver allowed native evidence: {allowed}"
            );
            forbid_trimmed_bytes(&allowed)?;
            backend.close();
            backend.shutdown().await?;
            let ControlMethod::CatalogPolicyResume {
                index_id,
                operation_id,
                caller,
                key,
            } = method
            else {
                unreachable!()
            };
            let receipt_method = ControlMethod::CatalogPolicyResumeReceipt {
                index_id,
                operation_id,
                caller,
                key,
            };
            let exact =
                airk_cli::service_runtime::request(&f.control, receipt_method.clone()).await?;
            let ControlResponse::CatalogPolicyResumeReceipt {
                receipt: Some(receipt),
                expired: false,
                ..
            } = &exact
            else {
                anyhow::bail!(
                    "missing exact resume evidence: {}",
                    serde_json::to_string(&exact)?
                );
            };
            assert_eq!(receipt.job.id, accepted_job_id);
            assert_eq!(
                receipt.configuration.request.configuration,
                match &draft.request.operation {
                    CatalogOperation::ChangeIndexPolicy { configuration, .. } =>
                        configuration.configuration.clone(),
                    _ => unreachable!(),
                }
            );
            assert!(receipt.result.is_some());
            assert_eq!(
                policy_rpc_cli(&f, receipt_method.clone()).await?,
                serde_json::to_value(&exact)?
            );
            let unblock = policy_draft(
                &f,
                97,
                SourceMutationSelection::UnblockPath {
                    rule_id: applied.policy.denies[0].rule_id,
                },
            )
            .await?;
            apply_policy(&f, &unblock).await?;
            let unblocked = wait_policy(&f).await?;
            assert_ne!(unblocked.operation_id, operation_id);
            let historical =
                airk_cli::service_runtime::request(&f.control, receipt_method.clone()).await?;
            assert_eq!(
                serde_json::to_value(&historical)?,
                serde_json::to_value(&exact)?,
                "superseding policy cannot change accepted retry evidence"
            );
            assert_eq!(
                policy_rpc_cli(&f, receipt_method).await?,
                serde_json::to_value(&exact)?
            );
            retire_policy_index(&f, 96, &unblocked).await?;
        } else {
            retire_policy_index(&f, 92, &failed).await?;
            let retained = airk_store::current::policy_capsule::PolicyCapsule::load(&directory)?
                .context("failed trim lost its enforcement copy during retirement")?;
            assert!(
                retained.application.is_none(),
                "retirement fabricated a native removal publication"
            );
        }
        f.assert_sources_unchanged()?;
    }
    Ok(())
}

#[cfg(feature = "embedding-worker")]
#[tokio::test]
async fn permanent_trim_keeps_allowed_occurrences_of_a_shared_native_vector() -> Result<()> {
    use airk_core::{readiness::DefaultSearchPolicy, service::JobKind};
    let mut f = Fixture::with_helper()?;
    f.session.project_ids = vec![f.projects[2]];
    f.session.index_ids = Some(vec![f.indexes[2]]);
    let registration = f.registrations[2].clone();
    let shared = "# sharedtrimneedle\nOriginal identical allowed and blocked vector fixture.\n";
    for path in ["api/shared.md", "private/shared.md"] {
        fs::write(Path::new(&registration.root).join(path), shared)?;
    }
    let kernel = airk_kernel::Kernel::open_service_collection(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?;
    kernel.configure_vectors(true)?;
    kernel.set_default_semantic_search_policy(DefaultSearchPolicy::PartialHybrid)?;
    drop(kernel);
    let model = f.owner.path().join("shared-vector-model");
    fs::create_dir(&model)?;
    fs::write(
        model.join("vocabulary.json"),
        br#"{"sharedtrimneedle":[1.0,0.0],"boundaryneedle":[0.0,1.0]}"#,
    )?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&json!({
            "schema":1,"name":"original-p9-shared-vector-fixture","license":"Apache-2.0","dimensions":2,
            "sha256":airk_cli::runtime_pair::digest_file(&model.join("vocabulary.json"))?
        }))?,
    )?;
    let _service = f.start().await?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::ConfigureWatch {
            project: registration.binding,
            enabled: false,
        },
    )
    .await?;
    ensure!(
        matches!(response, ControlResponse::Watch { .. }),
        "watcher disable refused: {}",
        serde_json::to_string(&response)?
    );
    fixture_pause(&f, 2, false).await?;
    fixture_install_model(&f, 2, &model).await?;
    fixture_job(&f, 2, JobKind::Index).await?;
    fixture_job(&f, 2, JobKind::Vectors).await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let query = json!({"query":"sharedtrimneedle","mode":"vector","project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"limit":16,"max_bytes":16384,"verification":"off","response_mode":"full","member_availability":"require"});
    let before = native_read(backend.as_ref(), "airk_search", &query).await?;
    ensure!(
        before["isError"] != true,
        "native shared vector setup failed: {before}"
    );
    let before = body(&before)?;
    let member = member_body(&before, &before["members"][0])?;
    let paths = |member: &Value| -> Result<Vec<String>> {
        Ok(member["hits"]
            .as_array()
            .context("vector hits absent")?
            .iter()
            .filter_map(|hit| hit["citation"]["path"].as_str().map(str::to_owned))
            .collect())
    };
    let before_paths = paths(&member)?;
    assert!(
        before_paths.iter().any(|path| path == "api/shared.md"),
        "allowed shared occurrence absent: {member}"
    );
    assert!(
        before_paths.iter().any(|path| path == "private/shared.md"),
        "blocked shared occurrence absent before trim: {member}"
    );
    let draft = policy_draft(&f, 91, trim_selector()).await?;
    apply_policy(&f, &draft).await?;
    wait_policy(&f).await?;
    backend.close();
    backend.shutdown().await?;
    renew_policy_grant(&mut f, 92).await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let after = native_read(backend.as_ref(), "airk_search", &query).await?;
    ensure!(
        after["isError"] != true,
        "trim lost shared native vector: {after}"
    );
    let logical = body(&after)?;
    let member = member_body(&logical, &logical["members"][0])?;
    let after_paths = paths(&member)?;
    assert!(
        after_paths.iter().any(|path| path == "api/shared.md"),
        "shared vector's allowed occurrence was removed: {member}"
    );
    assert!(
        !after_paths.iter().any(|path| path.starts_with("private/")),
        "blocked vector occurrence escaped: {member}"
    );
    forbid_trimmed_bytes(&after)?;
    policy_parity(
        "trim shared vector CLI",
        &after,
        &f.cli("airk_search", &query).await?,
    )?;
    let mut mcp = McpSession::start(&f)?;
    policy_parity(
        "trim shared vector MCP",
        &after,
        &mcp.call(20, "airk_search", query)?["result"],
    )?;
    backend.close();
    backend.shutdown().await?;
    drop(mcp);
    for path in ["api/shared.md", "private/shared.md"] {
        assert_eq!(
            fs::read_to_string(Path::new(&registration.root).join(path))?,
            shared
        );
    }
    f.assert_sources_unchanged()?;
    Ok(())
}
