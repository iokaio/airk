// SPDX-License-Identifier: Apache-2.0
//! Private actual-composition reconciliation. Original bytes, never check verdicts.
//! Service test-support consumer of the genuine Inference observer owner.
use super::*;
use airk_core::arbiter::resource_policy::ResourcePolicy;
use airk_core::{SearchResponse, service::ServiceHealth, service_resources::ResourceSnapshot};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct ComposedLimits {
    deadline_ms: u64,
    shared_input_tokens: usize,
    context_input_tokens: usize,
    current: bool,
}
impl ComposedLimits {
    fn from_policy(policy: Option<&ResourcePolicy>) -> Result<Self> {
        if let Some(policy) = policy {
            policy.validate()?;
            Ok(Self {
                deadline_ms: policy.deadline_ms,
                shared_input_tokens: policy.shared_input_tokens,
                context_input_tokens: policy.context_input_tokens,
                current: true,
            })
        } else {
            Ok(Self {
                deadline_ms: 30_000,
                shared_input_tokens: 8192,
                context_input_tokens: 2048,
                current: false,
            })
        }
    }
    fn read_deadline(self, actual: u64) -> Result<()> {
        ensure!(
            (1..=self.deadline_ms).contains(&actual),
            "original native read deadline exceeds approved composed policy"
        );
        Ok(())
    }
    fn profile(self, profile: &Profile) -> Result<()> {
        let expected = if profile.declared_stage() == Some(Stage::Focus) {
            512
        } else {
            self.context_input_tokens
        };
        ensure!(
            profile.max_tokens == expected,
            "composed profile width differs from original approved policy"
        );
        Ok(())
    }
    fn development_deadline(self, actual: &Value) -> Result<()> {
        bounded_number(actual, self.deadline_ms as f64 / 1000.0)
    }
    fn budget(self, budget: &Budget, submitted: Option<&Budget>) -> Result<()> {
        budget.validate()?;
        ensure!(
            budget.input_tokens <= self.shared_input_tokens,
            "original composed budget exceeds approved shared tokens"
        );
        self.read_deadline(budget.deadline_ms)?;
        if self.current {
            ensure!(
                budget.input_tokens == self.shared_input_tokens
                    && budget.deadline_ms == self.deadline_ms
                    && submitted == Some(budget),
                "current composed budget differs from actual original8192/120 Answer arguments"
            );
        }
        Ok(())
    }
}

const CASES: [&str; 10] = [
    "baseline_identity",
    "focus_only_search",
    "dual_compact",
    "dual_large",
    "noninterference",
    "cancel_drain",
    "source_revoke",
    "remove_recover",
    "stage_independence",
    "normal_shutdown",
];
fn object<'a>(value: &'a Value, field: &str) -> Result<&'a serde_json::Map<String, Value>> {
    value[field]
        .as_object()
        .with_context(|| format!("original {field} object absent"))
}
fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("original {field} array absent"))
}
fn integer(value: &Value, field: &str) -> Result<u64> {
    value[field]
        .as_u64()
        .with_context(|| format!("original {field} integer absent"))
}
fn artifact(reference: &Value) -> Result<Vec<u8>> {
    let path = PathBuf::from(
        reference["path"]
            .as_str()
            .context("original artifact path absent")?,
    );
    ensure!(
        path.is_absolute(),
        "original artifact path must be absolute"
    );
    let digest: Sha256Digest = serde_json::from_value(reference["sha256"].clone())?;
    let bytes = fs::symlink_metadata(&path)?.len();
    pinned(&PinnedFile {
        path,
        bytes,
        sha256: digest,
    })
}
fn json_artifact(reference: &Value) -> Result<Value> {
    Ok(serde_json::from_slice(&artifact(reference)?)?)
}
fn document(reference: &Value) -> Result<Value> {
    let value = json_artifact(reference)?;
    if value["schema"] == "airk.private.composed-observation/1"
        && value["request"]["kind"] == "os_memory"
    {
        return Ok(value["result"].clone());
    }
    if value["schema"] == "airk.private.composed-observation/1"
        && value["request"]["kind"] == "native_command"
    {
        ensure!(
            value["result"]["returncode"] == 0,
            "original native command failed"
        );
        let path = PathBuf::from(
            reference["path"]
                .as_str()
                .context("original observation path absent")?,
        );
        let stdout = path
            .parent()
            .context("original observation parent absent")?
            .join(format!("{:03}-stdout.bin", integer(&value, "step")?));
        let raw = pinned(&PinnedFile {
            bytes: fs::symlink_metadata(&stdout)?.len(),
            path: stdout,
            sha256: serde_json::from_value(value["result"]["stdout_sha256"].clone())?,
        })?;
        Ok(serde_json::from_slice(&raw)?)
    } else {
        Ok(value)
    }
}
fn references(value: &Value) -> Result<Vec<Value>> {
    value
        .as_array()
        .context("original artifact references absent")?
        .iter()
        .map(document)
        .collect()
}
fn typed_status(value: &Value) -> Result<Status> {
    let status: Status = serde_json::from_value(value.clone())?;
    status.validate()?;
    ensure!(
        status.initialization.is_none(),
        "initialization placeholder is not active consent"
    );
    Ok(status)
}
fn resource(value: &Value) -> Result<ResourceSnapshot> {
    let snapshot: ResourceSnapshot = serde_json::from_value(value.clone())?;
    snapshot.configuration.validate()?;
    ensure!(
        snapshot.telemetry_complete
            && snapshot.observed_rss_bytes.is_some()
            && snapshot.available_memory_bytes.is_some()
            && !snapshot.provider_drain_error
            && snapshot.memory_reserved_bytes <= snapshot.memory_limit_bytes,
        "actual native resource telemetry incomplete or beyond admission"
    );
    Ok(snapshot)
}
fn exchange_value(exchange: &Value, health: &ServiceHealth) -> Result<Value> {
    ensure!(
        exchange["kind"] == "invoked"
            && exchange["launch_count"] == 1
            && exchange["result"]["exitCode"] == 0
            && exchange["hello"]["serviceInstance"] == serde_json::to_value(health.instance)?
            && exchange["identity"]["service_instance"] == serde_json::to_value(health.instance)?
            && exchange["identity"]["catalog_namespace"]
                == serde_json::to_value(
                    health
                        .catalog_clock
                        .context("native catalog clock absent")?
                        .namespace
                )?,
        "original I2 exchange instance/exit/launch identity differs"
    );
    verify_named_seals(object(&exchange["identity"], "input_seals")?)?;
    let argv = array(exchange, "argv")?;
    let positions = argv
        .iter()
        .enumerate()
        .filter(|(_, item)| *item == "--args-file")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    ensure!(
        positions.len() == 1,
        "original argument file absent/ambiguous"
    );
    let path = PathBuf::from(
        argv.get(positions[0] + 1)
            .and_then(Value::as_str)
            .context("argument path absent")?,
    );
    let argument_sha: Sha256Digest = serde_json::from_value(exchange["arguments_sha256"].clone())?;
    let raw = pinned(&PinnedFile {
        bytes: fs::symlink_metadata(&path)?.len(),
        path,
        sha256: argument_sha,
    })?;
    ensure!(
        raw.len() <= 65536,
        "original read arguments exceed native input bound"
    );
    let stdout = exchange["result"]["stdout"]
        .as_str()
        .context("original I2 stdout absent")?;
    let envelope: Value = serde_json::from_str(stdout)?;
    ensure!(
        envelope["isError"] != true,
        "original canonical call refused"
    );
    let contents = array(&envelope, "content")?;
    ensure!(
        contents.len() == 1 && contents[0]["type"] == "text",
        "canonical envelope shape differs"
    );
    let value: Value = serde_json::from_str(
        contents[0]["text"]
            .as_str()
            .context("canonical text absent")?,
    )?;
    let identity = &exchange["identity"];
    let sealed = object(identity, "input_seals")?;
    let binding_paths = sealed
        .iter()
        .filter(|(_, digest)| **digest == identity["bindings_sha256"])
        .collect::<Vec<_>>();
    ensure!(
        binding_paths.len() == 1,
        "original sealed native bindings ambiguous/absent"
    );
    let source = PathBuf::from(binding_paths[0].0.as_str());
    let raw = pinned(&PinnedFile {
        bytes: fs::symlink_metadata(&source)?.len(),
        path: source,
        sha256: serde_json::from_value((*binding_paths[0].1).clone())?,
    })?;
    let bindings: Value = serde_json::from_slice(&raw)?;
    ensure!(
        bindings["schema"] == "airk.private-arbiter-index-bindings/1"
            && bindings["instance"] == serde_json::to_value(health.instance)?
            && bindings["namespace"] == identity["catalog_namespace"],
        "original native bindings/service namespace differs"
    );
    let index = serde_json::from_value(identity["index_id"].clone())?;
    bound_native_scope(&value, &bindings, index)?;
    Ok(value)
}
fn bound_native_scope(
    value: &Value,
    bindings: &Value,
    index: airk_core::index_catalog::IndexId,
) -> Result<()> {
    let observation: airk_core::scoped_reads::ScopeObservation =
        serde_json::from_value(value["scope_observation"].clone())?;
    observation.validate()?;
    ensure!(
        observation.members.len() == 1 && observation.members[0].index_id == index,
        "canonical native read widened/changed original selected index"
    );
    let rows = array(bindings, "bindings")?
        .iter()
        .filter(|row| row["index_id"] == serde_json::json!(index))
        .collect::<Vec<_>>();
    ensure!(
        rows.len() == 1,
        "original immutable source binding missing/ambiguous"
    );
    let row = rows[0];
    let definition: airk_core::index_catalog::IndexDefinition =
        serde_json::from_value(row["definition"].clone())?;
    ensure!(
        definition.id == index
            && !definition.tombstone
            && observation.members[0].source_revision == definition.source_revision,
        "original native index/source revision differs"
    );
    let roots = array(row, "source_roots")?
        .iter()
        .map(|root| serde_json::from_value(root["id"].clone()))
        .collect::<std::result::Result<Vec<airk_core::index_catalog::SourceRootId>, _>>()?;
    ensure!(
        observation.members[0].source_root_ids == roots,
        "original native source roots changed"
    );
    if value["schema"] == airk_core::arbiter::context_action::CONTEXT_SCHEMA_V2 {
        let packet = airk_core::arbiter::context_action::ContextResponseV2::from_canonical(
            value,
            &Budget::default(),
        )?;
        ensure!(
            packet.applications.len() == 1
                && serde_json::to_value(&packet.applications[0].scope.publication)?
                    == row["publication"],
            "original composed publication differs"
        );
        for record in packet.records {
            bound_native_scope(&record.canonical_result, bindings, index)?;
        }
    } else {
        let publication = value
            .get("publication")
            .or_else(|| value.get("read").and_then(|read| read.get("publication")))
            .context("original canonical publication absent")?;
        ensure!(
            *publication == row["publication"],
            "original read publication differs"
        );
    }
    Ok(())
}
fn canonical_call(value: &Value, health: &ServiceHealth, limits: ComposedLimits) -> Result<Value> {
    if value["schema"] == "airk.development-native-goal-call/1" {
        ensure!(
            value["returncode"] == 0
                && value["validated"] == true
                && value["identity_error"].is_null()
                && value["parse_error"].is_null()
                && value["timed_out"] == false,
            "original canonical call failed/incomplete"
        );
        limits.development_deadline(&value["deadline_seconds"])?;
        let exchange = &value["session_exchange"];
        ensure!(
            exchange["arguments_sha256"] == value["arguments_sha256"],
            "original argument digest differs"
        );
        let canonical = exchange_value(exchange, health)?;
        let stdout = exchange["result"]["stdout"]
            .as_str()
            .context("original stdout absent")?;
        ensure!(
            sha(stdout.as_bytes()) == serde_json::from_value(value["stdout_sha256"].clone())?
                && serde_json::from_str::<Value>(stdout)? == value["envelope"]
                && canonical == value["full_canonical_value"],
            "full canonical value/envelope was changed"
        );
        let actual: ServiceHealth = serde_json::from_value(
            value["actual_service_identity"]["actual_native_health"].clone(),
        )?;
        ensure!(
            actual.instance == health.instance
                && actual.process_id == health.process_id
                && actual.executable_sha256 == health.executable_sha256
                && actual.catalog_clock == health.catalog_clock,
            "actual service native identity differs"
        );
        Ok(canonical)
    } else {
        ensure!(
            value["result"]["ok"] == true && value["result"]["value"]["outcome"] == "returned",
            "actual composed controller call did not return"
        );
        let invocation = &value["result"]["value"];
        ensure!(
            integer(invocation, "completed_unix_ms")? >= integer(invocation, "started_unix_ms")?,
            "original composed call timestamps regressed"
        );
        exchange_value(&invocation["value"], health)
    }
}

