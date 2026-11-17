# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib, hashlib, json
base=Path(__file__).resolve().parent
tree=base/'p10-worktree'
metric_path=tree/'crates/airk-cli/tests/combined_reads/split_metrics.rs'
fixture_path=tree/'crates/airk-cli/tests/combined_reads.rs'
original=metric_path.read_text(encoding='utf-8')
s=original
def change(old,new):
    global s
    assert s.count(old)==1,(old[:80],s.count(old))
    s=s.replace(old,new)
change('    clients: Vec<McpSession>,','''    clients: Vec<McpSession>,
    selection: CombinedConnectRequest,
    authority_baseline: Value,
    publication_baseline: Option<Value>,''')
change('''        f.session = selected;
        let clients = (0..4)
            .map(|_| McpSession::start(f))
            .collect::<Result<Vec<_>>>();
        f.session = original;
        let clients = clients?;''','''        let (authority_baseline, authority_reads) = identity(f, &selected).await?;
        let clients = (0..4)
            .map(|_| McpSession::start_selected(f, &selected))
            .collect::<Result<Vec<_>>>()?;
        f.session = original;''')
change('''            clients,
            data:''','''            clients,
            selection: selected,
            authority_baseline,
            publication_baseline: None,
            data:''')
change('''        let mut baseline_successes = 0;''','''        capture.event(json!({"phase":"initial_confirmation","native_reads":authority_reads,
            "selection":capture.selection}))?;
        let mut baseline_successes = 0;''')
change('''            let outcome = if result["isError"] == true {
                ensure!(
                    read_retry::classify(result).is_some(),
                    "untyped metric query refusal: {reply}"
                );
                "typed_rejection"
            } else {''','''            let encoded_reply = serde_json::to_vec(&reply)?;
            let digest = airk_core::identity::framed_digest(b"airk/p10-metric-reply/1", &[&encoded_reply]);
            ensure!(encoded_reply.len() <= 65536, "metric native reply exceeds frozen 64KiB bound; digest:{digest};bytes:{}", encoded_reply.len());
            // Retain a completed failed attempt. Never replay it, even after a
            // later wave explicitly confirms the same independent authority.
            let outcome = if result["isError"] == true {
                let refusal = body(result)?;
                let scope_refused = matches!(refusal["error"]["code"].as_str(),
                    Some("combined_scope_unavailable" | "combined_scope_expired"))
                    && refusal["error"]["retryable"] == false
                    && refusal["error"]["query_replay_safe"] == false
                    && refusal["requested_scope"]["confirmed"] == false
                    && refusal["requested_scope"]["selection"] == serde_json::to_value(&self.selection)?;
                let typed = scope_refused || read_retry::classify(result).is_some();
                self.event(json!({"phase":phase,"query":{"outcome":if scope_refused {"scope_refusal"}else if typed {"typed_rejection"}else{"untyped_rejection"},
                    "elapsed_ms":micros as f64/1000.0,"attempts":1,"split_observed_running":false,
                    "reply_bytes":encoded_reply.len(),"reply_digest":digest,"reply":reply}}))?;
                ensure!(typed, "untyped metric query refusal retained at digest {digest}");
                continue;
            } else {''')
change('''                successes += 1;
                "success"''','''                let identity = json!({"publication":member["publication"],"generation":member["generation"],
                    "collection":member["collection"],"citation":member["hits"][0]["citation"]});
                ensure!(!identity["publication"].is_null(), "metric search omitted exact publication");
                if let Some(expected) = &self.publication_baseline {
                    ensure!(*expected == identity, "unrelated metric publication/citation identity changed: {identity}");
                } else {
                    self.publication_baseline = Some(identity);
                }
                successes += 1;
                "success"''')
change('''                "attempts":1,"split_observed_running":observed_running}}),''','''                "attempts":1,"split_observed_running":observed_running,
                "reply_bytes":encoded_reply.len(),"reply_digest":digest,"reply":reply}}),''')
change('''            // Count coverage only if this exact native child is Running at both bounds.
            let successes = self.wave("during", false)?;''','''            // Confirmation belongs to the next planned wave, never a replay.
            self.reconfirm(f, false).await?;
            // Bound the query wave itself, excluding reconfirmation latency.
            let before = running(f, snapshot).await?;
            let successes = self.wave("during", false)?;''')
# Avoid the now-unused earlier bound; the post-confirmation bound is authoritative.
change('''        let before = running(f, snapshot).await?;
        let native =''','''        let native =''')
change('''        for _ in 0..2 {
            settled_successes += self.wave("settled", false)?;
        }''','''        for _ in 0..2 {
            self.reconfirm(f, true).await?;
            settled_successes += self.wave("settled", false)?;
        }''')
change('''            "whole_split_cpu_us":null,"physical_disk_peak_bytes":null,"native_qualification":false}))?,''','''            "case_elapsed_us":self.started.elapsed().as_micros(),
            "whole_split_cpu_us":null,"physical_disk_peak_bytes":null,"native_qualification":false}))?,''')
