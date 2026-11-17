# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
b=Path(__file__).resolve().parent; w=b/'p10-worktree'; out=b/'p12-integrated-proposal';out.mkdir(exist_ok=True)
for part in ['p12-native-backend-proposal','p12-control-owner-proposal']:
 for p in (b/part).rglob('*.rs'):
  rel=p.relative_to(b/part);q=out/rel;q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(p.read_bytes())
def edit(rel,a,z):
 p=out/rel;s=p.read_text();assert s.count(a)==1,(rel,a[:100],s.count(a));p.write_text(s.replace(a,z),newline='\n')
core='crates/airk-core/src/';kernel='crates/airk-kernel/src/';store='crates/airk-store/src/';cli='crates/airk-cli/src/'
edit(core+'lib.rs','pub mod catalog_policy;','pub mod catalog_policy;\npub mod catalog_refresh;')
# Explicit capability is independent of application/policy version; old saved
# capabilities retain historical bytes when the optional field is absent.
p=out/(core+'index_catalog.rs');src=(w/(core+'index_catalog.rs')).read_text();a='    pub policy_contract: u32,';assert src.count(a)==1;src=src.replace(a,a+'\n    #[serde(default,skip_serializing_if="Option::is_none")]\n    pub reference_refresh_contract:Option<u32>,');p.write_text(src,newline='\n')
edit(store+'service_catalog.rs','        policy_contract: SUPPORTED_POLICY_CONTRACT,','        policy_contract: SUPPORTED_POLICY_CONTRACT,\n        reference_refresh_contract:Some(1),')
for relative in [cli+'catalog_authority.rs',cli+'catalog_grant_draft.rs']:
 src=(w/relative).read_text();a='            production_activation_enabled: false,';assert src.count(a)==1;src=src.replace(a,'            reference_refresh_contract:None,\n'+a);p=out/relative;p.write_text(src,newline='\n')
# Resource key is enumerated by normal journaling, inventory and reclamation.
p=out/(store+'current/resources.rs');p.parent.mkdir(parents=True,exist_ok=True);s=(w/(store+'current/resources.rs')).read_text().replace('pub const KEYS: [&str; 6]','pub const KEYS: [&str; 7]').replace('    "scan-work",','    "scan-work",\n    "source-controls/1",');p.write_text(s,newline='\n')
# Exact current writer is retained BEFORE all recovery and through publication.
a='''        let reviewed_writer = if let Some((project,request)) = reviewed {
            ensure!(!full && paths.is_none(),"reviewed full fallback must be exact and explicit");
            let writer=self.store.lock().context(Stage("writer_admission"))?;
            self.require_refresh_base_before_recovery(project,request)?;
            Some(writer)
        } else {None};'''
z='''        let (reviewed_writer,reviewed_controls) = if let Some((project,request)) = reviewed {
            ensure!(!full,"reviewed refresh cannot silently rebuild");
            let writer=self.store.lock().context(Stage("writer_admission"))?;
            let controls=self.require_refresh_base_before_recovery(project,request)?;
            (Some(writer),controls)
        } else {(None,None)};'''
edit(kernel+'current/indexing.rs',a,z)
edit(kernel+'current/indexing.rs','    fn index_current_selected(','''    pub(crate) fn index_current_reviewed_refresh_paths(&self,project:airk_core::service::ProjectBinding,request:&airk_core::service_configuration::ConfigurationRequest,paths:&std::collections::BTreeSet<String>)->Result<airk_ingest::IndexReport> {
        ensure!(!paths.is_empty(),"empty reviewed source subset");
        self.index_current_selected(false,true,Some(paths),Some((project,request)))
    }

    fn index_current_selected(''')
edit(kernel+'current/indexing.rs','''        // A newly initialized (or format-rebuilt) empty index needs the same''','''        if let Some((_,request))=reviewed {
            let airk_core::service_configuration::ProjectConfiguration::RefreshReferences {selection}=&request.configuration else {unreachable!()};
            if selection.mode==airk_core::catalog_refresh::RefreshMode::SelectedOnly {
                ensure!(paths.is_some(),airk_core::catalog_refresh::RefreshReviewRequired {reasons:vec![airk_core::catalog_refresh::RefreshFallbackReason::DependencyInput]});
                ensure!(!configuration_changed,airk_core::catalog_refresh::RefreshReviewRequired {reasons:vec![airk_core::catalog_refresh::RefreshFallbackReason::ConfigurationChanged]});
            }
        }
        // A newly initialized (or format-rebuilt) empty index needs the same''')