fn search_preservation(before: &Value, after: &Value, focus_profile: &Profile) -> Result<()> {
    let original: SearchResponse = serde_json::from_value(before.clone())?;
    let ranked: SearchResponse = serde_json::from_value(after.clone())?;
    ensure!(
        original.focus.is_none(),
        "baseline already carries Focus work"
    );
    let receipt = match ranked.focus.as_ref() {
        Some(FocusObservation::Receipt { receipt }) => receipt,
        _ => anyhow::bail!("actual applied Focus receipt absent"),
    };
    receipt.validate_search_response(&ranked, &Budget::default())?;
    ensure!(
        receipt.applied
            && receipt.scored_count > 0
            && receipt
                .identity
                .as_ref()
                .is_some_and(|identity| identity.profile == focus_profile.manifest
                    && identity.compiler == focus_profile.compiler
                    && identity.calibration == focus_profile.calibration
                    && identity.task_policy == focus_profile.task_policies[&Task::FocusRank]),
        "actual Focus receipt identity differs"
    );
    let mut left = before.clone();
    let mut right = after.clone();
    left.as_object_mut()
        .context("original Search object absent")?
        .remove("focus");
    right
        .as_object_mut()
        .context("ranked Search object absent")?
        .remove("focus");
    let left_hits = left
        .as_object_mut()
        .unwrap()
        .remove("hits")
        .context("original hits absent")?;
    let right_hits = right
        .as_object_mut()
        .unwrap()
        .remove("hits")
        .context("ranked hits absent")?;
    ensure!(left == right, "canonical nonranking Search fields differ");
    let original_hits = left_hits.as_array().context("original hit array absent")?;
    let expected = receipt.reorder(original_hits, &receipt.original_order, &Budget::default())?;
    ensure!(
        Value::Array(expected) == right_hits,
        "complete original hit bodies/order not preserved"
    );
    Ok(())
}
fn session_close(value: &Value, expected_poison: Option<&str>) -> Result<()> {
    ensure!(
        value["kind"] == "closed"
            && value["child_exit_observed"] == true
            && value["child_exit_code"] == 0
            && value["child_signal_code"].is_null()
            && value["launch_count"] == 1,
        "original native session did not close naturally"
    );
    match expected_poison {
        None => ensure!(
            value["poisoned"].is_null(),
            "unexpected native session poison"
        ),
        Some(reason) => ensure!(
            value["poisoned"] == reason,
            "canceled session poison differs from original refusal"
        ),
    }
    Ok(())
}
fn cancellation_poison(observations: &Value) -> Result<String> {
    let original = json_artifact(&observations["original_call"])?;
    let outcome = &original["result"]["value"];
    ensure!(
        outcome["outcome"] == "refused",
        "original AbortSignal did not yield a refused native invocation"
    );
    if outcome["error"]["code"].is_null() {
        Ok("invoke_failed_no_retry".into())
    } else {
        Ok(outcome["error"]["code"]
            .as_str()
            .context("original abort error code is not a string")?
            .into())
    }
}

fn natural_composed_owner(value: &Value) -> Result<()> {
    ensure!(
        value["complete"] == true
            && value["failure"].is_null()
            && value["post_input_drift"] == serde_json::json!([])
            && value["whole_phase_deadline_not_refreshed"] == true
            && value["whole_phase_expired"] == false
            && value["no_automatic_retry"] == true
            && value["other_processes_touched"] == false,
        "composed owner incomplete/drifted/renewed"
    );
    let owned = &value["owned"];
    ensure!(
        owned["exit_code"] == 0
            && owned["timed_out"] == false
            && owned["log_oversized"] == false
            && owned["owned_descendants_settled"] == true
            && owned["owned_job_forced_termination"] == false
            && owned["owned_job_final_active_processes"] == 0
            && owned["owned_job_final_member_pids"] == serde_json::json!([])
            && owned["child_assigned_before_resume"] == true
            && owned["parent_assigned_to_job"] == false
            && owned["whole_process_deadline_not_refreshed"] == true,
        "composed native job did not close naturally"
    );
    let exits = array(owned, "retained_member_exits")?;
    ensure!(
        !exits.is_empty()
            && exits.len() as u64 == integer(owned, "owned_total_processes")?
            && exits
                .iter()
                .all(|exit| exit["exit_observed"] == true && exit["exit_code"] == 0),
        "actual descendant exit roster incomplete"
    );
    let close = &owned["handle_close_proof"];
    ensure!(
        close["forced"] == false
            && close["settled"] == true
            && close["final_active"] == 0
            && close["members_before"] == serde_json::json!([])
            && close["final_member_pids"] == serde_json::json!([]),
        "composed owner handle close lacks empty-job proof"
    );
    Ok(())
}

