# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib,hashlib,json
root=Path(r'D:/code/Github/airk');out=root/'artifacts/v190/post-p14-deleted-source-r2-proposed'
p=out/'source/crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs'
s=p.read_text(encoding='utf-8')
s+='''
// Uses the real native Index job, never a forged publication or watch event.
#[cfg(feature = "embedding-worker")]
#[tokio::test]
async fn post_p14_individual_file_removal_keeps_healthy_index_and_reconciles_retained_paths() -> Result<()> {
    let mut f = Fixture::new(true)?;
    let mut service = Some(f.start().await?);
    let healthy_session = deletion_read_grant(&f, 0, 1, 151).await?;
    let affected_session = deletion_read_grant(&f, 2, 2, 152).await?;
    f.session = healthy_session.clone();
    let mut healthy = McpSession::start(&f)?;
    let before = healthy_deletion_read(&f, &mut healthy, 100).await?;
    let selected = Path::new(&f.registrations[2].root).join("api/policy.md");
    let preserved = Path::new(&f.registrations[2].root).join("impl/policy.md");
    let preserved_bytes = fs::read(&preserved)?;
    fs::remove_file(&selected)?;
    ensure!(healthy_deletion_read(&f, &mut healthy, 101).await? == before,
        "warm healthy member changed after A file removal");
    fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
    f.session = affected_session.clone();
    let mut affected = McpSession::start(&f)?;
    let args = json!({"prefix":"api/","limit":16,"max_bytes":8192,"response_mode":"full"});
    let cli = f.cli("airk_list_files", &args).await?;
    let wire = affected.call(102, "airk_list_files", args)?;
    ensure!(cli["isError"] != true && wire["result"]["isError"] != true,
        "surviving A index refused after ordinary reconciliation: {cli}; {wire}");
    policy_parity("removed-file native retained inventory", &cli, &wire["result"])?;
    ensure!(!serde_json::to_string(&body(&cli)?)?.contains("api/policy.md"),
        "reconciled file inventory still contains removed path");
    ensure!(ServiceRegistry::open_saved(&f.control)?.is_registered(f.registrations[2].binding)?,
        "a single missing file retired its whole index");
    ensure!(fs::read(&preserved)? == preserved_bytes && !selected.exists(),
        "reconciliation changed surviving source bytes or recreated removed file");
    drop(affected);
    f.session = healthy_session.clone();
    ensure!(healthy_deletion_read(&f, &mut healthy, 103).await? == before,
        "healthy member changed during A reconciliation");
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    service = Some(f.start().await?);
    let mut healthy = McpSession::start(&f)?;
    ensure!(healthy_deletion_read(&f, &mut healthy, 104).await? == before,
        "healthy member changed after cold restart");
    f.session = affected_session;
    let retirement = policy_draft(&f, 153, SourceMutationSelection::RetireIndex).await?;
    apply_policy(&f, &retirement).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if ServiceRegistry::open_saved(&f.control)?.project_was_removed(f.registrations[2].binding)? { break; }
        ensure!(Instant::now() < deadline, "explicit A retirement did not settle");
        f.session = healthy_session.clone();
        ensure!(healthy_deletion_read(&f, &mut healthy, 105).await? == before,
            "healthy member changed during explicit A retirement");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ensure!(fs::read(&preserved)? == preserved_bytes, "retirement removed surviving source");
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    Ok(())
}
'''
p.write_text(s,encoding='utf-8',newline='\n')
# Validate absence cannot be used to retire an operation with live paid split state.
p=out/'source/crates/airk-store/src/service_project_removal.rs';s=p.read_text(encoding='utf-8')
needle='''        let registration = self.retained_project(project)?.registration;
''';assert needle in s
s=s.replace(needle,'''        ensure!(self.catalog_active_split_for_binding(project)?.is_none(),
            "active split recovery owns this retired participant; finish exact split recovery first");
'''+needle,1);p.write_text(s,encoding='utf-8',newline='\n')
# Add source review guard before native fence effects; apply re-reviews the same draft.
rel='crates/airk-cli/src/catalog_source_change_control.rs';raw=(root/rel).read_bytes();s=raw.decode('utf-8')
p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
needle='''    if selection.change == SourceMutationSelection::RetireIndex {
''';assert needle in s
s=s.replace(needle,needle+'''        ensure!(registry.catalog_active_split_for_binding(index.backing.binding)?.is_none(),
            "active split owns this participant; cancel or finish its exact recovery before retirement");
''',1)
p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,encoding='utf-8',newline='\n')
patch=[];manifest=[]
for p in sorted((out/'source').rglob('*.rs')):
    rel=p.relative_to(out/'source').as_posix();base=out/'base'/rel
    old=base.read_text(encoding='utf-8') if base.exists() else '';new=p.read_text(encoding='utf-8')
    patch.append('diff --git a/'+rel+' b/'+rel+'\n')
    if not base.exists():patch.append('new file mode 100644\n')
    patch.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+rel if base.exists() else '/dev/null',tofile='b/'+rel))
    manifest.append({'path':rel,'base_sha256':hashlib.sha256(base.read_bytes()).hexdigest() if base.exists() else None,'proposed_sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
(out/'missing-data.patch').write_text(''.join(patch),encoding='utf-8',newline='\n')
(out/'base-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(hashlib.sha256((out/'missing-data.patch').read_bytes()).hexdigest())