edit(kernel+'current/indexing.rs','''        let progressive_enabled = defer_completion
            && paths.is_none()''','''        let progressive_enabled = defer_completion
            && reviewed.is_none()
            && paths.is_none()''')
edit(kernel+'current/indexing.rs','airk_ingest::scan_payloads_with_execution_source_policy(','airk_ingest::scan_payloads_with_optional_control_snapshot(')
edit(kernel+'current/indexing.rs','''            permanent_policy.as_deref(),
            |request|''','''            permanent_policy.as_deref(),
            reviewed_controls.clone(),
            |request|''')
edit(kernel+'current/indexing.rs','''        report.repaired_retained_artifacts = *repaired_retained_artifacts;''','''        let refreshed_baseline = if let Some(controls)=&reviewed_controls {
            if paths.is_some() {controls.revalidate_inputs()?;}
            let (_,request)=reviewed.context("tracked refresh lost native request")?;
            let baseline=controls.seal_complete(airk_core::identity::framed_digest(b"airk/refresh-applied-configuration/1",&[&serde_json::to_vec(&configuration)?]),request.settings)?;
            let bytes=serde_json::to_vec(&baseline)?;
            let read=self.current_reader()?;
            let prior=read.resource_reference(airk_core::source_control::CONTROL_BASELINE_RESOURCE)?.map(|reference|self.store.read_current_resource(&reference)).transpose()?;
            (prior.as_deref()!=Some(bytes.as_slice())).then_some(bytes)
        } else {None};
        report.repaired_retained_artifacts = *repaired_retained_artifacts;''')
edit(kernel+'current/indexing.rs','''        if !changes.is_empty()
            || configuration_changed''','''        if !changes.is_empty()
            || refreshed_baseline.is_some()
            || configuration_changed''')
a='''            if let Some(work) = &final_scan_work {
                resources.push(work.resource_change(&self.store, &writer, &reservation)?);
            }
            let result = CurrentUpdater'''
z='''            if let Some(work) = &final_scan_work {
                resources.push(work.resource_change(&self.store, &writer, &reservation)?);
            }
            // Publish attestation only after successful complete capture or
            // exact saved-control revalidation. Ordinary changes invalidate it.
            if reviewed_controls.is_some() {
                if let Some(bytes)=&refreshed_baseline {
                    resources.push(airk_store::current::resources::ResourceChange {key:airk_core::source_control::CONTROL_BASELINE_RESOURCE.into(),reference:Some(self.store.retain_current_resource_owned(&writer,bytes,&reservation)?)});
                } else {
                    let read=self.current_reader()?;
                    resources.push(airk_store::current::resources::ResourceChange {key:airk_core::source_control::CONTROL_BASELINE_RESOURCE.into(),reference:read.resource_reference(airk_core::source_control::CONTROL_BASELINE_RESOURCE)?});
                }
            }
            let result = CurrentUpdater'''
edit(kernel+'current/indexing.rs',a,z)
# Only an existing authenticated witness can carry through a known semantic-only
# publication. Preserve the exact resource reference; never re-attest a source
# scan or metadata writer that supplied no qualified source provenance.
s=(w/(kernel+'current/update.rs')).read_text();a='''        let trusted_cache = read''';z='''        let mut control_resources=resources.to_vec();
        let control_key=airk_core::source_control::CONTROL_BASELINE_RESOURCE;
        if !resources.iter().any(|resource|resource.key==control_key) {
            let pure_semantic=changes.is_empty() && self.configuration.is_none()
                && (semantic_inputs.is_some() || !resources.is_empty())
                && resources.iter().all(|resource|matches!(resource.key.as_str(),"semantic-work"|"semantic-policy"|"embedding-cache"));
            let mut carry=None;
            if pure_semantic && let Some(reference)=read.resource_reference(control_key)? {
                ensure!(reference.bytes<=airk_core::source_control::MAX_CONTROL_BASELINE_BYTES,"retained control witness exceeds bound");
                let bytes=self.store.read_current_resource(&reference)?;
                let baseline:airk_core::source_control::SourceControlBaseline=serde_json::from_slice(&bytes)?;baseline.validate()?;
                let state=read.collection_state()?.context("semantic carry lacks applied configuration")?;
                let same_configuration=baseline.configuration==airk_core::identity::framed_digest(b"airk/refresh-applied-configuration/1",&[&serde_json::to_vec(&state.configuration)?]);
                if same_configuration {
                    if let Some(semantic)=semantic.as_ref() {
                        ensure!(serde_json::to_vec(&state.configuration.model)?==serde_json::to_vec(&Some(semantic.objects.clone()))?
                            && state.configuration.vector_selection.digest()?==semantic.selection.digest()?,"semantic carry changed model/selection configuration");
                    }
                    if let Some(inputs)=semantic_inputs {
                        for input in inputs {
                            let current=read.artifact(&input.artifact.path)?.context("semantic carry source missing")?;
                            ensure!(serde_json::to_vec(&current)?==serde_json::to_vec(&input.artifact)?,"semantic carry changed source/extraction/profile binding");
                        }
                    }
                    carry=Some(reference);
                }
            }
            control_resources.push(airk_store::current::resources::ResourceChange {key:control_key.into(),reference:carry});
        }
        let resources=control_resources.as_slice();
        let trusted_cache = read'''