use airk_inference::inference_engine::pool::arbiter::execution_observer::{
    Kind, Outcome, Record, RecorderSummary,
};
fn journal(reference: &Value) -> Result<Vec<Record>> {
    let raw = artifact(reference)?;
    ensure!(
        raw.ends_with(b"\n"),
        "original observer journal has a partial record"
    );
    let mut events = Vec::new();
    for line in raw
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let event: Record = serde_json::from_slice(line)?;
        event.validate()?;
        ensure!(
            event.schema == "airk.private.native-dispatch/1"
                && event.sequence == events.len() as u64 + 1
                && event.binding.dispatch_identity > 0
                && event.binding.tokens > 0
                && event.binding.tokens <= 8192
                && !event.binding.charge_identity.is_empty()
                && event.binding.charge_bytes > 0
                && event.binding.original_deadline_unix_ms > 0,
            "invalid/missing original native observer identity"
        );
        events.push(event);
    }
    ensure!(
        !events.is_empty() && events.len() <= 8192,
        "original observer journal empty/excessive"
    );
    Ok(events)
}
fn same_binding(left: &Record, right: &Record) -> Result<()> {
    ensure!(
        serde_json::to_value(&left.binding)? == serde_json::to_value(&right.binding)?,
        "native original charge/input/deadline/scope binding changed"
    );
    Ok(())
}
fn returned<'a>(events: &'a [Record], dispatch: &Record) -> Result<&'a Record> {
    let matches = events
        .iter()
        .filter(|event| {
            event.kind == Kind::ScoreReturned
                && event.binding.observer_id == dispatch.binding.observer_id
                && event.binding.dispatch_identity == dispatch.binding.dispatch_identity
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "one actual same-input native completion required"
    );
    let result = matches[0];
    same_binding(dispatch, result)?;
    ensure!(
        result.sequence > dispatch.sequence
            && result.outcome == Some(Outcome::Success)
            && result.finite_raw_scores == Some(true)
            && result.error.is_none(),
        "native actual score did not succeed"
    );
    let scores = result
        .raw_scores
        .as_ref()
        .context("actual raw scores absent")?;
    let bits = result
        .raw_score_bits
        .as_ref()
        .context("actual raw score bits absent")?;
    ensure!(
        !scores.is_empty() && scores.len() == bits.len() && scores.len() <= 64,
        "actual complete native raw vector absent"
    );
    for (score, bits) in scores.iter().zip(bits) {
        ensure!(
            score.is_some_and(|score| score.is_finite() && score.to_bits() == *bits),
            "nonfinite or changed actual raw score bits"
        );
    }
    let work = result
        .work
        .as_ref()
        .context("actual returned work absent")?;
    ensure!(
        work.input_digest == dispatch.binding.input_digest
            && work.input_tokens == dispatch.binding.tokens,
        "actual native work differs from original compiled input"
    );
    Ok(result)
}
fn settled_released(events: &[Record], dispatch: &Record) -> Result<()> {
    let result = returned(events, dispatch)?;
    let event = |kind| -> Result<&Record> {
        let rows = events
            .iter()
            .filter(|event| {
                event.kind == kind
                    && event.binding.observer_id == dispatch.binding.observer_id
                    && event.binding.dispatch_identity == dispatch.binding.dispatch_identity
            })
            .collect::<Vec<_>>();
        ensure!(
            rows.len() == 1,
            "actual native settlement/release absent or duplicate"
        );
        Ok(rows[0])
    };
    let settled = event(Kind::JobSettled)?;
    let released = event(Kind::ChargeReleased)?;
    let profile = event(Kind::ProfileReleased)?;
    same_binding(dispatch, settled)?;
    same_binding(dispatch, released)?;
    same_binding(dispatch, profile)?;
    ensure!(
        result.sequence < settled.sequence
            && settled.sequence < profile.sequence
            && profile.sequence < released.sequence
            && settled.return_observed
            && settled.proof.as_deref()
                == Some("actual_job_compute_delivery_and_per_job_resident_permit_dropped")
            && released.proof.as_deref()
                == Some("actual_final_reservation_drop_after_native_scorer_and_source_teardown"),
        "native charge was not actually released after compute/delivery settlement"
    );
    Ok(())
}
fn trigger<'a>(observations: &Value, events: &'a [Record]) -> Result<&'a Record> {
    let original = json_artifact(&observations["trigger"])?;
    ensure!(
        original["result"]["ok"] == true,
        "original observer watch failed"
    );
    let event: Record = serde_json::from_value(original["result"]["value"]["event"].clone())?;
    ensure!(
        event.kind == Kind::DispatchAdmitted,
        "race trigger is not actual admitted dispatch"
    );
    let rows = events
        .iter()
        .filter(|actual| actual.sequence == event.sequence)
        .collect::<Vec<_>>();
    ensure!(
        rows.len() == 1 && serde_json::to_value(rows[0])? == serde_json::to_value(event)?,
        "race trigger differs from final original observer journal"
    );
    Ok(rows[0])
}
fn mutation_interval(observation: &Value, dispatch: &Record, returned: &Record) -> Result<()> {
    let start = integer(observation, "started_unix_ms")?;
    let finish = integer(observation, "completed_unix_ms")?;
    ensure!(
        dispatch.unix_ms <= start && start <= finish && finish < returned.unix_ms,
        "actual mutation/concurrent operation did not settle inside original score interval"
    );
    Ok(())
}
fn fixture_profiles(
    receipt: &Value,
    profile: &Profile,
    expected: &Binding,
) -> Result<BTreeMap<ModelChoice, Profile>> {
    let mut profiles = BTreeMap::new();
    for row in array(receipt, "profiles")? {
        let raw = artifact(&row["raw_profile"])?;
        let native: Profile = serde_json::from_slice(&raw)?;
        native.validate()?;
        ensure!(
            native.model.stage().is_some()
                && row["model"] == serde_json::to_value(native.model)?
                && row["manifest"] == serde_json::to_value(native.manifest)?
                && row["target"] == expected.target,
            "composed original model/manifest/target differs"
        );
        if native.model == profile.model {
            ensure!(
                sha(&raw) == expected.candidate_profile_sha256
                    && serde_json::to_value(&native)? == serde_json::to_value(profile)?,
                "composed evidence is not the final corrected candidate"
            );
        }
        ensure!(
            profiles.insert(native.model, native).is_none(),
            "duplicate composed model"
        );
    }
    ensure!(
        profiles.keys().copied().collect::<BTreeSet<_>>()
            == BTreeSet::from([
                ModelChoice::FocusQwenReranker06B,
                ModelChoice::ContextQwenCompact,
                ModelChoice::ContextQwenLarge
            ]),
        "actual composed three-model closure incomplete"
    );
    Ok(profiles)
}
fn verify_health(
    receipt: &Value,
    profiles: &BTreeMap<ModelChoice, Profile>,
) -> Result<ServiceHealth> {
    let ready = json_artifact(&receipt["fixture_binding"]["ready"])?;
    ensure!(
        ready["schema"] == "airk.private-arbiter-fixture/1"
            && ready["evaluation"] == true
            && ready["release_qualified"] == false,
        "actual native Ready absent"
    );
    let health: ServiceHealth = serde_json::from_value(ready["health"].clone())?;
    let actual: ServiceHealth =
        serde_json::from_value(document(&receipt["fixture_binding"]["health"])?)?;
    ensure!(
        actual.instance == health.instance
            && actual.process_id == health.process_id
            && actual.executable_sha256 == health.executable_sha256
            && actual.catalog_clock == health.catalog_clock
            && receipt["fixture_binding"]["pid"] == health.process_id
            && receipt["fixture_binding"]["image_sha256"] == health.executable_sha256
            && receipt["fixture_binding"]["instance"] == serde_json::to_value(health.instance)?
            && receipt["fixture_binding"]["actual_process_observation"]["process_id"]
                == health.process_id
            && receipt["fixture_binding"]["actual_process_observation"]["executable_sha256"]
                == health.executable_sha256
            && receipt["fixture_binding"]["actual_process_observation"]["creation_filetime_100ns"]
                .as_u64()
                .is_some_and(|creation| creation > 0),
        "original native service/creation identity differs"
    );
    let roster = array(&ready, "profiles")?;
    ensure!(roster.len() == 3, "actual Ready model roster incomplete");
    for (model, profile) in profiles {
        let rows = roster
            .iter()
            .filter(|row| row["model"] == serde_json::json!(model))
            .collect::<Vec<_>>();
        ensure!(
            rows.len() == 1 && rows[0]["manifest"] == serde_json::json!(profile.manifest),
            "actual Ready differs from supplied native profile"
        );
        let declared = array(receipt, "profiles")?
            .iter()
            .find(|row| row["model"] == serde_json::json!(model))
            .context("profile missing")?;
        ensure!(
            rows[0]["profile_sha256"] == declared["raw_profile"]["sha256"],
            "Ready raw profile bytes differ"
        );
    }
    Ok(health)
}

struct OriginalCall {
    canonical: Value,
    started: u64,
    completed: u64,
    deadline: u64,
    question_sha: Sha256Digest,
    submitted_budget: Option<Budget>,
}
fn original_call(
    observations: &Value,
    plan: &Value,
    health: &ServiceHealth,
    limits: ComposedLimits,
) -> Result<OriginalCall> {
    let start = json_artifact(&observations["call_start"])?;
    let completion = json_artifact(&observations["call_completion"])?;
    let step = usize::try_from(integer(&start, "step")?)?;
    let request = &array(plan, "steps")?
        .get(step)
        .context("original start step absent")?["request"];
    ensure!(
        start["schema"] == "airk.private.composed-observation/1"
            && start["request"] == array(plan, "steps")?[step],
        "original controller request/plan differs"
    );
    let submitted = &start["result"]["value"];
    let completion_step = usize::try_from(integer(&completion, "step")?)?;
    ensure!(
        completion["schema"] == "airk.private.composed-observation/1"
            && array(plan, "steps")?.get(completion_step) == Some(&completion["request"])
            && completion["request"]["request"]["kind"] == "await"
            && completion["request"]["request"]["id"] == request["id"],
        "original call completion belongs to another request/plan"
    );
    let result = &completion["result"]["value"];
    ensure!(
        request["kind"] == "start"
            && start["result"]["ok"] == true
            && submitted["submitted"] == true
            && submitted["id"] == request["id"]
            && result["value"]["id"] == request["id"],
        "original one-use read identity differs"
    );
    let deadline = integer(request, "deadline_ms")?;
    limits.read_deadline(deadline)?;
    let started = integer(result, "started_unix_ms")?;
    let completed = integer(result, "completed_unix_ms")?;
    ensure!(
        submitted["started_unix_ms"] == started
            && completed >= started
            && completed.saturating_sub(started) <= deadline,
        "original read exceeded/renewed deadline"
    );
    let args = PathBuf::from(
        request["args_file"]
            .as_str()
            .context("original request arguments absent")?,
    );
    let bytes = pinned(&PinnedFile {
        bytes: fs::symlink_metadata(&args)?.len(),
        path: args,
        sha256: serde_json::from_value(result["value"]["arguments_sha256"].clone())?,
    })?;
    let arguments: Value = serde_json::from_slice(&bytes)?;
    let canonical = canonical_call(&completion, health, limits)?;
    let submitted_budget = if limits.current && request["tool"] == "answer" {
        let arguments: airk_mcp::AnswerArgs = serde_json::from_value(arguments.clone())?;
        arguments.request.validate()?;
        limits.budget(&arguments.request.budget, Some(&arguments.request.budget))?;
        ensure!(
            deadline == arguments.request.budget.deadline_ms,
            "current composed controller deadline differs from actual Answer budget"
        );
        Some(arguments.request.budget)
    } else {
        None
    };
    let question = if request["tool"] == "answer" {
        let question = arguments["question"]
            .as_str()
            .context("original answer question absent")?;
        ensure!(
            canonical["question"] == question,
            "actual answer question differs"
        );
        question
    } else {
        arguments["query"]
            .as_str()
            .context("original search query absent")?
    };
    Ok(OriginalCall {
        canonical,
        started,
        completed,
        deadline,
        question_sha: sha(question.as_bytes()),
        submitted_budget,
    })
}
fn dispatches<'a>(events: &'a [Record], call: &OriginalCall) -> Result<Vec<&'a Record>> {
    let rows = events
        .iter()
        .filter(|event| {
            event.kind == Kind::DispatchAdmitted
                && event.binding.question_utf8_sha256 == call.question_sha
                && event.unix_ms >= call.started
                && event.unix_ms <= call.completed
        })
        .collect::<Vec<_>>();
    let mut identities = BTreeSet::new();
    for event in &rows {
        ensure!(
            identities.insert(event.binding.dispatch_identity)
                && event.binding.original_deadline_unix_ms
                    <= call
                        .started
                        .checked_add(call.deadline)
                        .context("original deadline overflow")?
                && event.unix_ms <= event.binding.original_deadline_unix_ms,
            "duplicate/replayed/late optional dispatch"
        );
        returned(events, event)?;
    }
    Ok(rows)
}
fn composed_packet(
    observations: &Value,
    plan: &Value,
    health: &ServiceHealth,
    profiles: &BTreeMap<ModelChoice, Profile>,
    model: ModelChoice,
    limits: ComposedLimits,
) -> Result<()> {
    let call = original_call(observations, plan, health, limits)?;
    let budget: Budget = serde_json::from_value(json_artifact(&observations["original_budget"])?)?;
    limits.budget(&budget, call.submitted_budget.as_ref())?;
    let packet = airk_core::arbiter::context_action::ContextResponseV2::from_canonical(
        &call.canonical,
        &budget,
    )?;
    let status = typed_status(&document(&observations["status"])?)?;
    let focus = &profiles[&ModelChoice::FocusQwenReranker06B];
    let context = &profiles[&model];
    ensure!(
        status.focus.availability == Availability::Ready
            && status.context.availability == Availability::Ready
            && status.focus.active_profile == Some(focus.manifest)
            && status.context.active_profile == Some(context.manifest),
        "final fitted profiles were not actually warmed/active"
    );
    let events = journal(&observations["journal"])?;
    let dispatches = dispatches(&events, &call)?;
    let mut focus_tokens = 0usize;
    let mut context_tokens = 0usize;
    let mut charges = BTreeMap::new();
    for dispatch in dispatches {
        let profile = if dispatch.binding.profile_manifest == focus.manifest {
            focus
        } else if dispatch.binding.profile_manifest == context.manifest {
            context
        } else {
            anyhow::bail!("foreign stage/model dispatched during final composed call")
        };
        ensure!(
            dispatch.binding.charge_bytes == profile.memory_reservation_bytes,
            "actual fitted resident reservation differs"
        );
        ensure!(
            dispatch.binding.scope.configuration_revision == status.configuration.revision
                && status
                    .configuration
                    .eligible(dispatch.binding.scope.index_id, dispatch.binding.task),
            "actual dispatch lacked current original consent/scope"
        );
        let set = charges
            .entry(profile.manifest)
            .or_insert_with(BTreeSet::new);
        set.insert(dispatch.binding.charge_identity.clone());
        if profile.model == ModelChoice::FocusQwenReranker06B {
            focus_tokens = focus_tokens
                .checked_add(dispatch.binding.tokens)
                .context("Focus tokens overflow")?;
        } else {
            context_tokens = context_tokens
                .checked_add(dispatch.binding.tokens)
                .context("Context tokens overflow")?;
        }
    }
    ensure!(
        focus_tokens > 0
            && context_tokens > 0
            && focus_tokens <= budget.input_tokens / 2
            && focus_tokens.checked_add(context_tokens) == Some(packet.work.input_tokens)
            && packet.work.input_tokens <= budget.input_tokens
            && budget.input_tokens <= 8192,
        "actual composed costs absent, changed, or exceed original shared allocation"
    );
    ensure!(
        charges.len() == 2 && charges.values().all(|set| set.len() == 1),
        "actual two fitted resident charges absent"
    );
    let mut context_reserved = false;
    for event in &packet.assessment_history {
        use airk_core::arbiter::context_action::AssessmentResult;
        let advice = match &event.result {
            AssessmentResult::Selected { advice, .. } | AssessmentResult::Abstained { advice } => {
                Some(advice)
            }
            AssessmentResult::Unavailable { advice, .. } => advice.as_ref(),
        };
        if let Some(advice) = advice {
            ensure!(
                advice.profile == context.manifest
                    && advice.compiler == context.compiler
                    && advice.calibration == context.calibration,
                "composed fitted Context advice differs"
            );
            context_reserved |= advice.reserved_input_tokens >= 1024;
        }
    }
    ensure!(
        context_reserved,
        "original Context capacity was consumed by Focus"
    );
    let reservations = focus
        .memory_reservation_bytes
        .checked_add(context.memory_reservation_bytes)
        .context("resident sum overflow")?;
    let resources = references(&observations["resource_calls"])?;
    ensure!(
        resources.len() >= 3,
        "actual before/during/after native resource observations absent"
    );
    for observation in resources {
        let snapshot = resource(&observation)?;
        ensure!(
            snapshot.memory_reserved_bytes >= reservations,
            "fitted two-resident charge not retained in actual admission"
        );
    }
    Ok(())
}

