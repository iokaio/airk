// SPDX-License-Identifier: Apache-2.0
//! Opt-in observations over the existing real split fixture, never another fixture.
use super::*;
use airk_core::index_catalog::CatalogRevision;
use std::io::Write;

pub(super) struct Capture {
    directory: PathBuf,
    events: fs::File,
    started: Instant,
    next_sample: Instant,
    source_before: Value,
    clients: Vec<McpSession>,
    selection: SessionScope,
    /// The catalog revision the measured selection was last confirmed at.
    confirmed_revision: CatalogRevision,
    reconfirm_required: bool,
    authority_baseline: Value,
    publication_baseline: Option<Value>,
    data: Vec<(String, PathBuf)>,
    sequence: usize,
    waves: usize,
    running_successes: usize,
    index: IndexId,
    expected_hash: String,
    finished: bool,
    require_running_overlap: bool,
}
fn source_inventory(f: &Fixture) -> Result<Value> {
    let mut entries = Vec::new();
    for (root, registration) in f.registrations.iter().enumerate() {
        for directory in ["api", "impl", "private", "api-other"] {
            let path = Path::new(&registration.root)
                .join(directory)
                .join("policy.md");
            entries.push(json!({"root_alias":root,"path":format!("{directory}/policy.md"),
                "bytes":fs::metadata(&path)?.len(),"sha256":airk_cli::runtime_pair::digest_file(&path)?}));
        }
    }
    Ok(json!({"shape":"existing four original files per source; unchanged","entries":entries}))
}
fn file_bytes(owner: &Path, roots: &[(String, PathBuf)]) -> Result<Value> {
    let owner = owner.canonicalize()?;
    let mut result = serde_json::Map::new();
    let mut count = 0usize;
    for (alias, root) in roots {
        ensure!(
            root.starts_with(&owner) && root != &owner,
            "metric root outside exact fixture"
        );
        let mut todo = vec![root.clone()];
        let mut bytes = 0u64;
        let mut files = 0usize;
        let mut raced = false;
        while let Some(path) = todo.pop() {
            let metadata = match fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    raced = true;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            ensure!(
                !metadata.file_type().is_symlink(),
                "redirected metric entry"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "reparse metric entry"
                );
            }
            count += 1;
            ensure!(count <= 20000, "owned metric entry budget exhausted");
            if metadata.is_dir() {
                match fs::read_dir(&path) {
                    Ok(entries) => {
                        for entry in entries {
                            match entry {
                                Ok(entry) => {
                                    ensure!(
                                        count
                                            .checked_add(todo.len())
                                            .is_some_and(|pending| pending < 20000),
                                        "owned metric pending-entry budget exhausted"
                                    );
                                    todo.push(entry.path());
                                }
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                    raced = true
                                }
                                Err(error) => return Err(error.into()),
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => raced = true,
                    Err(error) => return Err(error.into()),
                }
            } else if metadata.is_file() {
                files += 1;
                bytes = bytes
                    .checked_add(metadata.len())
                    .context("metric bytes overflow")?;
            } else {
                anyhow::bail!("unexpected metric data entry");
            }
        }
        result.insert(alias.clone(),json!({"logical_bytes":bytes,"files":files,"raced":raced,"state":if raced{"partial"}else{"observed_nonatomic"},
            "basis":"actual owned file lengths, sampled/non-atomic; physical allocation and missed spikes unknown"}));
    }
    Ok(Value::Object(result))
}
async fn running(f: &Fixture, snapshot: &SplitSnapshot) -> Result<bool> {
    let Some(job) = snapshot.receipt.jobs.last() else {
        return Ok(false);
    };
    let response =
        airk_cli::service_runtime::request(&f.control, ControlMethod::Job { id: job.job }).await?;
    Ok(
        matches!(response,ControlResponse::Job{job}if job.state==airk_core::service::JobState::Running),
    )
}

// Compare complete native authority items, excluding only enclosing observation
// revision. A changed membership, Project or Index definition refuses.
async fn identity(f: &Fixture, revision: CatalogRevision) -> Result<(Value, Value)> {
    use airk_core::catalog_management::CatalogEntity;
    let namespace = ServiceRegistry::open_saved(&f.control)?.incarnation()?;
    let entities = [
        CatalogEntity::Project { id: f.projects[0] },
        CatalogEntity::Index { id: f.indexes[0] },
        CatalogEntity::Membership {
            project_id: f.projects[0],
            index_id: f.indexes[0],
        },
    ];
    let mut items = Vec::new();
    let mut reads = Vec::new();
    for entity in entities {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::CatalogDetail {
                entity,
                expected_catalog_revision: Some(revision),
            },
        )
        .await?;
        let ControlResponse::CatalogDetail { detail, .. } = &response else {
            anyhow::bail!(
                "native independent identity observation refused: {}",
                serde_json::to_string(&response)?
            );
        };
        ensure!(
            detail.namespace == namespace && detail.revision == revision && detail.item.is_some(),
            "native independent identity absent or stale"
        );
        items.push(serde_json::to_value(&detail.item)?);
        reads.push(serde_json::to_value(&response)?);
    }
    Ok((json!(items), json!(reads)))
}

