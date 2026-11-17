# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
out=Path(r'D:/code/Github/airk/artifacts/v190/post-p14-deleted-source-r3-proposed')
p=out/'source/crates/airk-cli/tests/combined_reads.rs';s=p.read_text(encoding='utf-8')
start=s.index('    fn with_runtime_layout(');brace=s.index('    ) -> Result<Self> {',start)+len('    ) -> Result<Self> {')
s=s[:brace]+'''
        Self::with_runtime_topology(dedicated_is_indexed, count, helper, removed_last_parent, false)
    }
    fn with_shared_runtime() -> Result<Self> {
        Self::with_runtime_topology(true, 3, false, None, true)
    }
    fn with_runtime_topology(
        dedicated_is_indexed: bool, count: usize, helper: bool,
        removed_last_parent: Option<bool>, shared: bool,
    ) -> Result<Self> {'''+s[brace:]
needle='''            let data = if nested && removed_last_parent == Some(true) {''';assert needle in s
s=s.replace(needle,'''            let data = if shared {
                base.join("shared-data")
            } else if nested && removed_last_parent == Some(true) {''',1)
needle='''            fs::create_dir(&data)?;''';assert needle in s;s=s.replace(needle,'''            if shared { fs::create_dir_all(&data)?; } else { fs::create_dir(&data)?; }''',1)
p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs';s=p.read_text(encoding='utf-8')
s+='''
#[tokio::test]
async fn post_p14_missing_collection_settles_ordinary_credit_without_poisoning_healthy_shared_root() -> Result<()> {
    let mut f = Fixture::with_shared_runtime()?;
    let mut service = Some(f.start().await?);
    let healthy_session = deletion_read_grant(&f, 0, 1, 161).await?;
    f.session = healthy_session.clone();
    let mut healthy = McpSession::start(&f)?;
    let before = healthy_deletion_read(&f, &mut healthy, 110).await?;
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    let registration = f.registrations[2].clone();
    let data = Path::new(&registration.data_root);
    ensure!(data == Path::new(&f.registrations[1].data_root), "fixture did not share quota root");
    let grants = data.join(".quota-1.4/current-grants");
    let inventory = |collection: &str| -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut rows = Vec::new();
        for entry in fs::read_dir(&grants)? {
            let path = entry?.path();
            let bytes = fs::read(&path)?;
            let body: Value = serde_json::from_slice(&bytes)?;
            if body["scope"]["collection"] == collection { rows.push((path, bytes)); }
        }
        rows.sort_by(|a,b| a.0.cmp(&b.0)); Ok(rows)
    };
    let a_rows = inventory(&registration.collection)?;
    ensure!(!a_rows.is_empty(), "fixture did not leave real native durable Current allowance; do not claim grant recovery without a paid native fixture");
    let b_rows = inventory(&f.registrations[1].collection)?;
    let quota = airk_store::quota::Quota::open_service(data)?;
    let previous_policy = quota.policy()?;
    let capacity = quota.status()?;
    let mut constrained = previous_policy.clone();
    constrained.root_bytes = capacity.used_bytes + capacity.reserved_bytes + 128;
    quota.configure_if_current(constrained, Some(&previous_policy))?;
    ensure!(quota.reserve(&f.registrations[1].collection, 4096).is_err(),
        "ordinary writer consumed unavailable prepaid allowance");
    let logical = data.join("collections-1.4").join(&registration.collection);
    let owner = f.owner.path().canonicalize()?;
    ensure!(logical.starts_with(&owner) && logical != data && !f.control.starts_with(&logical),
        "selected collection deletion escaped fixture");
    fs::remove_dir_all(&logical)?;
    service = Some(f.start().await?);
    let mut healthy = McpSession::start(&f)?;
    ensure!(healthy_deletion_read(&f, &mut healthy, 111).await? == before,
        "cold shared-root healthy member changed after A storage loss");
    let retirement = policy_draft(&f, 162, SourceMutationSelection::RetireIndex).await?;
    apply_policy(&f, &retirement).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let registry = ServiceRegistry::open_saved(&f.control)?;
        if registry.project_was_removed(registration.binding)? {
            ensure!(!registry.pending_project_removals(None, 32)?.iter().any(|p| p.binding == registration.binding),
                "settled shared-root removal remained selected for cleanup");
            break;
        }
        ensure!(Instant::now() < deadline, "shared-root credit settlement did not finish");
        drop(registry);
        ensure!(healthy_deletion_read(&f, &mut healthy, 112).await? == before,
            "healthy B failed while exact A credit settled");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ensure!(inventory(&registration.collection)?.is_empty(), "disposed A durable grants leaked");
    ensure!(inventory(&f.registrations[1].collection)? == b_rows, "A cleanup changed B grant identity/bytes");
    let paid_b = quota.reserve(&f.registrations[1].collection, 4096)?;
    paid_b.charge(4096)?;
    drop(paid_b);
    ensure!(!logical.exists(), "cleanup recreated missing logical collection");
    f.assert_sources_unchanged()?;
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    service = Some(f.start().await?);
    f.session = healthy_session;
    let mut healthy = McpSession::start(&f)?;
    ensure!(healthy_deletion_read(&f, &mut healthy, 113).await? == before,
        "healthy B failed after settled A restart");
    let worker = airk_cli::service_runtime::request(&f.control,
        ControlMethod::Worker { project: registration.binding }).await?;
    ensure!(matches!(worker, ControlResponse::Worker { identity: None, .. }),
        "retired A evidence worker restarted");
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    Ok(())
}
'''
p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-store/src/quota/retired_tests.rs';s=p.read_text(encoding='utf-8');s+='''
#[test]
fn held_existing_collection_advisory_refuses_disposed_current_grant_release() -> Result<()> {
    let f = Fixture::new()?;
    let original = f.grants(&f.registration.collection)?;
    let healthy = f.grants(&f.healthy.collection)?;
    let old_writer = crate::service::acquire_existing_file(
        &f.quota.root.join("collections-1.4/.writer-locks"), &f.registration.collection)?;
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    assert!(AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace).is_err());
    assert_eq!(f.grants(&f.registration.collection)?, original);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(old_writer);
    drop(AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)?);
    assert!(f.grants(&f.registration.collection)?.is_empty());
    f.assert_sources()?; Ok(())
}
''';p.write_text(s,encoding='utf-8',newline='\n')
print('native shared-root and actual advisory negative copies prepared')