fn os_sample(reference: &Value, receipt: &Value, limit: u64) -> Result<Value> {
    let sample = document(reference)?;
    let identity = &receipt["fixture_binding"]["actual_process_observation"];
    ensure!(
        sample["schema"] == "airk.private.composed-os-memory/1"
            && sample["actual_process_observation"] == *identity
            && sample["handle_creation_filetime_100ns"] == identity["creation_filetime_100ns"]
            && sample["handle_executable"]
                .as_str()
                .is_some_and(|path| identity["executable"]
                    .as_str()
                    .is_some_and(|expected| path.eq_ignore_ascii_case(expected))),
        "actual OS sample process/creation identity differs"
    );
    let working = integer(&sample, "working_set_bytes")?;
    let peak = integer(&sample, "peak_working_set_bytes")?;
    let commit = integer(&sample, "pagefile_usage_bytes")?;
    let peak_commit = integer(&sample, "peak_pagefile_usage_bytes")?;
    let private = integer(&sample, "private_usage_bytes")?;
    ensure!(
        working > 0
            && working <= peak
            && commit <= peak_commit
            && working <= limit
            && commit <= limit
            && peak <= limit
            && peak_commit <= limit
            && private <= limit,
        "actual composed process memory exceeds native capacity"
    );
    Ok(sample)
}
fn final_warm_load(
    observations: &Value,
    receipt: &Value,
    plan: &Value,
    health: &ServiceHealth,
    expected_profile: &Profile,
    limits: ComposedLimits,
) -> Result<()> {
    let warm = &observations["warm_load"];
    ensure!(
        serde_json::to_value(serde_json::from_value::<Profile>(json_artifact(
            &warm["profile"]
        )?)?)?
            == serde_json::to_value(expected_profile)?,
        "warm proof targets another profile"
    );
    let start = json_artifact(&warm["configure_start"])?;
    let completed = json_artifact(&warm["configure_completion"])?;
    ensure!(
        integer(&start, "started_unix_ms")? <= integer(&completed, "completed_unix_ms")?,
        "actual final fitted configure timestamps differ"
    );
    let original_configure = original_observation(&warm["configure_completion"], plan)?;
    let argv = array(&original_configure["request"], "argv")?;
    ensure!(
        argv.iter().any(|arg| arg == "configure"),
        "warm proof has no actual explicit Configure"
    );
    let configured = typed_status(&document(&warm["configure_completion"])?)?;
    let ready = typed_status(&document(&warm["status_ready"])?)?;
    let stage = expected_profile.declared_stage().context("stage absent")?;
    ensure!(
        configured.configuration.stage(stage).selected_profile == Some(expected_profile.manifest)
            && configured.configuration.stage(stage).enabled
            && ready.configuration.stage(stage) == configured.configuration.stage(stage)
            && match stage {
                Stage::Focus => ready.focus.active_profile,
                Stage::Context => ready.context.active_profile,
            } == Some(expected_profile.manifest)
            && match stage {
                Stage::Focus => ready.focus.availability,
                Stage::Context => ready.context.availability,
            } == Availability::Ready,
        "actual fitted profile did not reach native Ready after explicit configure"
    );
    let prior = typed_status(&document(&warm["prior_idle_status"])?)?;
    let prior_stage = match stage {
        Stage::Focus => &prior.focus,
        Stage::Context => &prior.context,
    };
    ensure!(
        prior_stage.active_profile.is_none()
            && prior_stage.running == 0
            && prior_stage.queued == 0
            && !matches!(
                prior_stage.availability,
                Availability::Failed | Availability::Draining
            ),
        "actual prior stage did not settle before final fitted load"
    );
    let raw_summary = json_artifact(&warm["status_observations"])?;
    let summary = if raw_summary["schema"] == "airk.private.composed-observation/1" {
        let original = original_observation(&warm["status_observations"], plan)?;
        ensure!(
            original["request"]["kind"] == "await_ready",
            "fitted warm evidence is not original readiness observation"
        );
        original["result"].clone()
    } else {
        raw_summary
    };
    ensure!(
        summary["schema"] == "airk.private.composed-warm-observation/1"
            && summary["profile_manifest"] == serde_json::json!(expected_profile.manifest)
            && summary["configuration_revision"] == configured.configuration.revision,
        "actual fitted warm observation identity differs"
    );
    bounded_number(&summary["original_warm_deadline_seconds"], 1800.0)?;
    let limit = summary["original_warm_deadline_seconds"]
        .as_f64()
        .context("original warm deadline absent")?;
    ensure!(limit > 0.0, "zero original warm deadline");
    let configure_step = integer(&completed, "step")?;
    ensure!(
        summary["configuration_observation_step"] == configure_step
            && summary["stage"] == serde_json::to_value(stage)?,
        "original warm configure/stage binding differs"
    );
    let waits = array(plan, "steps")?
        .iter()
        .filter(|step| {
            step["kind"] == "await_ready"
                && step["configuration_observation_step"] == configure_step
                && step["profile_manifest"] == serde_json::json!(expected_profile.manifest)
        })
        .collect::<Vec<_>>();
    ensure!(
        waits.len() == 1
            && waits[0]["deadline_seconds"] == summary["original_warm_deadline_seconds"],
        "fitted warm deadline was renewed or observed more than once"
    );
    let actual_statuses = array(&summary, "observations")?;
    ensure!(
        !actual_statuses.is_empty() && actual_statuses.len() <= 128,
        "actual warm observations absent/excessive"
    );
    let mut previous = integer(&start, "started_unix_ms")?;
    for (index, observation) in actual_statuses.iter().enumerate() {
        ensure!(
            observation["returncode"] == 0
                && observation["timed_out"] == false
                && integer(observation, "started_unix_ms")? >= previous,
            "failed/replayed warm status observation"
        );
        let completed = integer(observation, "completed_unix_ms")?;
        ensure!(
            completed >= integer(observation, "started_unix_ms")?
                && (completed.saturating_sub(integer(&start, "started_unix_ms")?) as f64)
                    <= limit * 1000.0,
            "fitted warm passed only after original deadline"
        );
        let observed = typed_status(&json_artifact(&observation["stdout"])?)?;
        artifact(&observation["stderr"])?;
        ensure!(
            observed.configuration.revision == configured.configuration.revision
                && observed.configuration.stage(stage) == configured.configuration.stage(stage),
            "fitted warm consent/revision changed"
        );
        let observed_stage = match stage {
            Stage::Focus => &observed.focus,
            Stage::Context => &observed.context,
        };
        if index + 1 == actual_statuses.len() {
            ensure!(
                observed_stage.availability == Availability::Ready
                    && observed_stage.active_profile == Some(expected_profile.manifest)
                    && serde_json::to_value(&observed)? == serde_json::to_value(&ready)?,
                "actual final fitted Ready observation absent/different"
            );
        } else {
            ensure!(
                matches!(
                    observed_stage.availability,
                    Availability::Requested | Availability::Verifying | Availability::Warming
                ),
                "failed/constrained/draining warm cannot be replayed"
            );
        }
        previous = completed;
    }
    let forward = json!({"call_start":warm["first_forward_start"],"call_completion":warm["first_forward_completion"]});
    let call = original_call(&forward, plan, health, limits)?;
    let events = journal(&observations["journal"])?;
    ensure!(
        dispatches(&events, &call)?
            .iter()
            .any(
                |event| event.binding.profile_manifest == expected_profile.manifest
                    && event.binding.charge_bytes == expected_profile.memory_reservation_bytes
            ),
        "final fitted profile first actual successful forward absent"
    );
    for (resource_field, os_field) in [
        ("resources_before", "os_before"),
        ("resources_ready", "os_ready"),
        ("resources_after_first_forward", "os_after_first_forward"),
    ] {
        let snapshot = resource(&document(&warm[resource_field])?)?;
        if resource_field != "resources_before" {
            ensure!(
                snapshot.memory_reserved_bytes >= expected_profile.memory_reservation_bytes,
                "actual final fitted warm reservation absent"
            );
        }
        os_sample(&warm[os_field], receipt, snapshot.memory_limit_bytes)?;
    }
    Ok(())
}
fn refused_original(reference: &Value) -> Result<()> {
    let call = json_artifact(reference)?;
    ensure!(
        call["result"]["ok"] == true,
        "original controller failed before observing refusal"
    );
    let result = &call["result"]["value"];
    if result["outcome"] == "refused" {
        ensure!(result["error"].is_object(), "original refusal cause absent");
    } else {
        ensure!(
            result["outcome"] == "returned",
            "original request outcome unknown"
        );
        let exchange = &result["value"];
        let stdout = exchange["result"]["stdout"]
            .as_str()
            .context("original refused stdout absent")?;
        let envelope: Value = serde_json::from_str(stdout)?;
        ensure!(
            exchange["result"]["exitCode"] != 0 || envelope["isError"] == true,
            "stale/canceled original canonical result was actually delivered"
        );
    }
    Ok(())
}

