# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out=Path('D:/code/Github/airk/artifacts/v190')
changes=[]
path='crates/airk-core/src/service_configuration.rs'
old=(root/path).read_text(encoding='utf-8')
marker='    pub fn requests_source_pass(&self) -> bool {\n'
assert old.count(marker)==1
new=old.replace(marker,'''    /// Classifies already-admitted work for pause controls. This grants no
    /// admission, native ownership, phase transition, funding or source access.
    pub fn runs_while_paused(&self) -> bool {
        self.is_derived_cleanup()
            || matches!(self,
                Self::Unregister | Self::ApplySourcePolicy { .. }
                    | Self::SplitWork {
                        purpose: crate::catalog_split::SplitJobPurpose::OriginalWitness
                            | crate::catalog_split::SplitJobPurpose::DestinationCleanup
                            | crate::catalog_split::SplitJobPurpose::OriginalCleanup,
                        ..
                    })
    }
''' +marker)
changes.append((path,old,new))
path='crates/airk-store/src/service_jobs.rs'
old=(root/path).read_text(encoding='utf-8')
start=old.index('    pub fn job_runs_while_paused(')
end=old.index('    /// Fair oldest-pending order',start)
new=old[:start]+'''    pub fn job_runs_while_paused(&self, id: Sha256Digest) -> Result<bool> {
        let job = record(&self.connection, id)?;
        let configuration = if job.kind == JobKind::Configure {
            super::configuration::work(&self.connection,id)?
        } else { None };
        Ok(runs_while_paused(job.kind,
            configuration.as_ref().map(|work| &work.request.configuration)))
    }
''' +old[end:]
marker='pub(super) const SCHEMA: &str = "\n'
assert new.count(marker)==1
new=new.replace(marker,'''fn runs_while_paused(
    kind: JobKind,
    configuration: Option<&airk_core::service_configuration::ProjectConfiguration>,
) -> bool {
    matches!(kind,JobKind::Inspect|JobKind::PlanInspect)
        || kind == JobKind::Configure && configuration.is_some_and(|work|work.runs_while_paused())
}

''' +marker)
marker='''    #[test]
    fn full_queue_rotation_protects_the_failure_named_by_a_new_retry() -> Result<()> {'''