impl Capture {
    pub(super) async fn start(
        f: &mut Fixture,
        service: &mut OwnedProcess,
        case: &str,
    ) -> Result<Option<Self>> {
        let Some(base) = std::env::var_os("AIRK_P10_RESOURCE_OUTPUT_DIR") else {
            return Ok(None);
        };
        let case_started = Instant::now();
        // Performance evidence requires an explicitly selected fresh matched pair.
        ensure!(
            std::env::var_os("AIRK_CATALOG_CANDIDATE_BINARY").is_some(),
            "metric run requires approved external runtime pair"
        );
        let base = PathBuf::from(base).canonicalize()?;
        ensure!(base.is_dir(), "metric artifact root missing");
        let directory = base.join(case);
        fs::create_dir(&directory)?;
        let source_before = source_inventory(f)?;
        let entries = source_before["entries"]
            .as_array()
            .context("metric source inventory absent")?;
        ensure!(
            entries.len() == 12
                && entries
                    .iter()
                    .map(|entry| entry["bytes"].as_u64().unwrap_or(u64::MAX))
                    .sum::<u64>()
                    == 621,
            "source fixture differs from reviewed four-file-per-source / 621-byte shape"
        );
        let native =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Resources).await?;
        let ControlResponse::Resources { .. } = &native else {
            anyhow::bail!("baseline resource response unavailable")
        };
        // The measured reads: Project 0 and Index 0, unrelated to the split
        // Index, named per call by tokenless open MCP sessions.
        let selected = SessionScope {
            project_ids: vec![f.projects[0]],
            index_ids: Some(vec![f.indexes[0]]),
        };
        let confirmed_revision = revision(f)?;
        let (authority_baseline, authority_reads) = identity(f, confirmed_revision).await?;
        let clients = (0..4)
            .map(|_| McpSession::start_selected(f, &selected))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            service.child.try_wait()?.is_none(),
            "owned service exited before measurement"
        );
        fs::write(
            directory.join("descriptor-start.json"),
            serde_json::to_vec_pretty(&json!({
            "schema":"airk.p10.resource-descriptor/1","case":case,"owner":f.owner.path(),"control":f.control,
            "candidate":f.candidate,"service_pid":service.child.id(),"service_handle_retained_by_fixture":true,
            "source_inventory":source_before,"resources_native":native,
            "source_identity_basis":"externally selected matched native pair; consult campaign build receipt for source and profile",
            "query_selection":{"project_ids":selected.project_ids,"index_ids":selected.index_ids},
            "query_selection_basis":"tokenless open MCP reads naming the one unrelated Index; no catalog state created",
            "limits":{"clients":4,"source_shape":"existing four files per source","baseline_waves":2,"during_max_waves":30,"settled_waves":2,
                "observation_interval_ms":250,"maximum_events":4096,"maximum_entries_per_disk_sample":20000},
            "cpu_scope":"native Index spans and separately observed owned launcher/service only; total split CPU unknown"}))?,
        )?;
        let mut capture = Self {
            events: fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join("observations.jsonl"))?,
            directory,
            started: case_started,
            next_sample: Instant::now(),
            source_before,
            clients,
            selection: selected,
            confirmed_revision,
            reconfirm_required: false,
            authority_baseline,
            publication_baseline: None,
            data: vec![
                (
                    "original_data".into(),
                    PathBuf::from(&f.registrations[2].data_root),
                ),
                (
                    "unrelated_data".into(),
                    PathBuf::from(&f.registrations[0].data_root),
                ),
            ],
            sequence: 0,
            waves: 0,
            running_successes: 0,
            index: f.indexes[0],
            expected_hash: airk_cli::runtime_pair::digest_file(
                &Path::new(&f.registrations[0].root).join("api/policy.md"),
            )?,
            finished: false,
            require_running_overlap: case == "complete",
        };
        capture.event(
            json!({"phase":"initial_confirmation","native_reads":authority_reads,
            "selection":{"project_ids":capture.selection.project_ids,"index_ids":capture.selection.index_ids},
            "catalog_revision":capture.confirmed_revision}),
        )?;
        let mut baseline_successes = 0;
        for _ in 0..2 {
            baseline_successes += capture.wave("baseline", false)?;
        }
        ensure!(
            baseline_successes >= 4,
            "measurement inconclusive: baseline lacks four successful samples"
        );
        let native =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Resources).await?;
        let disk = file_bytes(f.owner.path(), &capture.data)?;
        capture
            .event(json!({"phase":"baseline","scenario":case,"resources_native":native,"disk":{"roots":disk}}))?;
        Ok(Some(capture))
    }
    fn event(&mut self, mut value: Value) -> Result<()> {
        ensure!(
            self.sequence < 4096 && self.started.elapsed() < Duration::from_secs(600),
            "metric observation budget exhausted"
        );
        value["schema"] = json!("airk.p10.resource-observation/1");
        value["sequence"] = json!(self.sequence);
        value["elapsed_us"] = json!(self.started.elapsed().as_micros());
        let bytes = serde_json::to_vec(&value)?;
        ensure!(bytes.len() <= 128 << 10, "metric event exceeds bound");
        self.events.write_all(&bytes)?;
        self.events.write_all(b"\n")?;
        self.events.flush()?;
        self.sequence += 1;
        Ok(())
    }
    pub(super) fn destination(&mut self, f: &Fixture, draft: &SplitDraft) -> Result<()> {
        let path = PathBuf::from(&draft.request.destination_registration.data_root);
        ensure!(
            path.starts_with(f.owner.path().canonicalize()?),
            "metric destination outside owner"
        );
        self.data.push(("destination_data".into(), path));
        self.event(json!({"phase":"reviewed","capacity":draft.request.capacity,"operation_id":draft.plan.operation_id}))
    }

    async fn reconfirm(&mut self, f: &Fixture, force: bool) -> Result<bool> {
        let observed = revision(f)?;
        if !force && !self.reconfirm_required && observed == self.confirmed_revision {
            return Ok(true);
        }
        let started = Instant::now();
        let selected = self.selection.clone();
        let outcome: Result<(Vec<McpSession>, Value)> = async {
            let (identity, reads) = identity(f, observed).await?;
            ensure!(
                identity == self.authority_baseline,
                "independent native Project/Index/membership authority changed; reconfirmation refused"
            );
            ensure!(
                source_inventory(f)? == self.source_before,
                "source bytes changed before reconfirmation"
            );
            let backend =
                f.resolve(&selected).await?;
            backend.close();
            backend.shutdown().await?;
            let clients = (0..4)
                .map(|_| McpSession::start_selected(f, &selected))
                .collect::<Result<Vec<_>>>()?;
            Ok((clients, reads))
        }
        .await;
        match outcome {
            Ok((clients, reads)) => {
                // Exactly the same projects/index; only observed catalog
                // revision changes, without writing any native state.
                self.clients = clients;
                self.confirmed_revision = observed;
                self.reconfirm_required = false;
                self.event(json!({"phase":"reconfirmation","outcome":"confirmed",
                    "elapsed_us_separate_from_query":started.elapsed().as_micros(),
                    "selection":{"project_ids":self.selection.project_ids,"index_ids":self.selection.index_ids},
                    "catalog_revision":observed,"native_reads":reads,"query_replayed":false}))?;
                Ok(true)
            }
            Err(error) => {
                let text = format!("{error:#}");
                ensure!(
                    text.len() <= 16384,
                    "reconfirmation error exceeds frozen bound"
                );
                self.event(json!({"phase":"reconfirmation","outcome":"refused",
                    "elapsed_us_separate_from_query":started.elapsed().as_micros(),
                    "proposed_selection":{"project_ids":selected.project_ids,"index_ids":selected.index_ids},
                    "proposed_revision":observed,"error":text,"query_replayed":false}))?;
                // Keep the old session. The next planned wave records its
                // actual refusals; failure is never a successful sample.
                Ok(false)
            }
        }
    }
    fn wave(&mut self, phase: &str, observed_running: bool) -> Result<usize> {
        let wave = self.waves;
        self.waves += 1;
        let replies = std::thread::scope(|scope| {
            let mut tasks = Vec::new();
            for (client, session) in self.clients.iter_mut().enumerate() {
                tasks.push(scope.spawn(move||->Result<(Value,u128)>{
                let started=Instant::now();
                // native read: one timed sample retains typed overload instead of replaying it.
                // U110: the session names the selected index per call: a
                // one-index open read whose body is that index's own result.
                let reply=session.request((1000+wave*4+client)as u64,"tools/call",json!({"name":"search_passages","arguments":{
                    "query":"boundaryneedle","mode":"lexical","path_prefix":"api/","limit":1,"max_bytes":16384,"verification":"off","response_mode":"full"}}))?;
                Ok((reply,started.elapsed().as_micros()))}));
            }
            tasks
                .into_iter()
                .map(|task| {
                    task.join()
                        .map_err(|_| anyhow::anyhow!("metric query thread panicked"))?
                })
                .collect::<Result<Vec<_>>>()
        })?;
        let mut successes = 0;
        for (reply, micros) in replies {
            let result = &reply["result"];
            let encoded_reply = serde_json::to_vec(&reply)?;
            let digest =
                airk_core::identity::framed_digest(b"airk/p10-metric-reply/1", &[&encoded_reply]);
            ensure!(
                encoded_reply.len() <= 65536,
                "metric native reply exceeds frozen 64KiB bound; digest:{digest};bytes:{}",
                encoded_reply.len()
            );
            self.event(json!({"phase":phase,"query_reply":{"elapsed_ms":micros as f64/1000.0,
                "attempts":1,"reply_bytes":encoded_reply.len(),"reply_digest":digest,"reply":reply}}))?;
            // Retain a completed failed attempt. Never replay it, even after a
            // later wave explicitly confirms the same independent authority.
            let outcome = if result["isError"] == true {
                let refusal = body(result)?;
                // An open read's refusal echoes its own requested open scope.
                let exact_nonreplayable = refusal["error"]["retryable"] == false
                    && refusal["error"]["query_replay_safe"] == false
                    && refusal["requested_scope"]["confirmed"] == false
                    && refusal["requested_scope"]["selection"].is_object();
                let scope_refused = exact_nonreplayable
                    && matches!(
                        refusal["error"]["code"].as_str(),
                        Some("combined_scope_unavailable" | "combined_scope_expired")
                    );
                // Canonical generic read failure is indeterminate, never proof
                // of a scope-only cause. Preserve it as a failed attempt.
                let indeterminate = exact_nonreplayable
                    && refusal["error"]["code"] == "combined_read_unavailable"
                    && refusal["error"].get("query_submitted") == Some(&Value::Null);
                if scope_refused || indeterminate {
                    self.reconfirm_required = true;
                }
                let typed =
                    scope_refused || indeterminate || read_retry::classify(result).is_some();
                self.event(json!({"phase":phase,"query":{"outcome":if scope_refused {"scope_refusal"}else if indeterminate {"indeterminate_read_failure"}else if typed {"typed_rejection"}else{"untyped_rejection"},
                    "elapsed_ms":micros as f64/1000.0,"attempts":1,"split_observed_running":false,
                    "reply_bytes":encoded_reply.len(),"reply_digest":digest,"reply":reply}}))?;
                ensure!(
                    typed,
                    "untyped metric query refusal retained at digest {digest}"
                );
                continue;
            } else {
                let member = body(result)?;
                ensure!(
                    member["read_scope"]["index_ids"] == json!([self.index])
                        && member.get("members").is_none(),
                    "metric read did not read exactly its one selected index: {reply}"
                );
                ensure!(
                    member["hits"][0]["citation"]["content_hash"] == self.expected_hash,
                    "unrelated metric citation/hash changed: {reply}"
                );
                let identity = json!({"publication":member["publication"],"generation":member["generation"],
                    "collection":member["collection"],"citation":member["hits"][0]["citation"]});
                ensure!(
                    !identity["publication"].is_null(),
                    "metric search omitted exact publication"
                );
                if let Some(expected) = &self.publication_baseline {
                    ensure!(
                        *expected == identity,
                        "unrelated metric publication/citation identity changed: {identity}"
                    );
                } else {
                    self.publication_baseline = Some(identity);
                }
                successes += 1;
                "success"
            };
            self.event(
                json!({"phase":phase,"query":{"outcome":outcome,"elapsed_ms":micros as f64/1000.0,
                "attempts":1,"split_observed_running":observed_running,
                "reply_bytes":encoded_reply.len(),"reply_digest":digest,"reply":reply}}),
            )?;
        }
        Ok(successes)
    }
    pub(super) async fn observe(
        &mut self,
        f: &Fixture,
        service: &mut OwnedProcess,
        snapshot: &SplitSnapshot,
    ) -> Result<()> {
        if Instant::now() < self.next_sample {
            return Ok(());
        }
        self.next_sample = Instant::now() + Duration::from_millis(250);
        ensure!(
            service.child.try_wait()?.is_none(),
            "owned service exited during measurement"
        );
        let started = Instant::now();
        let native =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Resources).await?;
        let disk = file_bytes(f.owner.path(), &self.data)?;
        self.event(json!({"phase":"split_active","operation_stage":snapshot.receipt.stage,"operation_state":snapshot.receipt.state,
            "finalization":snapshot.finalization,"resources_native":native,"disk":{"roots":disk},"service_pid":service.child.id(),
            "service_handle_retained_by_fixture":true,"observer_elapsed_us":started.elapsed().as_micros()}))?;
        if self.waves < 32 {
            // Confirmation belongs to the next planned wave, never a replay.
            let confirmed = self.reconfirm(f, false).await?;
            // Bound the query wave itself, excluding reconfirmation latency.
            let before = running(f, snapshot).await?;
            let successes = self.wave("during", false)?;
            let after = running(f, snapshot).await?;
            if confirmed && before && after {
                self.running_successes += successes;
                self.event(json!({"phase":"split_active","running_wave_successes":successes,
                "job":snapshot.receipt.jobs.last().map(|job|job.job),"same_job_running_at_both_bounds":true}))?;
            }
        }
        Ok(())
    }
    pub(super) async fn finish(
        mut self,
        f: &Fixture,
        service: &mut OwnedProcess,
        snapshot: &SplitSnapshot,
    ) -> Result<()> {
        let mut settled_successes = 0;
        for _ in 0..2 {
            let confirmed = self.reconfirm(f, true).await?;
            let successes = self.wave("settled", false)?;
            if confirmed {
                settled_successes += successes;
            }
        }
        ensure!(
            settled_successes >= 4,
            "measurement inconclusive: settled window lacks four successful samples"
        );
        let native =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Resources).await?;
        let disk = file_bytes(f.owner.path(), &self.data)?;
        self.event(json!({"phase":"settled","resources_native":native,"disk":{"roots":disk}}))?;
        ensure!(
            source_inventory(f)? == self.source_before,
            "source inventory changed during measurement"
        );
        ensure!(
            service.child.try_wait()?.is_none(),
            "owned service exited before end descriptor"
        );
        if !matches!(
            snapshot.receipt.state,
            SplitRunState::CanceledBeforeFence { .. }
        ) {
            use airk_core::native_reads::{NativeRead, ReportFormat};
            let mut cursor = None;
            let mut assembled = Value::Null;
            let mut complete = false;
            for _ in 0..4 {
                let started = Instant::now();
                let response = airk_cli::service_runtime::request(
                    &f.control,
                    ControlMethod::NativeRead {
                        project: snapshot.request.destination_registration.binding,
                        usage_client: airk_core::usage::ClientFamily::Unknown,
                        request: NativeRead::UsageReport {
                            days: 1,
                            cursor,
                            max_bytes: 20 << 10,
                            format: ReportFormat::Json,
                            price: None,
                        },
                    },
                )
                .await?;
                let encoded = serde_json::to_vec(&response)?;
                ensure!(
                    encoded.len() <= 65536,
                    "usage RPC observation exceeds frozen 64KiB reply bound"
                );
                let digest =
                    airk_core::identity::framed_digest(b"airk/p10-metric-usage-rpc/1", &[&encoded]);
                self.event(json!({"phase":"settled","usage_rpc_reply":response,
                    "reply_bytes":encoded.len(),"reply_digest":digest,"elapsed_us_separate_from_query":started.elapsed().as_micros(),
                    "observation_basis":"canonical service NativeRead UsageReport RPC; four pages, 20KiB per page"}))?;
                let ControlResponse::NativeRead { result } = response else {
                    anyhow::bail!("unexpected native usage RPC response retained at {digest}");
                };
                // The requested budget meets the canonical 20..48KiB contract.
                // Native errors and malformed pages/records remain failures,
                // retained above in full; none imply a missing/zero counter.
                ensure!(
                    result.get("error").is_none(),
                    "native usage RPC error retained at {digest}: {result}"
                );
                let page: airk_kernel::usage_page::Page = serde_json::from_value(result)?;
                for record in page.items {
                    airk_kernel::usage_page::assemble(&mut assembled, record)?;
                }
                cursor = page.cursor;
                if cursor.is_none() {
                    complete = true;
                    break;
                }
            }
            if complete {
                if let Some(resources) = assembled
                    .pointer("/value/report/operations/index/resources")
                    .filter(|value| value.is_object())
                {
                    self.event(json!({"phase":"settled","index_alias":"destination","index_resources":resources,
                    "cpu_coverage":"canonical service NativeRead UsageReport RPC; exclusive native Index spans only; not original removal, cleanup, actor or helper total"}))?;
                } else {
                    self.event(json!({"phase":"settled","cpu_coverage":"destination Index counter absent; unknown, not zero"}))?;
                }
            } else {
                self.event(json!({"phase":"settled","cpu_coverage":"usage report incomplete within four-page budget; unknown"}))?;
            }
        } else {
            self.event(json!({"phase":"settled","cpu_coverage":"canceled destination usage may be purged; unknown, not zero"}))?;
        }
        let overlap = self.running_successes >= 4;
        if !self.require_running_overlap {
            ensure!(
                snapshot.receipt.jobs.is_empty(),
                "frozen metadata-only cancel scenario unexpectedly owns native child work"
            );
        }
        fs::write(
            self.directory.join("descriptor-end.json"),
            serde_json::to_vec_pretty(&json!({
            "schema":"airk.p10.resource-descriptor/1","source_unchanged":true,"source_inventory":self.source_before,
            "service_pid":service.child.id(),"service_handle_retained_by_fixture":true,"operation_id":snapshot.receipt.operation_id,
            "operation_stage":snapshot.receipt.stage,"finalization":snapshot.finalization,"running_successes":self.running_successes,
            "contention_coverage":if !self.require_running_overlap{"not_applicable_metadata_only_cancel"}else if overlap{"observed"}else{"inconclusive_no_required_overlap"},
            "case_elapsed_us":self.started.elapsed().as_micros(),
            "whole_split_cpu_us":null,"physical_disk_peak_bytes":null,"native_qualification":false}))?,
        )?;
        self.finished = true;
        ensure!(
            !self.require_running_overlap || overlap,
            "measurement inconclusive: four successful queries while the same native split child remained running were not observed"
        );
        Ok(())
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.event(json!({"phase":"incomplete","native_qualification":false}));
        }
    }
}