assert s.count(a)==1;s=s.replace(a,z);p=out/(kernel+'current/update.rs');p.write_text(s,newline='\n')
# An accepted selected request must not trigger generic startup replacement or
# Current recovery before its own guarded execution. Use the already existing
# observer open path and leave all unrelated watcher watermarks untouched.
edit(store+'service_jobs.rs','    pub fn job(&self, id: Sha256Digest) -> Result<JobRecord> {','''    pub fn pending_reference_refresh(&self,project:ProjectBinding)->Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_jobs j JOIN service_configuration c ON c.job=j.id WHERE j.project=?1 AND j.state IN('pending','running') AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation')='refresh_references')",[project.to_string()],|row|row.get(0))?)
    }
    pub fn job(&self, id: Sha256Digest) -> Result<JobRecord> {''')
edit(cli+'project_worker.rs','    let split_observer = registry', '    let refresh_pending=registry.pending_reference_refresh(init.project)?;\n    let split_observer = registry')
edit(cli+'project_worker.rs','    let kernel = if split_write_fenced {\n        split_observer.context("staged participant observer missing")?\n    } else {','    let kernel = if split_write_fenced {\n        split_observer.context("staged participant observer missing")?\n    } else if refresh_pending {\n        Kernel::open_existing_service_observer(&registration)?\n    } else {')
edit(cli+'project_worker.rs','    if !split_write_fenced {\n        source_configuration::recover(', '    if !split_write_fenced && !refresh_pending {\n        source_configuration::recover(')
edit(cli+'project_worker.rs','    if !split_write_fenced\n        && kernel.service_recovery_required()', '    if !split_write_fenced && !refresh_pending\n        && kernel.service_recovery_required()')
edit(cli+'project_worker.rs','                let mut automatic_settled = false;','                let refresh_job=registry.configuration_work(job.id)?.is_some_and(|work|matches!(work.request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}));\n                let mut automatic_settled = false;')
edit(cli+'project_worker.rs','                        if !split_job {\n                            let _ = reconcile_semantic_setup(', '                        if !split_job && !refresh_job {\n                            let _ = reconcile_semantic_setup(')
edit(store+'service_configuration.rs','    #[test]\n    fn unregister_is_atomic_preserves_other_projects_and_old_request_replay_cannot_unregister_a_reregistration()','''    #[test]
    fn generic_configuration_cannot_admit_refresh_or_consume_unrelated_work()->Result<()> {
        use airk_core::{catalog_refresh::{RefreshSelection,RefreshMember,RefreshSource,RefreshMode},index_catalog::{IndexId,SourceRootId,GrantId,QueryProjectId,CatalogRevision},identity::RevisionToken,service::CombinedConnectRequest};
        let temporary=tempfile::tempdir()?;let caller=Incarnation::from_bytes([1;16]);
        let mut registry=open(temporary.path(),caller)?;let selected=project(temporary.path())?;registry.register(&selected)?;
        let other=ProjectRegistration::new(selected.root.clone(),selected.data_root.clone(),"refresh-unrelated".into())?;registry.register(&other)?;
        let other_key=key(&mut registry,41)?;let other_job=registry.enqueue(other.binding,caller,&other_key,JobKind::Index)?;
        let key=key(&mut registry,42)?;let id=Incarnation::from_bytes([7;16]);let hash=Sha256Digest::from_bytes([8;32]);let index=IndexId::from_identity(id);
        let mut request=request(false);request.configuration=ProjectConfiguration::RefreshReferences {selection:Box::new(RefreshSelection {schema:1,caller,key:key.clone(),original_result_digest:hash,original_evidence_selection_digest:hash,original_query_digest:hash,mode:RefreshMode::SelectedOnly,
            selection:CombinedConnectRequest {schema:1,context_key:hash,namespace:key.namespace,expected_catalog_revision:CatalogRevision::INITIAL,grant_id:GrantId::from_identity(id),expected_grant_revision:CatalogRevision::INITIAL,project_ids:vec![QueryProjectId::from_identity(id)],index_ids:Some(vec![index])},
            members:vec![RefreshMember {index_id:index,original_publication:RevisionToken {incarnation:id,generation:1,revision:hash},sources:vec![RefreshSource {source_root_id:SourceRootId::from_identity(id),source_relative_path:"one.txt".into(),canonical_root:selected.root.clone(),indexed_hash:hash}]}]})};
        request.validate()?;
        assert!(registry.enqueue_configuration(selected.binding,caller,&key,&request,hash).is_err());
        assert!(registry.request_receipt(caller,&key)?.0.is_none());
        assert!(!registry.pending_reference_refresh(selected.binding)?);
        assert!(registry.next_pending_job(selected.binding)?.is_none());
        assert_eq!(registry.job(other_job.id)?.state,JobState::Pending);
        assert_eq!(registry.next_pending_job(other.binding)?.context("unrelated job missing")?.id,other_job.id);
        Ok(())
    }
    #[test]
    fn unregister_is_atomic_preserves_other_projects_and_old_request_replay_cannot_unregister_a_reregistration()''')