fn original_arguments(call: &Value) -> Result<(Vec<u8>, Value)> {
    let exchange = if call["schema"] == "airk.development-native-goal-call/1" {
        &call["session_exchange"]
    } else {
        &call["result"]["value"]["value"]
    };
    let argv = array(exchange, "argv")?;
    let index = argv
        .iter()
        .position(|arg| arg == "--args-file")
        .context("original argument file absent")?;
    let path = PathBuf::from(
        argv.get(index + 1)
            .and_then(Value::as_str)
            .context("argument path absent")?,
    );
    let bytes = pinned(&PinnedFile {
        bytes: fs::symlink_metadata(&path)?.len(),
        path,
        sha256: serde_json::from_value(exchange["arguments_sha256"].clone())?,
    })?;
    let value = serde_json::from_slice(&bytes)?;
    Ok((bytes, value))
}
fn source_ids(observations: &Value, before: &Value, after: &Value) -> Result<()> {
    let left = json_artifact(&observations["before_call"])?;
    let right = json_artifact(&observations["after_call"])?;
    let (left_raw, left_args) = original_arguments(&left)?;
    let (right_raw, _) = original_arguments(&right)?;
    ensure!(
        left_raw == right_raw,
        "Focus baseline/current original arguments changed"
    );
    let query = left_args["query"]
        .as_str()
        .context("original query absent")?;
    let source: SearchResponse = serde_json::from_value(before.clone())?;
    let ranked: SearchResponse = serde_json::from_value(after.clone())?;
    let receipt = match ranked.focus.as_ref() {
        Some(FocusObservation::Receipt { receipt }) => receipt,
        _ => anyhow::bail!("actual Focus receipt absent"),
    };
    for (ordinal, hit) in source.hits.into_iter().enumerate() {
        let window = airk_core::native_focus::FocusSourceWindow {
            utf8_start: 0,
            utf8_end: hit.text.len(),
        };
        let candidate = airk_core::native_focus::NativeFocusCandidate::new(
            &receipt.scope,
            query,
            ordinal,
            hit,
            window,
        )?;
        ensure!(
            receipt.original_order.get(ordinal) == Some(&candidate.id),
            "actual Focus candidate identity differs from original complete source/query/order"
        );
    }
    Ok(())
}
fn source_reads(
    observations: &Value,
    before: &Value,
    health: &ServiceHealth,
    limits: ComposedLimits,
) -> Result<()> {
    let search: SearchResponse = serde_json::from_value(before.clone())?;
    let calls = references(&observations["source_calls"])?;
    let mut citations = Vec::new();
    for call in calls {
        let value = canonical_call(&call, health, limits)?;
        let read: airk_core::evidence::ReadEvidence =
            serde_json::from_value(value["read"].clone())?;
        ensure!(
            matches!(
                read.status,
                airk_core::evidence::EvidenceReadStatus::Content
            ) && read.publication == search.publication
                && !read.historical
                && read.validator.is_some(),
            "original source read was not authenticated current content"
        );
        let source: SearchResponse = serde_json::from_value(value)?;
        ensure!(!source.truncated, "source verification read was truncated");
        citations.extend(source.hits.into_iter().map(|hit| hit.citation));
    }
    ensure!(
        !search.hits.is_empty()
            && search
                .hits
                .iter()
                .all(|hit| citations
                    .iter()
                    .any(|citation| serde_json::to_value(citation).ok()
                        == serde_json::to_value(&hit.citation).ok())),
        "complete original source ranges/locations were not read and authenticated"
    );
    Ok(())
}
fn core_configuration(reference: &Value) -> Result<Configuration> {
    let value = document(reference)?;
    if value["schema"] == STATUS_SCHEMA {
        Ok(typed_status(&value)?.configuration)
    } else {
        let configuration: Configuration = serde_json::from_value(value)?;
        configuration.validate()?;
        Ok(configuration)
    }
}
fn comparable(reference: &Value, health: &ServiceHealth, limits: ComposedLimits) -> Result<Value> {
    let original = json_artifact(reference)?;
    if original["schema"] == "airk.private.composed-observation/1"
        && original["request"]["kind"] == "native_command"
    {
        let mut value = document(reference)?;
        if value.get("executable_sha256").is_some() {
            let observed: ServiceHealth = serde_json::from_value(value.clone())?;
            ensure!(
                observed.instance == health.instance
                    && observed.process_id == health.process_id
                    && observed.executable_sha256 == health.executable_sha256,
                "protected native Health identity changed"
            );
            // Core Health's elapsed uptime is the sole normalized native field.
            value
                .as_object_mut()
                .context("Health object absent")?
                .remove("uptime_ms");
        }
        Ok(value)
    } else {
        canonical_call(&original, health, limits)
    }
}
fn comparison_calls(
    observations: &Value,
    health: &ServiceHealth,
    limits: ComposedLimits,
) -> Result<()> {
    let before = observations["baseline_calls"]
        .as_array()
        .context("baseline calls absent")?;
    let after = observations["concurrent_calls"]
        .as_array()
        .context("concurrent calls absent")?;
    ensure!(
        before.len() >= 4 && before.len() == after.len(),
        "protected native call roster incomplete"
    );
    let mut lexical = false;
    let mut minilm = false;
    let mut health_seen = false;
    let mut metadata = false;
    for (before, after) in before.iter().zip(after.iter()) {
        let left = comparable(before, health, limits)?;
        let right = comparable(after, health, limits)?;
        ensure!(
            left == right,
            "unaffected canonical result changed during optional scoring"
        );
        if right.get("execution").is_some() {
            let search: SearchResponse = serde_json::from_value(right.clone())?;
            if let Some(execution) = search.execution {
                use airk_core::search_execution::ServedMode;
                lexical |= execution.served_mode == ServedMode::Lexical;
                minilm |= matches!(
                    execution.served_mode,
                    ServedMode::Vector | ServedMode::Hybrid
                ) && !execution.incomplete;
            }
        } else if right.get("executable_sha256").is_some() {
            health_seen = true;
        } else {
            metadata = true;
        }
    }
    ensure!(
        lexical && minilm && health_seen && metadata,
        "lexical/MiniLM/Health/metadata protected execution incomplete"
    );
    Ok(())
}

fn original_observation(reference: &Value, plan: &Value) -> Result<Value> {
    let observation = json_artifact(reference)?;
    let index = usize::try_from(integer(&observation, "step")?)?;
    ensure!(
        observation["schema"] == "airk.private.composed-observation/1"
            && array(plan, "steps")?.get(index) == Some(&observation["request"])
            && observation["case"] == observation["request"]["case"],
        "original command/controller observation differs from frozen plan"
    );
    Ok(observation)
}