assert new.count(marker)==1
test='''    #[test]
    fn paused_native_control_matches_exact_sql_classification_for_all_split_purposes() -> Result<()> {
        use airk_core::{catalog_split::SplitJobPurpose,service_configuration::ProjectConfiguration as C};
        let db=Connection::open_in_memory()?;
        let sql=format!("WITH j(id,kind) AS (VALUES('classification',?1)), service_configuration(job,payload) AS (VALUES('classification',?2)) SELECT {RUN_WHILE_PAUSED} FROM j");
        let classify=|kind:JobKind,configuration:Option<&C>,expected:bool| -> Result<()> {
            // JSON is the actual saved-work shape, but no real job/registration,
            // admitted claim, policy mutation, source or funding is manufactured.
            let payload=configuration.map(|configuration|serde_json::to_string(&serde_json::json!({"request":{"configuration":configuration}}))).transpose()?;
            let database:bool=db.query_row(&sql,params![kind.code(),payload],|row|row.get(0))?;
            assert_eq!(database,expected,"SQL: kind={kind:?},configuration={configuration:?}");
            assert_eq!(runs_while_paused(kind,configuration),database,
                "native control diverged from actual SQL pause admission classification");
            Ok(())
        };
        for kind in [JobKind::Index,JobKind::Reconcile,JobKind::Vectors,JobKind::ModelSetup,
            JobKind::Rebuild,JobKind::Recover,JobKind::Configure,JobKind::Maintain,
            JobKind::Inspect,JobKind::PlanInspect,JobKind::Repair,JobKind::Verify] {
            classify(kind,None,matches!(kind,JobKind::Inspect|JobKind::PlanInspect))?;
        }
        let digest=Sha256Digest::from_bytes([1;32]);
        for configuration in [C::Unregister,C::PrepareDerivedCleanup,C::DeleteDerived {preview:digest}] {
            classify(JobKind::Configure,Some(&configuration),true)?;
        }
        let policy:C=serde_json::from_value(serde_json::json!({
            "operation":"apply_source_policy","operation_id":Incarnation::from_bytes([2;16]),
            "policy":{"schema":"airk.source-policy/1","index_id":airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes([3;16])),
                "owner_binding":digest,"namespace":"collections-1.4","collection":"classification","epoch":1,
                "roots":[],"positive_includes":null,"denies":[]}}))?;
        classify(JobKind::Configure,Some(&policy),true)?;
        for purpose in [SplitJobPurpose::DestinationBuild,SplitJobPurpose::OriginalWitness,
            SplitJobPurpose::OriginalRemoval,SplitJobPurpose::DestinationCleanup,SplitJobPurpose::OriginalCleanup] {
            let configuration=C::SplitWork {operation_id:Incarnation::from_bytes([4;16]),request_digest:digest,
                expected_receipt_digest:digest,purpose,require_semantic_ready:false};
            classify(JobKind::Configure,Some(&configuration),matches!(purpose,
                SplitJobPurpose::OriginalWitness|SplitJobPurpose::DestinationCleanup|SplitJobPurpose::OriginalCleanup))?;
        }
        classify(JobKind::Configure,Some(&C::SplitWork {
            operation_id:Incarnation::from_bytes([4;16]),request_digest:digest,
            expected_receipt_digest:digest,purpose:SplitJobPurpose::DestinationBuild,
            require_semantic_ready:true}),false)?;
        for configuration in [C::Vectors {enabled:true},C::Vectors {enabled:false},
            C::Detail {level:airk_core::extraction::IndexDetail::Detailed},C::BundledModel,
            C::RemoveRoot {alias:"classification".into()},C::ResetScanLimits,C::ClearUsage] {
            classify(JobKind::Configure,Some(&configuration),false)?;
        }
        Ok(())
    }

'''
new=new.replace(marker,test+marker)
changes.append((path,old,new))
path='crates/airk-cli/tests/combined_reads/split.rs'
old=(root/path).read_text(encoding='utf-8')
start=old.index('async fn terminal(')
pos=old.index('    let deadline = Instant::now() + Duration::from_secs(180);',start)
new=old[:pos]+'    let mut pause_checked_witness = None;\n'+old[pos:]
marker='''        if let Some(observer) = capture.as_mut() {
'''
assert new.count(marker)>=1
new=new.replace(marker,'''        if let Some(witness)=snapshot.receipt.jobs.iter().rev().find(|job|
            job.purpose==airk_core::catalog_split::SplitJobPurpose::OriginalWitness)
            && pause_checked_witness!=Some(witness.job) {
            let registry=ServiceRegistry::open_saved(&f.control)?;
            ensure!(registry.job_runs_while_paused(witness.job)?,
                "actual claimed witness would receive paused=true from native controls");
            let progress=airk_cli::service_runtime::request(&f.control,ControlMethod::Progress {
                project:snapshot.request.original_registration.binding }).await?;
            ensure!(matches!(&progress,ControlResponse::Progress {project_paused:Some(true),..}),
                "explicit witness altered desired original Pause: {}",serde_json::to_string(&progress)?);
            if let ControlResponse::Progress {pending_job:Some(job),pending_paused,..}=&progress
                && job.id==witness.job {
                ensure!(*pending_paused==Some(false),"native Progress marks exact witness paused: {}",serde_json::to_string(&progress)?);
            }
            pause_checked_witness=Some(witness.job);
        }
''' +marker,1)
marker='''    let complete = terminal(&f, operation, false, &mut capture, &mut _service).await?;'''
assert new.count(marker)==1
new=new.replace(marker,marker+'''
    ensure!(ServiceRegistry::open_saved(&f.control)?.get(f.registrations[2].binding)?.paused,
        "native split changed the original user's Pause setting");''')
changes.append((path,old,new))
patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+path,tofile='b/'+path)) for path,old,new in changes)
(out/'p10-paused-classification.proposed.patch').write_text(patch,encoding='utf-8',newline='\n')
for path,old,new in changes:
    (out/('p10-paused-'+Path(path).name)).write_text(new,encoding='utf-8',newline='\n')
print('Created inert canonical pause-classification correction; compiler inputs unchanged.')