# Native receipt validates actual selected-vs-broader scope; no replay or requery.
edit(cli+'reference_refresh_control.rs','    ensure!(selection.mode==RefreshMode::FullReconcileReviewed,"selected refresh requires authenticated control provenance");','')
edit(cli+'reference_refresh_control.rs','"effective_mode":selection.mode,"effective_scope":"whole_index_explicitly_reviewed",','"effective_mode":selection.mode,"effective_scope":if selection.mode==RefreshMode::SelectedOnly {"selected_sources"} else {"whole_index_explicitly_reviewed"},')
edit(store+'service_configuration.rs','&& value["effective_scope"]=="whole_index_explicitly_reviewed"','&& value["effective_scope"]==if selection.mode==airk_core::catalog_refresh::RefreshMode::SelectedOnly {"selected_sources"} else {"whole_index_explicitly_reviewed"}')
# Preserve typed no-effect/refusal evidence and exact successful publication
# during ordinary immutable Configure-result crash recovery.
edit(core+'catalog_refresh.rs','impl std::fmt::Display for RefreshReviewRequired {','''impl RefreshReviewRequired {
    pub fn validate(&self)->Result<()> {ensure!(!self.reasons.is_empty(),"empty refresh review reason");unique_bounded(&self.reasons,9)}
}
impl std::fmt::Display for RefreshReviewRequired {''')
edit(core+'service.rs','    ReferenceRefreshReceipt {\n        job:Option<JobRecord>,','''    ReferenceRefreshReviewRequired {
        project:ProjectBinding,caller:Incarnation,key:RequestKey,
        request:Box<crate::service_configuration::ConfigurationRequest>,
        review:crate::catalog_refresh::RefreshReviewRequired,
    },
    ReferenceRefreshReceipt {
        job:Option<JobRecord>,''')
edit(cli+'reference_refresh_control.rs','/// Exact saved acknowledgment, independent','''/// Only a fresh pre-admission typed refusal can claim no accepted job.
/// An exact already-accepted key always returns its historical accepted job.
pub(crate) fn admit_response(registry:&mut ServiceRegistry,project:ProjectBinding,caller:Incarnation,key:&RequestKey,request:&ConfigurationRequest)->Result<airk_core::service::ControlResponse> {
    match admit(registry,project,caller,key,request) {
        Ok(job)=>Ok(airk_core::service::ControlResponse::Job {job}),
        Err(error)=>if let Some(review)=error.downcast_ref::<airk_core::catalog_refresh::RefreshReviewRequired>() {
            review.validate()?;
            Ok(airk_core::service::ControlResponse::ReferenceRefreshReviewRequired {project,caller,key:key.clone(),request:Box::new(request.clone()),review:review.clone()})
        } else {Err(error)},
    }
}

/// Exact saved acknowledgment, independent''')
a='''        } => ControlResponse::Job {
            job: if matches!(request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}) {
                crate::reference_refresh_control::admit(registry,project,caller,&key,&request)?
            } else {
                registry.enqueue_configuration(project,caller,&key,&request,crate::runtime_pair::engine_identity()?)?
            },
        },'''