fn parse_control_lines(raw: &[u8]) -> Result<Vec<airk_core::service::ControlResponse>> {
    let mut frames = Vec::new();
    for line in std::str::from_utf8(raw)?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        ensure!(
            frames.len() < 2,
            "unexpected additional native control frame"
        );
        frames.push(serde_json::from_str(line)?);
    }
    ensure!(!frames.is_empty(), "native control frame absent");
    Ok(frames)
}
fn control_frames(reference: &Value) -> Result<Vec<airk_core::service::ControlResponse>> {
    let observation = json_artifact(reference)?;
    ensure!(
        observation["schema"] == "airk.private.composed-observation/1"
            && observation["request"]["kind"] == "native_command"
            && observation["result"]["returncode"] == 0,
        "actual successful native control observation absent"
    );
    let path = PathBuf::from(
        reference["path"]
            .as_str()
            .context("native observation path")?,
    );
    let stdout = path
        .parent()
        .context("native observation parent")?
        .join(format!("{:03}-stdout.bin", integer(&observation, "step")?));
    let raw = pinned(&PinnedFile {
        bytes: fs::symlink_metadata(&stdout)?.len(),
        path: stdout,
        sha256: serde_json::from_value(observation["result"]["stdout_sha256"].clone())?,
    })?;
    parse_control_lines(&raw)
}
fn singleton_fault_session(
    observations: &Value,
    plan: &Value,
    before: &Value,
    snapshot: &Value,
    index: airk_core::index_catalog::IndexId,
) -> Result<()> {
    let call = original_observation(&observations["original_call"], plan)?;
    let id = &call["request"]["request"]["id"];
    let starts = array(plan, "steps")?
        .iter()
        .filter(|step| {
            step["kind"] == "controller"
                && step["request"]["kind"] == "start"
                && &step["request"]["id"] == id
        })
        .collect::<Vec<_>>();
    ensure!(
        starts.len() == 1,
        "original fault request start absent/ambiguous"
    );
    let open = original_observation(&observations["original_open"], plan)?;
    ensure!(
        open["request"]["kind"] == "controller"
            && open["request"]["request"]["kind"] == "open"
            && open["request"]["request"]["id"] == starts[0]["request"]["session"]
            && open["result"]["ok"] == true
            && open["result"]["value"]["kind"] == "ready"
            && open["result"]["value"]["launch_count"] == 1
            && open["result"]["value"]["scope_result"]["status"] == "scope",
        "actual original singleton-set I2 open/selection absent"
    );
    let config = &open["request"]["request"]["config"];
    ensure!(
        config["bindings_sha256"] == observations["before_binding"]["sha256"]
            && config["index_bindings"] == observations["before_binding"]["path"]
            && config["index_id"] == serde_json::to_value(index)?
            && open["result"]["value"]["identity"]["bindings_sha256"] == config["bindings_sha256"]
            && open["result"]["value"]["identity"]["index_id"] == config["index_id"]
            && open["result"]["value"]["identity"]["service_instance"] == snapshot["instance"]
            && before["instance"] == snapshot["instance"],
        "original fault session did not use exact native singleton membership snapshot"
    );
    Ok(())
}
fn membership_revocation(observations: &Value, plan: &Value, dispatch: &Record) -> Result<()> {
    use airk_core::{
        catalog_management::{CatalogItem, CatalogOperation},
        index_catalog::{IndexDefinition, Membership, QueryProject},
        service::ControlResponse,
    };
    let before = json_artifact(&observations["before_binding"])?;
    let snapshot = json_artifact(&observations["before_membership"])?;
    ensure!(
        before["schema"] == "airk.private-arbiter-index-bindings/1"
            && snapshot["schema"] == "airk.private-composed-membership-before/1"
            && snapshot["evaluation"] == true
            && snapshot["release_qualified"] == false
            && snapshot["namespace"] == serde_json::to_value(dispatch.binding.scope.catalog)?
            && before["namespace"] == snapshot["namespace"]
            && before["catalog_revision"] == snapshot["catalog_revision"],
        "original singleton membership namespace/revision differs"
    );
    let index: IndexDefinition = serde_json::from_value(snapshot["index"].clone())?;
    let project: QueryProject = serde_json::from_value(snapshot["project"].clone())?;
    let membership: Membership = serde_json::from_value(snapshot["membership"].clone())?;
    index.validate()?;
    project.validate()?;
    membership.validate()?;
    let selection: airk_core::service::WindowScopeSelection =
        serde_json::from_value(snapshot["window_scope_selection"].clone())?;
    ensure!(
        index.id == dispatch.binding.scope.index_id
            && !index.tombstone
            && membership.index_id == index.id
            && membership.project_id == project.id
            && selection.namespace == dispatch.binding.scope.catalog
            && selection.index_ids == vec![index.id]
            && selection.project_ids == vec![project.id]
            && serde_json::to_value(selection.expected_catalog_revision)?
                == snapshot["catalog_revision"],
        "membership proof did not bind original fault index/project"
    );
    let index_value = serde_json::to_value(index.id)?;
    let original = array(&before, "bindings")?
        .iter()
        .filter(|row| row["index_id"] == index_value)
        .collect::<Vec<_>>();
    ensure!(
        original.len() == 1
            && original[0]["definition"] == snapshot["index"]
            && original[0]["window_scope_selection"] == snapshot["window_scope_selection"],
        "original W4 binding/snapshot differs"
    );
    singleton_fault_session(observations, plan, &before, &snapshot, index.id)?;
    let operation = CatalogOperation::RemoveMembership {
        project_id: project.id,
        index_id: index.id,
        expected_scope_revision: project.scope_revision,
        expected_membership_revision: membership.revision,
    };
    ensure!(
        serde_json::to_value(&operation)? == snapshot["operation"]
            && operation.requires_authority_fence()
            && !operation.requires_worker_drain(),
        "source mutation is not exact no-worker-wait membership revocation"
    );
    for (key, command, flag, target) in [
        ("after_project", "show", "--set", project.id.to_string()),
        ("after_memberships", "list", "--set", project.id.to_string()),
        ("after_index", "show", "--index", index.id.to_string()),
    ] {
        let observation = original_observation(&observations[key], plan)?;
        let argv = array(&observation["request"], "argv")?;
        ensure!(
            argv.windows(2)
                .any(|pair| pair[0] == "library" && pair[1] == command)
                && argv
                    .windows(2)
                    .any(|pair| pair[0] == flag && pair[1] == target),
            "actual post-revoke native command targets foreign entity"
        );
        if key == "after_memberships" {
            ensure!(
                argv.windows(2)
                    .any(|pair| pair[0] == "--kind" && pair[1] == "memberships")
                    && argv.iter().any(|arg| arg == "--all"),
                "post-revoke membership observation filtered/truncated"
            );
        }
    }
    let mut frames = control_frames(&observations["revoke_call"])?;
    ensure!(
        frames.len() == 2,
        "membership change must retain preview AND applied native frames"
    );
    let applied = frames.pop().context("applied frame absent")?;
    let ControlResponse::CatalogPreview {
        preview,
        preview_digest,
    } = frames.pop().context("preview frame absent")?
    else {
        anyhow::bail!("first native membership frame is not CatalogPreview")
    };
    let ControlResponse::CatalogApplied {
        receipt,
        fenced_receipt: Some(fenced),
    } = applied
    else {
        anyhow::bail!("membership mutation lacks actual fenced CatalogApplied")
    };
    fenced.validate()?;
    ensure!(
        preview.operation == operation
            && preview.identity()? == preview_digest
            && preview.authority_fence_required
            && !preview.file_data_changes
            && preview.catalog_revision == selection.expected_catalog_revision
            && preview.next_revision == preview.catalog_revision.next()?
            && preview.request_digest == receipt.request_digest
            && receipt == fenced.metadata
            && fenced.preview_digest == preview_digest
            && fenced.request.operation == operation
            && fenced.request.expected_catalog_revision == preview.catalog_revision
            && fenced.request.key.namespace == dispatch.binding.scope.catalog
            && serde_json::to_value(fenced.proof.service)? == snapshot["instance"]
            && receipt.catalog_revision == preview.next_revision
            && fenced.proof.worker_obligation.is_none()
            && fenced.proof.worker_confirmations.is_empty(),
        "actual membership preview/apply/fence lineage differs"
    );
    let project_frame = control_frames(&observations["after_project"])?;
    let [
        ControlResponse::CatalogDetail {
            detail: project_after,
            ..
        },
    ] = project_frame.as_slice()
    else {
        anyhow::bail!("actual post-revoke project detail absent")
    };
    let Some(CatalogItem::Project(actual_project)) = &project_after.item else {
        anyhow::bail!("fault set was deleted instead of membership revoked")
    };
    let mut expected_project = project.clone();
    expected_project.scope_revision = project.scope_revision.next()?;
    if project.default_indexes.contains(&index.id) {
        expected_project.defaults_revision = project.defaults_revision.next()?;
        expected_project
            .default_indexes
            .retain(|id| *id != index.id);
    }
    ensure!(
        actual_project == &expected_project
            && project_after.namespace == dispatch.binding.scope.catalog
            && project_after.revision == receipt.catalog_revision,
        "actual fault project scope/revision differs after removal"
    );
    let membership_frame = control_frames(&observations["after_memberships"])?;
    let [ControlResponse::CatalogPage { page, .. }] = membership_frame.as_slice() else {
        anyhow::bail!("actual post-revoke membership page absent")
    };
    ensure!(
        page.namespace == dispatch.binding.scope.catalog
            && page.revision == receipt.catalog_revision
            && page.items.is_empty()
            && page.continuation.is_none(),
        "fault membership still present or observation truncated"
    );
    let index_frame = control_frames(&observations["after_index"])?;
    let [
        ControlResponse::CatalogDetail {
            detail: index_after,
            ..
        },
    ] = index_frame.as_slice()
    else {
        anyhow::bail!("actual post-revoke fault index detail absent")
    };
    ensure!(
        index_after.namespace == dispatch.binding.scope.catalog
            && index_after.revision == receipt.catalog_revision
            && index_after.item == Some(CatalogItem::Index(index)),
        "membership revoke retired/changed the actual source index"
    );
    Ok(())
}

