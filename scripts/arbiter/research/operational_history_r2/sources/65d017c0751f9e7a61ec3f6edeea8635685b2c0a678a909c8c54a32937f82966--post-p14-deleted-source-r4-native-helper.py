# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
out=Path(r'D:/code/Github/airk/artifacts/v190/post-p14-deleted-source-r4-proposed')
p=out/'source/crates/airk-store/src/quota/funding_retired.rs';s=p.read_text(encoding='utf-8');s=s.replace('format!("funding-retire-{}-{}-{}",\n                record.request.operation_id, record.revision, registration.binding)','format!("funding-retire-{}-{}",\n                record.request.operation_id, record.revision)')
start=s.index('                if let Ok(prior) =');end=s.index('\n            }\n            rows.push',start)
s=s[:start]+'''                match directory.read_file(&scratch, MAX_RECORD_BYTES) {
                    Ok(prior) => ensure!(bytes.starts_with(&prior), "retired Current scratch belongs to another write"),
                    Err(error) if absent(&error) => (),
                    Err(error) => return Err(error),
                }'''+s[end:];p.write_text(s,encoding='utf-8',newline='\n')
# Expose only this test module to a sibling native integration fixture.
p=out/'source/crates/airk-cli/tests/combined_reads/permanent_trim.rs';s=p.read_text(encoding='utf-8').replace('mod deleted_source_recovery;','pub(super) mod deleted_source_recovery;',1);p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs';s=p.read_text(encoding='utf-8');s=s.replace('''    ensure!(
        members.len() == 1 && members[0]["index_id"] == f.indexes[1].to_string(),''','''    let expected = f.session.index_ids.as_ref().filter(|ids| ids.len() == 1)
        .context("healthy grant must select exactly one explicit index")?[0];
    ensure!(
        members.len() == 1 && members[0]["index_id"] == expected.to_string(),''',1)
s+='''
// Called ONLY after a real shared-root provider cutover and native two-participant
// finalization/ack. No complete/release/current_owned bit is constructed by tests.
pub(crate) async fn retire_completed_destination(
    f: &mut Fixture, service: &mut Option<OwnedProcess>,
    complete: &airk_core::catalog_split::SplitSnapshot,
) -> Result<()> {
    use airk_store::quota::funding::FundingChildKind;
    let destination = complete.request.destination_registration.clone();
    ensure!(destination.data_root == complete.request.original_registration.data_root,
        "terminal Current preservation regression requires a genuinely shared root");
    let quota = airk_store::quota::Quota::open_service(Path::new(&destination.data_root))?;
    let operation = complete.receipt.operation_id;
    let before = quota.observe_funding(operation)?;
    ensure!(before.complete && before.release_acknowledged,
        "actual native cutover did not acknowledge ordinary Current ownership");
    let selected: Vec<_> = before.children.iter().filter(|child| child.index_id == complete.request.destination.index_id).cloned().collect();
    let original: Vec<_> = before.children.iter().filter(|child| child.index_id == complete.request.original_index).cloned().collect();
    ensure!(!selected.is_empty() && !original.is_empty()
        && selected.iter().chain(&original).all(|child| child.current_owned && matches!(child.kind, FundingChildKind::Current { .. })),
        "real native terminal WAL children missing; no fabricated positive fixture");
    let healthy = deletion_read_grant(f, 2, 2, 181).await?;
    f.session = healthy;
    let mut mcp = McpSession::start(f)?;
    let token = healthy_deletion_read(f, &mut mcp, 120).await?;
    drop(mcp);
    stop_deletion_service(f, service).await?;
    let logical = Path::new(&destination.data_root).join("collections-1.4").join(&destination.collection);
    let owner = f.owner.path().canonicalize()?;
    ensure!(logical.starts_with(&owner) && logical != Path::new(&destination.data_root)
        && !f.control.starts_with(&logical), "terminal selected deletion escaped fixture");
    fs::remove_dir_all(&logical)?;
    *service = Some(f.start().await?);
    let mut mcp = McpSession::start(f)?;
    ensure!(healthy_deletion_read(f, &mut mcp, 121).await? == token,
        "surviving original changed after terminal destination loss");
    drop(mcp);
    let response = airk_cli::service_runtime::request(&f.control,
        ControlMethod::CatalogSourceMutationDraft { selection: SourceMutationDraftSelection {
            key: policy_key(f, 182).await?, expected_catalog_revision: policy_revision(f)?,
            index_id: complete.request.destination.index_id, change: SourceMutationSelection::RetireIndex,
        }}).await?;
    let ControlResponse::CatalogSourceMutationDraft { draft } = response else {
        anyhow::bail!("completed split destination retirement review refused: {}", serde_json::to_string(&response)?);
    };
    apply_policy(f, &draft).await?;
    // This is an explicit new grant review after restrictive native retirement;
    // never silently renew the old Project authority or relax its errors.
    f.session = deletion_read_grant(f, 2, 2, 183).await?;
    let mut mcp = McpSession::start(f)?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if ServiceRegistry::open_saved(&f.control)?.project_was_removed(destination.binding)? { break; }
        ensure!(Instant::now() < deadline, "acknowledged Current-owned destination cleanup did not settle");
        ensure!(healthy_deletion_read(f, &mut mcp, 122).await? == token,
            "terminal A disposal changed surviving B publication");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let after = quota.observe_funding(operation)?;
    ensure!(after.complete && after.release_acknowledged
        && after.release == before.release && after.request == before.request
        && !after.children.iter().any(|child| child.index_id == complete.request.destination.index_id)
        && after.children == original,
        "terminal disposal changed immutable funding lineage or surviving original Current credit");
    let saved = ServiceRegistry::open_saved(&f.control)?
        .catalog_split_status(operation, None)?.context("completed split receipt missing")?;
    ensure!(saved == *complete, "ordinary Current disposal changed the exact completed native receipt");
    ensure!(!logical.exists(), "terminal disposal recreated the missing destination");
    f.assert_sources_unchanged()?;
    drop(mcp);
    stop_deletion_service(f, service).await?;
    *service = Some(f.start().await?);
    let mut mcp = McpSession::start(f)?;
    ensure!(healthy_deletion_read(f, &mut mcp, 123).await? == token,
        "surviving original failed after retired terminal destination restart");
    let worker = airk_cli::service_runtime::request(&f.control, ControlMethod::Worker { project: destination.binding }).await?;
    ensure!(matches!(worker, ControlResponse::Worker { identity: None, .. }), "retired terminal destination worker restarted");
    drop(mcp);
    stop_deletion_service(f, service).await?;
    Ok(())
}
'''
# Use actual enum spellings verified from owning permanent policy fixture.
s=s.replace('ControlResponse::CatalogSourceMutationDraft { draft }','ControlResponse::CatalogSourceMutationDraft { draft }')
p.write_text(s,encoding='utf-8',newline='\n')
print('r4 native terminal helper prepared; transport names will be live-verified before freezing')