z='''        } => {
            if matches!(request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}) {
                crate::reference_refresh_control::admit_response(registry,project,caller,&key,&request)?
            } else {
                ControlResponse::Job {job:registry.enqueue_configuration(project,caller,&key,&request,crate::runtime_pair::engine_identity()?)?}
            }
        },'''
edit(cli+'service_management.rs',a,z)
a='''    qualify(registry,project,&work.request)?;
    let before=selection.members[0].original_publication;

    let report=kernel.refresh_service_references(project,&work.request)?;'''
z='''    let before=selection.members[0].original_publication;
    let effect=(||->Result<airk_ingest::IndexReport> {qualify(registry,project,&work.request)?;kernel.refresh_service_references(project,&work.request)})();
    let report=match effect {
        Ok(report)=>report,
        Err(error)=>if let Some(review)=error.downcast_ref::<airk_core::catalog_refresh::RefreshReviewRequired>() {
            review.validate()?;
            let refusal=serde_json::json!({"schema":"airk.reference-refresh-review-required/1","selection_digest":selection.identity()?,"original_query_digest":selection.original_query_digest,"review":review,"publication_applied":false,"requery_executed":false});
            registry.record_configuration_result(lease,job,&refusal)?;
            return Ok(JobOutcome::Failed {code:"refresh_scope_changed_review_required".into()});
        } else {return Err(error)},
    };'''
# CLI has no direct production ingest dependency; infer closure's report type.
z=z.replace('(||->Result<airk_ingest::IndexReport>', '(||')
edit(cli+'reference_refresh_control.rs',a,z)
edit(store+'service_configuration.rs','    ensure!(selection.members.len()==1 && value["schema"]=="airk.reference-refresh-result/1"','''    if value["schema"]=="airk.reference-refresh-review-required/1" {
        ensure!(value["selection_digest"]==serde_json::to_value(selection.identity()?)?
            && value["original_query_digest"]==serde_json::to_value(selection.original_query_digest)?
            && value["publication_applied"]==false && value["requery_executed"]==false,ConfigurationResultCorrupt("refresh refusal intent changed"));
        let review:airk_core::catalog_refresh::RefreshReviewRequired=serde_json::from_value(value["review"].clone()).context(ConfigurationResultCorrupt("refresh refusal missing typed reason"))?;
        review.validate()?;return Ok(());
    }
    ensure!(selection.members.len()==1 && value["schema"]=="airk.reference-refresh-result/1"''')
a='''            connection.execute(
                "UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2",
                params![
                    serde_json::to_vec(&airk_core::service::JobOutcome::Succeeded {
                        revision: None
                    })?,
                    id
                ],
            )?;'''
z='''            let outcome=if saved["schema"]=="airk.reference-refresh-review-required/1" {
                airk_core::service::JobOutcome::Failed {code:"refresh_scope_changed_review_required".into()}
            } else {airk_core::service::JobOutcome::Succeeded {revision:if saved["schema"]=="airk.reference-refresh-result/1" {Some(serde_json::from_value(saved["after_publication"].clone())?)} else {None}}};
            let state=if matches!(outcome,airk_core::service::JobOutcome::Failed {..}) {"failed"} else {"succeeded"};
            connection.execute("UPDATE service_jobs SET state=?1,outcome=?2 WHERE id=?3",params![state,serde_json::to_vec(&outcome)?,id])?;'''
