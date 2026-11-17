# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root=Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out=Path('D:/code/Github/airk/artifacts/v190')
path='crates/airk-cli/tests/combined_reads/split.rs'
old=(root/path).read_text(encoding='utf-8')
new=old
helper='''// Bounded read-only native queue/settings evidence, captured before fixture Drop.
// This does not replay a job, recover a journal, or inspect source document bytes.
fn stalled_native_diagnostics(f: &Fixture, snapshot: &SplitSnapshot) -> String {
    let observe = || -> Result<Value> {
        let registry = ServiceRegistry::open_saved(&f.control)?;
        let sql = Connection::open_with_flags(f.control.join("service-1.4.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let native = registry.catalog_split_state(snapshot.receipt.operation_id)?;
        let mut participants = Vec::new();
        for registration in [&snapshot.request.original_registration, &snapshot.request.destination_registration] {
            let mut statement = sql.prepare("SELECT id FROM service_jobs WHERE project=?1 AND state IN('pending','running') ORDER BY seq LIMIT 9")?;
            let ids = statement.query_map([registration.binding.to_string()], |row| row.get::<_,String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut jobs = Vec::new();
            for id in ids.iter().take(8) {
                let id: Sha256Digest = id.parse()?;
                let work = registry.configuration_work(id)?;
                jobs.push(json!({"job":registry.job(id)?,
                    "configuration_operation":work.as_ref().map(|work| serde_json::to_value(&work.request.configuration)).transpose()?
                        .and_then(|value| value.get("operation").cloned()),
                    "runs_while_paused":registry.job_runs_while_paused(id)?}));
            }
            participants.push(json!({"binding":registration.binding,"jobs":jobs,"truncated":ids.len()>8,
                "registered":registry.get(registration.binding).map(|row| json!({"paused":row.paused})).unwrap_or_else(|error|json!({"error":format!("{error:#}")})),
                "pending_gate":registry.pending_job_gate(registration.binding).map(|value|json!(value)).unwrap_or_else(|error|json!({"error":format!("{error:#}")}))}));
        }
        let witness_preparation = if snapshot.receipt.stage == SplitStage::DestinationValidated
            && !snapshot.receipt.jobs.iter().any(|job| job.purpose == airk_core::catalog_split::SplitJobPurpose::OriginalWitness) {
            let registration = &snapshot.request.original_registration;
            let prepared = airk_store::service::collection::Collection::existing(Path::new(&registration.data_root),
                &registration.root,&registration.collection)
                .and_then(|collection| collection.split_work_configuration(&native,
                    airk_core::catalog_split::SplitJobPurpose::OriginalWitness));
            match prepared {
                Ok(work)=>json!({"metadata_preparation":"available","incarnation":work.incarnation,"settings":work.settings}),
                Err(error)=>json!({"metadata_preparation":"refused","error":format!("{error:#}")}),
            }
        } else { Value::Null };
        Ok(json!({"operation":snapshot.receipt.operation_id,"stage":snapshot.receipt.stage,
            "receipt_digest":snapshot.receipt_digest,"participants":participants,"witness_preparation":witness_preparation}))
    };
    let metadata = observe().unwrap_or_else(|error|json!({"diagnostic_error":format!("{error:#}")}));
    let log = (|| -> Result<String> {
        use std::io::{Read,Seek,SeekFrom};
        let mut file=fs::File::open(f.owner.path().join("service.log"))?;
        let bytes=file.metadata()?.len().min(65536);
        file.seek(SeekFrom::End(-(bytes as i64)))?;
        let mut tail=Vec::with_capacity(bytes as usize);
        file.take(bytes).read_to_end(&mut tail)?;
        Ok(String::from_utf8_lossy(&tail).into_owned())
    })().unwrap_or_else(|error|format!("service log unavailable: {error:#}"));
    json!({"native":metadata,"service_log_tail":log}).to_string()
}

'''
marker='async fn terminal(f: &Fixture, operation: Incarnation, canceled: bool) -> Result<SplitSnapshot> {'
assert new.count(marker)==1
new=new.replace(marker,helper+marker)
old_message='''            "native split deadline exceeded: {}",
            serde_json::to_string(&snapshot)?
'''
assert new.count(old_message)==1
new=new.replace(old_message,'''            "native split deadline exceeded: {}; bounded native diagnostics:{}",
            serde_json::to_string(&snapshot)?, stalled_native_diagnostics(f,&snapshot)
''')
start=new.index('        if snapshot.receipt.stage == SplitStage::Building {',new.index('async fn split_actual_service_cancel'))
end=new.index('        tokio::time::sleep(Duration::from_millis(25)).await;',start)
new=new[:start]+'''        // Shared protected setup also owns this exact ProjectLease. Thus the
        // truthful stable cancellation milestone is Accepted after reservation,
        // before any physical destination, funding or worker job exists.
        if snapshot.receipt.stage == SplitStage::Accepted && snapshot.receipt.revision.get() > 1 {
            ensure!(snapshot.receipt.destination_storage.is_none() && snapshot.receipt.capacity_receipt.is_none()
                && snapshot.receipt.funding_receipts.is_empty() && snapshot.receipt.jobs.is_empty(),
                "held setup owner did not preserve metadata-only cancellation boundary");
            let registry=ServiceRegistry::open_saved(&f.control)?;
            let detail=registry.catalog_detail(&airk_core::catalog_management::CatalogEntity::Index {
                id:snapshot.request.destination.index_id},None)?;
            ensure!(detail.item.is_some(),"destination reservation not retained");
            break snapshot;
        }
        ensure!(snapshot.receipt.stage == SplitStage::Accepted && Instant::now()<deadline,
            "real held-owner Accepted reservation not reached: {}; bounded native diagnostics:{}",
            serde_json::to_string(&snapshot)?,stalled_native_diagnostics(&f,&snapshot));
''' +new[end:]
new=new.replace('''    // Metadata setup/funding may proceed, but no build can cross its real owner.
''','''    // Shared protected setup needs the same real owner, so it remains reserved
    // metadata until this test releases its lease after exact cancellation ACK.
''')
marker='''    fixture_job(&f, 2, airk_core::service::JobKind::Vectors).await?;
    fixture_pause(&f, 2, true).await?;
'''
assert new.count(marker)==1
new=new.replace(marker,'''    fixture_job(&f, 2, airk_core::service::JobKind::Vectors).await?;
    let before_pause = airk_cli::service_runtime::request(&f.control,
        ControlMethod::Progress { project: f.registrations[2].binding }).await?;
    eprintln!("native split original before pause: {}",serde_json::to_string(&before_pause)?);
    fixture_pause(&f, 2, true).await?;
    let after_pause = airk_cli::service_runtime::request(&f.control,
        ControlMethod::Progress { project: f.registrations[2].binding }).await?;
    eprintln!("native split original after pause: {}",serde_json::to_string(&after_pause)?);
    ensure!(matches!(&after_pause, ControlResponse::Progress {
        queued_jobs, pending_job:None, project_paused:Some(true),queue_truncated:Some(false),..
    } if queued_jobs.is_empty()),
        "paused split fixture is not quiescent; no split admitted; before:{};after:{};service:{}",
        serde_json::to_string(&before_pause)?,serde_json::to_string(&after_pause)?,
        fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default());
''')
# Actual native Progress (bounded sixteen-job observation) is also retained at
# timeout, before process/fixture Drop removes the isolated ownership evidence.
marker='''        ensure!(
            Instant::now() < deadline,
            "native split deadline exceeded: {}; bounded native diagnostics:{}",
            serde_json::to_string(&snapshot)?, stalled_native_diagnostics(f,&snapshot)
        );'''
assert new.count(marker)==1
new=new.replace(marker,'''        if Instant::now() >= deadline {
            let mut progress=Vec::new();
            for registration in [&snapshot.request.original_registration,&snapshot.request.destination_registration] {
                progress.push(match airk_cli::service_runtime::request(&f.control,
                    ControlMethod::Progress { project:registration.binding }).await {
                    Ok(value)=>serde_json::to_value(value)?,
                    Err(error)=>json!({"error":format!("{error:#}")}),
                });
            }
            anyhow::bail!("native split deadline exceeded: {};progress:{};bounded native diagnostics:{}",
                serde_json::to_string(&snapshot)?,json!(progress),stalled_native_diagnostics(f,&snapshot));
        }''')
patch=''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+path,tofile='b/'+path))
(out/'p10-process-diagnostics.proposed.patch').write_text(patch,encoding='utf-8',newline='\n')
(out/'p10-process-diagnostics.proposed.rs').write_text(new,encoding='utf-8',newline='\n')
print('Created inert cancellation/diagnostics correction; no compiler inputs changed.')