insert='''
    async fn reconfirm(&mut self, f: &Fixture, force: bool) -> Result<()> {
        let observed = revision(f)?;
        if !force && observed == self.selection.expected_catalog_revision {
            return Ok(());
        }
        let started = Instant::now();
        let mut selected = self.selection.clone();
        selected.expected_catalog_revision = observed;
        let outcome: Result<(Vec<McpSession>, Value)> = async {
            let (identity, reads) = identity(f, &selected).await?;
            ensure!(identity == self.authority_baseline,
                "independent native grant/Project/Index authority changed; reconfirmation refused");
            ensure!(source_inventory(f)? == self.source_before, "source bytes changed before reconfirmation");
            let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &selected).await?;
            backend.close();
            backend.shutdown().await?;
            let clients = (0..4).map(|_| McpSession::start_selected(f, &selected))
                .collect::<Result<Vec<_>>>()?;
            Ok((clients, reads))
        }.await;
        match outcome {
            Ok((clients, reads)) => {
                // Exactly the same grant/key/projects/index; only observed
                // catalog revision changes, without writing any native state.
                self.clients = clients;
                self.selection = selected;
                self.event(json!({"phase":"reconfirmation","outcome":"confirmed",
                    "elapsed_us_separate_from_query":started.elapsed().as_micros(),
                    "selection":self.selection,"native_reads":reads,"query_replayed":false}))?;
            }
            Err(error) => {
                let text = format!("{error:#}");
                ensure!(text.len() <= 16384, "reconfirmation error exceeds frozen bound");
                self.event(json!({"phase":"reconfirmation","outcome":"refused",
                    "elapsed_us_separate_from_query":started.elapsed().as_micros(),
                    "proposed_selection":selected,"error":text,"query_replayed":false}))?;
                // Keep the old session. The next planned wave records its
                // actual refusals; failure is never a successful sample.
            }
        }
        Ok(())
    }
'''
change('    fn wave(&mut self, phase: &str, observed_running: bool) -> Result<usize> {',insert+'    fn wave(&mut self, phase: &str, observed_running: bool) -> Result<usize> {')
identity='''
// Compare complete native authority items, excluding only enclosing observation
// revision. A changed/revoked grant, membership or source definition refuses.
async fn identity(f: &Fixture, selected: &CombinedConnectRequest) -> Result<(Value, Value)> {
    use airk_core::catalog_management::CatalogEntity;
    let entities = [CatalogEntity::Grant { id:selected.grant_id },
        CatalogEntity::Project { id:f.projects[0] }, CatalogEntity::Index { id:f.indexes[0] },
        CatalogEntity::Membership { project_id:f.projects[0], index_id:f.indexes[0] }];
    let mut items = Vec::new();
    let mut reads = Vec::new();
    for entity in entities {
        let response = airk_cli::service_runtime::request(&f.control, ControlMethod::CatalogDetail {
            entity, expected_catalog_revision:Some(selected.expected_catalog_revision)
        }).await?;
        let ControlResponse::CatalogDetail { detail, .. } = &response else {
            anyhow::bail!("native independent identity observation refused: {}",serde_json::to_string(&response)?);
        };
        ensure!(detail.namespace == selected.namespace && detail.revision == selected.expected_catalog_revision
            && detail.item.is_some(), "native independent identity absent or stale");
        items.push(serde_json::to_value(&detail.item)?);
        reads.push(serde_json::to_value(&response)?);
    }
    Ok((json!(items),json!(reads)))
}
'''
change('impl Capture {',identity+'impl Capture {')
(base/'p10-resource-reconfirmation.proposed.rs').write_text(s,encoding='utf-8',newline='\n')
fixture=fixture_path.read_text(encoding='utf-8')
needle='''    fn start(f: &Fixture) -> Result<Self> {
        let mut command = Command::new(&f.candidate);'''
assert fixture.count(needle)==1
fixture_after=fixture.replace(needle,'''    fn start(f: &Fixture) -> Result<Self> {
        Self::start_selected(f, &f.session)
    }
    fn start_selected(f: &Fixture, selected: &CombinedConnectRequest) -> Result<Self> {
        let mut command = Command::new(&f.candidate);''')
needle='.arg(serde_json::to_string(&f.session)?)'
assert fixture_after.count(needle)==1
fixture_after=fixture_after.replace(needle,'.arg(serde_json::to_string(selected)?)')
(base/'p10-resource-fixture-reconfirmation.proposed.rs').write_text(fixture_after,encoding='utf-8',newline='\n')
print(json.dumps({'metrics_base':hashlib.sha256(metric_path.read_bytes()).hexdigest(),'fixture_base':hashlib.sha256(fixture_path.read_bytes()).hexdigest()}))