fn race_case(observations: &Value, plan: &Value, cancel: bool) -> Result<()> {
    let events = journal(&observations["journal"])?;
    let dispatch = trigger(observations, &events)?;
    let result = returned(&events, dispatch)?;
    if cancel {
        let abort = original_observation(&observations["abort_call"], plan)?;
        ensure!(
            abort["request"]["request"]["kind"] == "abort"
                && abort["result"]["ok"] == true
                && abort["result"]["value"]["abort_signalled"] == true
                && abort["request"]["request"]["id"] == abort["result"]["value"]["id"],
            "actual original AbortSignal absent"
        );
        mutation_interval(&abort, dispatch, result)?;
        let close = original_observation(&observations["session_close"], plan)?;
        let starts = array(plan, "steps")?
            .iter()
            .filter(|step| {
                step["kind"] == "controller"
                    && step["request"]["kind"] == "start"
                    && step["request"]["id"] == abort["request"]["request"]["id"]
            })
            .collect::<Vec<_>>();
        ensure!(
            starts.len() == 1
                && close["request"]["request"]["kind"] == "close"
                && close["request"]["request"]["id"] == starts[0]["request"]["session"],
            "AbortSignal/close did not target original actual session"
        );
        session_close(
            &close["result"]["value"],
            Some(&cancellation_poison(observations)?),
        )?;
    } else {
        let revoke = original_observation(&observations["revoke_call"], plan)?;
        ensure!(
            revoke["result"]["returncode"] == 0,
            "actual native source revocation failed"
        );
        mutation_interval(&revoke, dispatch, result)?;
        membership_revocation(observations, plan, dispatch)?;
    }
    refused_original(&observations["original_call"])?;
    let disable = core_configuration(&observations["disable_call"])?;
    let stage = if dispatch.binding.task == Task::FocusRank {
        Stage::Focus
    } else {
        Stage::Context
    };
    ensure!(
        !disable.stage(stage).enabled,
        "final resident charge release lacked explicit stage disable"
    );
    settled_released(&events, dispatch)
}
fn independent_stages(
    observations: &Value,
    health: &ServiceHealth,
    focus: &Profile,
    limits: ComposedLimits,
) -> Result<()> {
    let configs = references(&observations["config_calls"])?;
    ensure!(
        configs.len() >= 3,
        "stage independence configuration sequence absent"
    );
    let mut configurations = Vec::new();
    for value in configs {
        let status = typed_status(&value)?;
        configurations.push(status.configuration);
    }
    let mut context_off = false;
    let mut focus_off = false;
    for pair in configurations.windows(2) {
        ensure!(
            pair[1].revision > pair[0].revision,
            "configuration revisions did not advance"
        );
        if pair[0].context.enabled && !pair[1].context.enabled {
            ensure!(
                pair[1].focus == pair[0].focus,
                "disabling Context changed Focus consent"
            );
            context_off = true;
        }
        if pair[0].focus.enabled && !pair[1].focus.enabled {
            ensure!(
                pair[1].context == pair[0].context,
                "disabling Focus changed Context consent"
            );
            focus_off = true;
        }
    }
    ensure!(
        context_off && focus_off,
        "both independent disable transitions absent"
    );
    let baselines = references(&observations["baseline_calls"])?;
    let currents = references(&observations["current_calls"])?;
    ensure!(
        !baselines.is_empty() && baselines.len() == currents.len(),
        "stage independence baseline/current calls absent"
    );
    let mut applied = false;
    for (before, after) in baselines.iter().zip(currents.iter()) {
        let before = canonical_call(before, health, limits)?;
        let after = canonical_call(after, health, limits)?;
        if after.get("focus").is_some() {
            search_preservation(&before, &after, focus)?;
            applied = true;
        } else {
            ensure!(
                before == after,
                "excluded or disabled-stage full native result changed"
            );
        }
    }
    let events = journal(&observations["journal"])?;
    for event in events
        .iter()
        .filter(|event| event.kind == Kind::DispatchAdmitted)
    {
        if let Some(configuration) = configurations.iter().find(|configuration| {
            configuration.revision == event.binding.scope.configuration_revision
        }) {
            ensure!(
                configuration.eligible(event.binding.scope.index_id, event.binding.task),
                "unauthorized stage dispatch after independent disable"
            );
        }
    }
    ensure!(applied, "independent Focus-only actual search absent");
    Ok(())
}
/// All ten actual cases are reconciled from originals. Wrapper verdict/check
/// flags are deliberately ignored and cannot manufacture release authority.
pub(super) fn validate(
    profile: &Profile,
    expected: &Binding,
    records: &[Original],
    policy: Option<&ResourcePolicy>,
) -> Result<()> {
    let limits = ComposedLimits::from_policy(policy)?;
    let (_, receipt) = original_schema(records, "airk.private.actual-composed-service/1")?;
    ensure!(
        receipt["proof_class"] == "ActualComposed" && receipt["qualification_granted"] == false,
        "original composed evidence scope changed"
    );
    let cases = object(receipt, "cases")?;
    ensure!(
        cases.keys().map(String::as_str).collect::<BTreeSet<_>>() == BTreeSet::from(CASES),
        "actual composed case roster incomplete/foreign"
    );
    let profiles = fixture_profiles(receipt, profile, expected)?;
    for profile in profiles.values() {
        limits.profile(profile)?;
    }
    let health = verify_health(receipt, &profiles)?;
    let plan = json_artifact(&receipt["driver_plan"])?;
    ensure!(
        plan["schema"] == "airk.private.composed-execution-plan/1",
        "actual frozen driver plan absent"
    );
    verify_named_seals(object(&plan, "pins")?)?;
    let focus = &profiles[&ModelChoice::FocusQwenReranker06B];
    for id in CASES {
        let case = &cases[id];
        let evidence = array(case, "evidence")?;
        ensure!(
            !evidence.is_empty(),
            "actual composed case has no original evidence"
        );
        for reference in evidence {
            artifact(reference)?;
        }
        let observations = &case["observations"];
        match id {
            "baseline_identity" => {
                resource(&document(&observations["resources"])?)?;
                let bindings = json_artifact(&observations["bindings"])?;
                ensure!(
                    bindings["schema"] == "airk.private-arbiter-index-bindings/1"
                        && bindings["instance"] == serde_json::to_value(health.instance)?
                        && bindings["namespace"]
                            == serde_json::to_value(
                                health
                                    .catalog_clock
                                    .context("catalog clock absent")?
                                    .namespace
                            )?,
                    "actual baseline source/service namespace differs"
                );
            }
            "focus_only_search" => {
                let before = canonical_call(
                    &json_artifact(&observations["before_call"])?,
                    &health,
                    limits,
                )?;
                let after = canonical_call(
                    &json_artifact(&observations["after_call"])?,
                    &health,
                    limits,
                )?;
                let before_status = typed_status(&document(&observations["status_before"])?)?;
                let after_status = typed_status(&document(&observations["status_after"])?)?;
                ensure!(
                    !before_status.configuration.focus.enabled
                        && !before_status.configuration.context.enabled
                        && after_status.configuration.focus.enabled
                        && !after_status.configuration.context.enabled,
                    "Focus-only baseline/independent consent differs"
                );
                search_preservation(&before, &after, focus)?;
                source_ids(observations, &before, &after)?;
                source_reads(observations, &before, &health, limits)?;
            }
            "dual_compact" | "dual_large" => {
                let model = if id == "dual_compact" {
                    ModelChoice::ContextQwenCompact
                } else {
                    ModelChoice::ContextQwenLarge
                };
                composed_packet(observations, &plan, &health, &profiles, model, limits)?;
                final_warm_load(
                    observations,
                    receipt,
                    &plan,
                    &health,
                    &profiles[&model],
                    limits,
                )?;
                let resource_calls = references(&observations["resource_calls"])?;
                let limit = resource(&resource_calls[0])?.memory_limit_bytes;
                let samples = observations["os_samples"]
                    .as_array()
                    .context("actual composed OS samples absent")?;
                ensure!(
                    samples.len() >= 3,
                    "actual composed OS before/during/after samples absent"
                );
                for sample in samples {
                    os_sample(sample, receipt, limit)?;
                }
                if id == "dual_large" {
                    let events = journal(&observations["journal"])?;
                    let release: Record =
                        serde_json::from_value(json_artifact(&observations["prior_release"])?)?;
                    release.validate()?;
                    ensure!(
                        release.kind == Kind::ChargeReleased
                            && release.binding.profile_manifest
                                == profiles[&ModelChoice::ContextQwenCompact].manifest
                            && events.iter().any(|event| event.sequence == release.sequence
                                && serde_json::to_value(event).ok()
                                    == serde_json::to_value(&release).ok()),
                        "previous Compact actual final charge release absent"
                    );
                }
            }
            "noninterference" => {
                let events = journal(&observations["journal"])?;
                let dispatch = trigger(observations, &events)?;
                let result = returned(&events, dispatch)?;
                comparison_calls(observations, &health, limits)?;
                for reference in observations["concurrent_calls"]
                    .as_array()
                    .context("concurrent call refs absent")?
                {
                    mutation_interval(&original_observation(reference, &plan)?, dispatch, result)?;
                }
            }
            "cancel_drain" => race_case(observations, &plan, true)?,
            "source_revoke" => {
                race_case(observations, &plan, false)?;
                ensure!(
                    comparable(&observations["unaffected_call"], &health, limits)?
                        == comparable(&observations["unaffected_baseline"], &health, limits)?,
                    "unaffected-index full native result changed after original source revocation"
                );
            }
            "remove_recover" => {
                let prior = core_configuration(&observations["prior_config"])?;
                let recovered = typed_status(&document(&observations["recovered_status"])?)?;
                ensure!(
                    prior == recovered.configuration,
                    "Recover changed consent or silently enabled a stage"
                );
                let recover = original_observation(&observations["recover_call"], &plan)?;
                let argv = array(&recover["request"], "argv")?;
                ensure!(
                    argv.iter().any(|arg| arg == "recover") && recover["result"]["returncode"] == 0,
                    "actual explicit cached Recover absent"
                );
                let missing = typed_status(&document(&observations["missing_status"])?)?;
                let remove = original_observation(&observations["remove_call"], &plan)?;
                let argv = array(&remove["request"], "argv")?;
                ensure!(
                    argv.iter().any(|arg| arg == "remove") && remove["result"]["returncode"] == 0,
                    "actual explicit Remove absent"
                );
                let positions = argv
                    .iter()
                    .enumerate()
                    .filter(|(_, arg)| *arg == "--profile")
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                ensure!(
                    positions.len() == 1,
                    "actual Remove profile argument absent/ambiguous"
                );
                let manifest: Sha256Digest = argv
                    .get(positions[0] + 1)
                    .and_then(Value::as_str)
                    .context("removed profile absent")?
                    .parse()?;
                let removed = profiles
                    .values()
                    .find(|profile| profile.manifest == manifest)
                    .context("Remove targets foreign profile")?;
                let stage = removed.declared_stage().context("removed stage absent")?;
                ensure!(
                    prior.stage(stage).enabled
                        && prior.stage(stage).selected_profile == Some(manifest),
                    "Recover did not retain original enabled selected consent"
                );
                let recovered_stage = match stage {
                    Stage::Focus => &recovered.focus,
                    Stage::Context => &recovered.context,
                };
                ensure!(
                    recovered_stage.availability == Availability::Ready
                        && recovered_stage.active_profile == Some(manifest),
                    "Recover never observed the actual selected cached resident"
                );
                let missing_stage = match stage {
                    Stage::Focus => &missing.focus,
                    Stage::Context => &missing.context,
                };
                ensure!(
                    missing_stage.active_profile != Some(manifest)
                        && missing_stage
                            .profiles
                            .iter()
                            .any(|row| row.profile == Some(manifest)
                                && row.availability == Availability::MissingAssets),
                    "actual removed profile MissingAssets/release observation absent"
                );
                let events = journal(&observations["journal"])?;
                let dispatch = events
                    .iter()
                    .find(|event| {
                        event.kind == Kind::DispatchAdmitted
                            && event.binding.profile_manifest == manifest
                    })
                    .context("removed profile actual original dispatch absent")?;
                settled_released(&events, dispatch)?;
                let release = events
                    .iter()
                    .find(|event| {
                        event.kind == Kind::ChargeReleased
                            && event.binding.dispatch_identity == dispatch.binding.dispatch_identity
                    })
                    .context("actual removed charge absent")?;
                ensure!(
                    release.unix_ms <= integer(&remove, "completed_unix_ms")?,
                    "Remove completed before actual final resident release"
                );
            }
            "stage_independence" => independent_stages(observations, &health, focus, limits)?,
            "normal_shutdown" => {
                let closes = observations["session_closes"]
                    .as_array()
                    .context("actual session close refs absent")?;
                ensure!(!closes.is_empty(), "actual session close roster empty");
                let cancel = &cases["cancel_drain"]["observations"];
                let expected_poison = cancellation_poison(cancel)?;
                let mut canceled_close = false;
                for reference in closes {
                    let closed = json_artifact(reference)?;
                    let value = if closed["schema"] == "airk.private.composed-observation/1" {
                        &closed["result"]["value"]
                    } else {
                        &closed
                    };
                    if reference["sha256"] == cancel["session_close"]["sha256"] {
                        ensure!(!canceled_close, "duplicate original canceled session close");
                        canceled_close = true;
                        session_close(value, Some(&expected_poison))?;
                    } else {
                        session_close(value, None)?;
                    }
                }
                ensure!(
                    canceled_close,
                    "original canceled session natural exit omitted from final closure"
                );
                let closed = json_artifact(&observations["closed"])?;
                ensure!(
                    closed["schema"] == "airk.private-arbiter-fixture-closed/1"
                        && closed["instance"] == serde_json::to_value(health.instance)?
                        && closed["shutdown_completed"] == true
                        && closed["service_lease_reacquired"] == true,
                    "actual native shutdown/lease reacquisition absent"
                );
                natural_composed_owner(&json_artifact(&observations["owned_execution"])?)?;
                let summary: RecorderSummary =
                    serde_json::from_value(json_artifact(&observations["observer_closed"])?)?;
                summary.validate()?;
                let events = journal(&receipt["dispatch_journal"])?;
                ensure!(
                    summary.complete
                        && summary.dropped == 0
                        && summary.writer_failed == 0
                        && summary.error.is_none()
                        && summary.records == events.len()
                        && summary.bytes == artifact(&receipt["dispatch_journal"])?.len() as u64
                        && summary.last_sequence == events.len() as u64,
                    "actual recorder lost/failed/omitted native events"
                );
                ensure!(
                    artifact(&observations["before_seal"])?
                        == artifact(&observations["after_seal"])?,
                    "original source/artifact seals drifted"
                );
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_inference::inference_engine::pool::arbiter::{ScoreReceipt, execution_observer};
    #[test]
    fn composed_policy_preserves_historical30s_and_requires_approved_current120s() -> Result<()> {
        let historical = ComposedLimits::from_policy(None)?;
        historical.read_deadline(30_000)?;
        historical.development_deadline(&json!(30.0))?;
        assert!(historical.read_deadline(30_001).is_err());
        assert!(historical.development_deadline(&json!(30.001)).is_err());
        assert!(historical.read_deadline(0).is_err());
        let policy = ResourcePolicy::software_v2();
        let current = ComposedLimits::from_policy(Some(&policy))?;
        current.read_deadline(120_000)?;
        current.development_deadline(&json!(120.0))?;
        assert!(current.read_deadline(120_001).is_err());
        assert!(current.development_deadline(&json!(120.001)).is_err());
        let mut unapproved_shape = policy;
        unapproved_shape.deadline_ms = 120_001;
        assert!(ComposedLimits::from_policy(Some(&unapproved_shape)).is_err());
        Ok(())
    }
    #[test]
    fn current_composed_budget_refuses_old_or_substituted_original_arguments() -> Result<()> {
        let policy = ResourcePolicy::software_v2();
        let current = ComposedLimits::from_policy(Some(&policy))?;
        let budget = Budget {
            calls: 8,
            rounds: 3,
            fanout: 8,
            input_tokens: 8192,
            candidates: 32,
            max_bytes: 262144,
            deadline_ms: 120000,
        };
        current.budget(&budget, Some(&budget))?;
        assert!(current.budget(&budget, None).is_err());
        let mut old = budget.clone();
        old.input_tokens = 2048;
        old.deadline_ms = 30000;
        ComposedLimits::from_policy(None)?.budget(&old, None)?;
        assert!(current.budget(&old, Some(&old)).is_err());
        assert!(current.budget(&budget, Some(&old)).is_err());
        let mut substituted = budget.clone();
        substituted.candidates = 16;
        assert!(current.budget(&budget, Some(&substituted)).is_err());
        let mut overrun = budget.clone();
        overrun.deadline_ms = 120001;
        assert!(current.budget(&overrun, Some(&overrun)).is_err());
        Ok(())
    }
    #[test]
    fn composed_profile_widths_do_not_relabel_historical_context_or_focus() -> Result<()> {
        let policy = ResourcePolicy::software_v2();
        let historical = ComposedLimits::from_policy(None)?;
        let current = ComposedLimits::from_policy(Some(&policy))?;
        let catalog = airk_retrieval::arbiter::runtime::catalog_profiles()?;
        let focus = catalog
            .iter()
            .find(|profile| profile.model == ModelChoice::FocusQwenReranker06B)
            .context("Focus fixture absent")?;
        historical.profile(focus)?;
        current.profile(focus)?;
        let mut context = catalog
            .iter()
            .find(|profile| profile.model == ModelChoice::ContextQwenCompact)
            .context("Context fixture absent")?
            .clone();
        context.max_tokens = 2048;
        historical.profile(&context)?;
        assert!(current.profile(&context).is_err());
        context.max_tokens = 8192;
        current.profile(&context)?;
        assert!(historical.profile(&context).is_err());
        let mut wrong_focus = focus.clone();
        wrong_focus.max_tokens = 8192;
        assert!(current.profile(&wrong_focus).is_err());
        Ok(())
    }
    fn events() -> Result<Vec<Record>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("root")?;
        let request: airk_core::native_focus::NativeFocusRequest = serde_json::from_slice(
            &fs::read(root.join("fixtures/split/arbiter/native_focus_request.json"))?,
        )?;
        let binding = execution_observer::Binding {
            observer_id: airk_core::identity::Incarnation::from_bytes([1; 16]),
            dispatch_identity: 1,
            profile_manifest: Sha256Digest::from_bytes([2; 32]),
            task: Task::FocusRank,
            input_digest: Sha256Digest::from_bytes([3; 32]),
            question_utf8_sha256: sha(b"synthetic observer control"),
            original_deadline_unix_ms: 30_000,
            tokens: 512,
            charge_identity: "synthetic-observer:1".into(),
            charge_bytes: 4096,
            scope: request.scope,
        };
        let mut events = Vec::new();
        for (index, kind) in [
            Kind::DispatchAdmitted,
            Kind::ScoreReturned,
            Kind::JobSettled,
            Kind::ProfileReleased,
            Kind::ChargeReleased,
        ]
        .into_iter()
        .enumerate()
        {
            let mut event = Record {
                schema: "airk.private.native-dispatch/1".into(),
                sequence: index as u64 + 1,
                unix_ms: 100 + index as u64,
                kind,
                binding: binding.clone(),
                outcome: None,
                finite_raw_scores: None,
                raw_scores: None,
                raw_score_bits: None,
                work: None,
                error: None,
                return_observed: false,
                proof: None,
            };
            if kind == Kind::ScoreReturned {
                event.outcome = Some(Outcome::Success);
                event.finite_raw_scores = Some(true);
                event.raw_scores = Some(vec![Some(0.0), Some(1.0)]);
                event.raw_score_bits = Some(vec![0.0f32.to_bits(), 1.0f32.to_bits()]);
                event.work = Some(ScoreReceipt {
                    input_digest: binding.input_digest,
                    input_tokens: 512,
                    queue_us: 1,
                    compile_us: 2,
                    inference_us: 3,
                });
                event.return_observed = true;
            }
            if kind == Kind::JobSettled {
                event.return_observed = true;
                event.proof =
                    Some("actual_job_compute_delivery_and_per_job_resident_permit_dropped".into());
            }
            if matches!(kind, Kind::ProfileReleased | Kind::ChargeReleased) {
                event.proof = Some(
                    "actual_final_reservation_drop_after_native_scorer_and_source_teardown".into(),
                );
            }
            events.push(event);
        }
        Ok(events)
    }
    #[test]
    fn original_native_sequence_requires_same_actual_input_work_and_final_release() -> Result<()> {
        let original = events()?;
        settled_released(&original, &original[0])?;
        for index in [1, 2, 3, 4] {
            let mut invalid = original.clone();
            invalid[index].binding.charge_bytes += 1;
            assert!(settled_released(&invalid, &invalid[0]).is_err());
        }
        let mut invalid = original.clone();
        invalid[4].sequence = 2;
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        let mut invalid = original.clone();
        invalid.pop();
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        let mut invalid = original.clone();
        invalid[1].work.as_mut().unwrap().input_tokens = 511;
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        let mut invalid = original;
        invalid[1].raw_score_bits.as_mut().unwrap()[1] = f32::NAN.to_bits();
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        Ok(())
    }

    #[test]
    fn native_membership_cli_preserves_two_typed_control_frames_and_refuses_partial_or_extra()
    -> Result<()> {
        let raw = br#"{"status":"failure","code":"preview-control"}
{"status":"failure","code":"applied-control"}
"#;
        ensure!(
            parse_control_lines(raw)?.len() == 2,
            "actual compact two-line framing changed"
        );
        ensure!(
            parse_control_lines(br#"{"status":"#).is_err(),
            "partial native frame accepted"
        );
        let mut extra = raw.to_vec();
        extra.extend_from_slice(
            br#"{"status":"failure","code":"extra"}
"#,
        );
        ensure!(
            parse_control_lines(&extra).is_err(),
            "extra native control frame ignored"
        );
        Ok(())
    }
    #[test]
    fn source_revoke_race_cannot_borrow_another_forward_or_refresh_deadline() -> Result<()> {
        let original = events()?;
        let mut returned = original[1].clone();
        returned.unix_ms = 200;
        mutation_interval(
            &serde_json::json!({"started_unix_ms":110,"completed_unix_ms":199}),
            &original[0],
            &returned,
        )?;
        assert!(
            mutation_interval(
                &serde_json::json!({"started_unix_ms":99,"completed_unix_ms":150}),
                &original[0],
                &returned
            )
            .is_err()
        );
        assert!(
            mutation_interval(
                &serde_json::json!({"started_unix_ms":150,"completed_unix_ms":200}),
                &original[0],
                &returned
            )
            .is_err()
        );
        returned.binding.original_deadline_unix_ms += 1;
        assert!(same_binding(&original[0], &returned).is_err());
        Ok(())
    }
    #[test]
    fn original_session_close_preserves_expected_cancel_poison_and_refuses_other_failure()
    -> Result<()> {
        let mut closed = json!({"kind":"closed","child_exit_observed":true,
            "child_exit_code":0,"child_signal_code":null,"launch_count":1,"poisoned":null});
        session_close(&closed, None)?;
        closed["poisoned"] = json!("original_abort_code");
        assert!(session_close(&closed, None).is_err());
        session_close(&closed, Some("original_abort_code"))?;
        assert!(session_close(&closed, Some("another_code")).is_err());
        closed["child_exit_code"] = json!(1);
        assert!(session_close(&closed, Some("original_abort_code")).is_err());
        Ok(())
    }
    #[test]
    fn native_composed_refusal_fixture_emitter() -> Result<()> {
        let original = events()?;
        settled_released(&original, &original[0])?;
        let mut rows =
            vec![json!({"id":"complete_original_sequence","events":original,"valid":true})];
        let mut invalid = events()?;
        invalid.pop();
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        rows.push(json!({"id":"missing_actual_charge_release","events":invalid,"valid":false}));
        let mut invalid = events()?;
        invalid[1].binding.charge_bytes += 1;
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        rows.push(json!({"id":"crossed_actual_reservation","events":invalid,"valid":false}));
        let mut invalid = events()?;
        invalid[1].work.as_mut().unwrap().input_tokens = 511;
        assert!(settled_released(&invalid, &invalid[0]).is_err());
        rows.push(json!({"id":"changed_compiled_input_cost","events":invalid,"valid":false}));
        if let Some(path) = std::env::var_os("AIRK_WRITE_COMPOSED_VALIDATION_CASES") {
            fs::write(
                path,
                serde_json::to_vec_pretty(&json!({
                "schema":"airk.private.composed-native-refusal-cases/1",
                "synthetic_controls":true,"qualification_granted":false,"cases":rows}))?,
            )?;
        }
        Ok(())
    }
}