edit(store+'service_configuration.rs',a,z)
edit(store+'service_configuration.rs','    #[test]\n    fn generic_configuration_cannot_admit_refresh_or_consume_unrelated_work()','''    #[test]
    fn saved_refresh_result_recovers_exact_publication_and_typed_refusal_never_becomes_success()->Result<()> {
        use airk_core::{identity::RevisionToken,service::JobOutcome};
        for refused in [false,true] {
            let temporary=tempfile::tempdir()?;let caller=Incarnation::from_bytes([1;16]);let engine=Sha256Digest::from_bytes([4;32]);let mut registry=open(temporary.path(),caller)?;let project=project(temporary.path())?;registry.register(&project)?;
            let key=key(&mut registry,33)?;let ordinary=request(false);let job=registry.enqueue_configuration(project.binding,caller,&key,&ordinary,engine)?;
            let index=format!("idx_{}",Incarnation::from_bytes([5;16]));let root=format!("src_{}",Incarnation::from_bytes([6;16]));let grant=format!("grt_{}",Incarnation::from_bytes([7;16]));let query_project=format!("prj_{}",Incarnation::from_bytes([8;16]));let hash=Sha256Digest::from_bytes([9;32]);
            let before=RevisionToken {incarnation:ordinary.incarnation,generation:1,revision:hash};let after=RevisionToken {generation:2,..before};
            // This result-codec fixture installs saved immutable native work
            // directly in its owned registry. It is not an admission/authority
            // or real source-publication proof (covered by separate native tests).
            let work:ConfigurationWork=serde_json::from_value(serde_json::json!({"engine":engine,"request":{"incarnation":ordinary.incarnation,"settings":ordinary.settings,"configuration":{"operation":"refresh_references","selection":{"schema":1,"caller":caller,"key":key,"original_result_digest":hash,"original_query_digest":hash,"original_evidence_selection_digest":hash,"mode":"selected_only","selection":{"schema":1,"context_key":hash,"namespace":key.namespace,"expected_catalog_revision":1,"grant_id":grant,"expected_grant_revision":1,"project_ids":[query_project],"index_ids":[index]},"members":[{"index_id":index,"original_publication":before,"sources":[{"source_root_id":root,"source_relative_path":"one.txt","canonical_root":project.root,"indexed_hash":hash}]}]}}}}))?;
            work.request.validate()?;registry.connection.execute("UPDATE service_configuration SET payload=?1 WHERE job=?2",params![serde_json::to_vec(&work)?,job.id.to_string()])?;
            let ProjectConfiguration::RefreshReferences {selection}=&work.request.configuration else {unreachable!()};
            let result=if refused {serde_json::json!({"schema":"airk.reference-refresh-review-required/1","selection_digest":selection.identity()?,"original_query_digest":hash,"review":{"reasons":["pending_recovery"]},"publication_applied":false,"requery_executed":false})} else {serde_json::json!({"schema":"airk.reference-refresh-result/1","selection_digest":selection.identity()?,"original_query_digest":hash,"effective_mode":"selected_only","effective_scope":"selected_sources","before_publication":before,"after_publication":after,"report":{"generation":2},"status":{"publication":{"token":after}},"requery_executed":false})};
            {let mut lease=ProjectLease::acquire(&registry.root,project.binding)?;registry.admit_worker(&mut lease)?;registry.claim_job_id(&lease,job.id)?.context("fixture Configure not claimed")?;registry.record_configuration_result(&lease,job.id,&result)?;}
            let mut replacement=ProjectLease::acquire(&registry.root,project.binding)?;registry.admit_worker(&mut replacement)?;registry.recover_jobs(&replacement)?;
            let recovered=registry.job(job.id)?;
            assert_eq!(registry.configuration_result(job.id)?,Some(result));
            if refused {assert_eq!(recovered.state,JobState::Failed);assert_eq!(recovered.outcome,Some(JobOutcome::Failed {code:"refresh_scope_changed_review_required".into()}));}
            else {assert_eq!(recovered.state,JobState::Succeeded);assert_eq!(recovered.outcome,Some(JobOutcome::Succeeded {revision:Some(after)}));}
        }
        Ok(())
    }
    #[test]
    fn generic_configuration_cannot_admit_refresh_or_consume_unrelated_work()''')
p=out/(kernel+'current/enrichment/tests.rs');p.parent.mkdir(parents=True,exist_ok=True);p.write_text((w/(kernel+'current/enrichment/tests.rs')).read_text()+'\n'+(b/'p12-hybrid-continuity-test.proposed.rs').read_text(encoding='utf-8-sig'),newline='\n')
base={str(p.relative_to(out)):hashlib.sha256((w/p.relative_to(out)).read_bytes()).hexdigest() if (w/p.relative_to(out)).exists() else None for p in out.rglob('*.rs')}
(b/'p12-integrated-base.json').write_text(json.dumps(base,indent=2)+'\n')
print(json.dumps({'files':len(base),'status':'INERT positive selected/explicit qualification proposal; no compile or native execution'}))
