# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
root=Path(r'D:/code/Github/airk');out=root/'artifacts/v190/post-p14-deleted-source-r4-proposed'
rel='crates/airk-cli/tests/combined_reads/split.rs';raw=(root/rel).read_bytes();p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw);s=raw.decode('utf-8')
needle='''async fn draft(f: &Fixture, nonce: u8) -> Result<SplitDraft> {''';assert needle in s;s=s.replace(needle,needle+'''
    draft_at_root(f, nonce, false).await
}
async fn draft_at_root(f: &Fixture, nonce: u8, shared: bool) -> Result<SplitDraft> {''',1)
old='''        destination_data_root: f
            .owner
            .path()
            .join(format!("split-data-{nonce}"))
            .to_str()
            .context("UTF8 destination")?
            .into(),''';assert old in s;s=s.replace(old,'''        destination_data_root: if shared { f.registrations[2].data_root.clone() } else {
            f.owner.path().join(format!("split-data-{nonce}")).to_str()
                .context("UTF8 destination")?.into()
        },''',1)
old='''        !Path::new(&draft.request.destination_registration.data_root).exists(),
        "read-only review allocated destination"''';assert old in s;s=s.replace(old,'''        Path::new(&draft.request.destination_registration.data_root).exists() == shared
            && !Path::new(&draft.request.destination_registration.data_root)
                .join("collections-1.4").join(&draft.request.destination_registration.collection).exists(),
        "read-only review allocated destination or changed root presence"''',1)
needle='''async fn split_actual_service_provider_cutover_retains_complete_rpc_cli_mcp_evidence() -> Result<()>
{''';assert needle in s;s=s.replace(needle,needle+'''
    provider_cutover_with_retirement(false).await
}
#[cfg(feature = "embedding-worker")]
#[tokio::test]
async fn post_p14_completed_shared_root_current_owned_destination_can_retire_without_changing_original() -> Result<()> {
    provider_cutover_with_retirement(true).await
}
#[cfg(feature = "embedding-worker")]
async fn provider_cutover_with_retirement(shared: bool) -> Result<()> {''',1)
s=s.replace('''    let mut capture = metrics::Capture::start(&mut f, &mut _service, "complete").await?;''','''    // New lifecycle scenario has no historical complete-workload cost claim.
    let mut capture = if shared { None } else {
        metrics::Capture::start(&mut f, &mut _service, "complete").await?
    };''',1)
s=s.replace('''    let reviewed = draft(&f, 131).await?;''','''    let reviewed = draft_at_root(&f, 131, shared).await?;''',1)
s=s.replace('''        complete.receipt.funding_release_receipts.len() == 2''','''        complete.receipt.funding_release_receipts.len() == if shared { 1 } else { 2 }''',1)
s=s.replace('''                == 2,
        "actual native roots were not acknowledged"''','''                == if shared { 1 } else { 2 },
        "actual native roots were not acknowledged"''',1)
needle='''    if let Some(observer) = capture.take() {
        observer.finish(&f, &mut _service, &complete).await?;
    }
    Ok(())
}''';assert needle in s
s=s.replace(needle,'''    if let Some(observer) = capture.take() {
        observer.finish(&f, &mut _service, &complete).await?;
    }
    if shared {
        let mut service = Some(_service);
        super::permanent_trim::deleted_source_recovery::retire_completed_destination(
            &mut f, &mut service, &complete,
        ).await?;
    }
    Ok(())
}''',1)
p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs';s=p.read_text(encoding='utf-8').replace('.catalog_split_status(operation, None)?','.catalog_split_status(operation)?');p.write_text(s,encoding='utf-8',newline='\n')
print('actual real completed shared-root provider fixture wired; no run')
