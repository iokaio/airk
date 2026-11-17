# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
b=Path(__file__).resolve().parent
w=b/'p10-worktree'; out=b/'p12-native-backend-proposal'
out.mkdir(exist_ok=True)
originals={}
def read(relative):
    p=w/relative; originals[relative]=hashlib.sha256(p.read_bytes()).hexdigest();return p.read_text()
def write(relative,value):
    p=out/relative;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(value,newline='\n')
def replace(text,old,new,count=1):
    assert text.count(old)==count,(old[:100],text.count(old),count)
    return text.replace(old,new)
core='crates/airk-core/src/'
s=read(core+'lib.rs');s=replace(s,'pub mod catalog_policy;','pub mod catalog_policy;\npub mod catalog_refresh;');write(core+'lib.rs',s)
write(core+'catalog_refresh.rs',(b/'p12-catalog_refresh.proposed.rs').read_text())
s=read(core+'service_configuration.rs')
s=replace(s,'    Unregister,','''    Unregister,
    /// Existing authenticated native management only; generic Store admission
    /// rejects this variant. No evidence/MCP read can construct accepted work.
    RefreshReferences { selection:Box<crate::catalog_refresh::RefreshSelection> },''',1)
s=replace(s,'            Self::ImportUsage { input }','''            Self::RefreshReferences {selection} => selection.validate()?,
            Self::ImportUsage { input }''',1)
write(core+'service_configuration.rs',s)
s=read(core+'service.rs')
s=replace(s,'    ConfigurationResult {\n        id: Sha256Digest,\n    },','''    ConfigurationResult {
        id: Sha256Digest,
    },
    /// Complete exact-key historical refresh proof; never starts or retries work.
    ReferenceRefreshReceipt {caller:Incarnation,key:RequestKey},''')
needle='pub enum ControlResponse {'
s=replace(s,needle,needle+'''
    ReferenceRefreshReceipt {
        job:Option<JobRecord>,
        configuration:Option<crate::service_configuration::ConfigurationWork>,
        result:Option<serde_json::Value>,
        expired:bool,
    },''')
write(core+'service.rs',s)
kernel='crates/airk-kernel/src/'
s=read(kernel+'lib.rs');s=replace(s,'mod service_enrichment;','mod service_enrichment;\nmod service_reference_refresh;');write(kernel+'lib.rs',s)
write(kernel+'service_reference_refresh.rs',(b/'p12-service_reference_refresh.proposed.rs').read_text())
s=read(kernel+'service_configuration.rs')
s=replace(s,'            ProjectConfiguration::SplitWork { .. } => {','''            ProjectConfiguration::RefreshReferences {..} => {
                anyhow::bail!("reference refresh requires its exact native worker owner")
            }
            ProjectConfiguration::SplitWork { .. } => {''')
write(kernel+'service_configuration.rs',s)
s=read(kernel+'current/indexing.rs')
s=replace(s,'self.index_current_selected(full, defer_completion, None)','self.index_current_selected(full, defer_completion, None, None)')
s=replace(s,'self.index_current_selected(false, defer_completion, Some(&paths))','self.index_current_selected(false, defer_completion, Some(&paths), None)')
needle='''    fn index_current_selected(
        &self,
        full: bool,
        defer_completion: bool,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<airk_ingest::IndexReport> {'''
replacement='''    pub(crate) fn index_current_reviewed_refresh(
        &self,
        project:airk_core::service::ProjectBinding,
        request:&airk_core::service_configuration::ConfigurationRequest,
    )->Result<airk_ingest::IndexReport> {
        // Legacy control provenance requires explicit full reconciliation.
        // Selected execution is never silently widened through this adapter.
        self.index_current_selected(false,true,None,Some((project,request)))
    }

    fn index_current_selected(
        &self,
        full: bool,
        defer_completion: bool,
        paths: Option<&std::collections::BTreeSet<String>>,
        reviewed:Option<(airk_core::service::ProjectBinding,&airk_core::service_configuration::ConfigurationRequest)>,
    ) -> Result<airk_ingest::IndexReport> {'''
s=replace(s,needle,replacement)
s=replace(s,'            &mut repaired_retained_artifacts,\n            paths,','            &mut repaired_retained_artifacts,\n            paths,\n            reviewed,')
s=replace(s,'            {\n                // The failed attempt has released its capture guard, writer and','''            {
                if reviewed.is_some() {
                    return Err(error.context("reviewed refresh stopped; retained-object repair requires a separate new review"));
                }
                // The failed attempt has released its capture guard, writer and''')
s=replace(s,'self.index_current_once(full, defer_completion, &mut repaired_retained_artifacts, paths)','self.index_current_once(full, defer_completion, &mut repaired_retained_artifacts, paths, reviewed)')
needle='''        repaired_retained_artifacts: &mut usize,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<airk_ingest::IndexReport> {'''
s=replace(s,needle,'''        repaired_retained_artifacts: &mut usize,
        paths: Option<&std::collections::BTreeSet<String>>,
        reviewed:Option<(airk_core::service::ProjectBinding,&airk_core::service_configuration::ConfigurationRequest)>,
    ) -> Result<airk_ingest::IndexReport> {''')
needle='''        if full {
            self.prepare_full_rebuild()'''
s=replace(s,needle,'''        let reviewed_writer = if let Some((project,request)) = reviewed {
            ensure!(!full && paths.is_none(),"reviewed full fallback must be exact and explicit");
            let writer=self.store.lock().context(Stage("writer_admission"))?;
            self.require_refresh_base_before_recovery(project,request)?;
            Some(writer)
        } else {None};
        if full {
            self.prepare_full_rebuild()''')
s=replace(s,'''        self.recover_runtime_bootstrap()
            .context(Stage("bootstrap"))?;''','''        if reviewed.is_none() {
            self.recover_runtime_bootstrap().context(Stage("bootstrap"))?;
        }''')
s=replace(s,'''        if let Some(scope) = self.current_scope().context(Stage("current_scope"))? {
            super::recover_current(&self.store, &self.collection_quota()?, &scope)
                .context(Stage("recover_current"))?;
        }
        let writer = self.store.lock().context(Stage("writer_admission"))?;''','''        if reviewed.is_none() {
            if let Some(scope) = self.current_scope().context(Stage("current_scope"))? {
                super::recover_current(&self.store, &self.collection_quota()?, &scope)
                    .context(Stage("recover_current"))?;
            }
        }
        let writer = match reviewed_writer {
            Some(writer)=>writer,
            None=>self.store.lock().context(Stage("writer_admission"))?,
        };''')
write(kernel+'current/indexing.rs',s)
store='crates/airk-store/src/'
s=read(store+'service_jobs.rs')
s=replace(s,'    explicit: bool,','    explicit: bool,\n    native_refresh: bool,')
s=replace(s,'''        let EnqueueOptions {
            explicit,''','''        let EnqueueOptions {
            explicit,
            native_refresh,''')
s=replace(s,'airk_core::service_configuration::ProjectConfiguration::SplitWork { .. }\n            )),','''airk_core::service_configuration::ProjectConfiguration::SplitWork { .. }
                    | airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}
            )),''')
s=replace(s,'airk_core::service_configuration::ProjectConfiguration::SplitWork { .. }\n            ),','''airk_core::service_configuration::ProjectConfiguration::SplitWork { .. }
                    | airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}
            ),''')
needle='''    fn enqueue_impl(
'''
method='''    /// Authenticated native adapter must establish current OS host/grant and
    /// exact backing before this call. This method creates no such authority;
    /// its transaction preserves the adapter's observed catalog fence.
    pub fn enqueue_native_reference_refresh(
        &mut self,project:ProjectBinding,caller:Incarnation,key:&RequestKey,
        request:&airk_core::service_configuration::ConfigurationRequest,engine:Sha256Digest,
    )->Result<JobRecord> {
        request.validate()?;
        let airk_core::service_configuration::ProjectConfiguration::RefreshReferences {selection}=&request.configuration else {
            anyhow::bail!("native reference refresh request missing");
        };
        ensure!(!request.prepare && selection.caller==caller && selection.key==*key,
            "refresh exact caller/key/request binding changed");
        self.enqueue_impl(project,caller,key,JobKind::Configure,EnqueueOptions {
            native_refresh:true,
            configuration:Some(Box::new(airk_core::service_configuration::ConfigurationWork {engine,request:request.clone()})),
            ..Default::default()
        })
    }

'''
s=replace(s,needle,method+needle)
needle='''        let epoch = i64::try_from(key.epoch).map_err(|_| JobError::KeyExpired)?;'''
s=replace(s,needle,'''        let is_refresh=configuration.as_ref().is_some_and(|work|matches!(work.request.configuration,
            airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}));
        ensure!(is_refresh==native_refresh,"refresh requires the authenticated native admission adapter");
        let epoch = i64::try_from(key.epoch).map_err(|_| JobError::KeyExpired)?;''')
needle='''        let (namespace, current): (String, i64) = transaction.query_row('''
s=replace(s,needle,'''        if native_refresh {
            let work=configuration.as_ref().context("native refresh work missing")?;
            let airk_core::service_configuration::ProjectConfiguration::RefreshReferences {selection}=&work.request.configuration else {unreachable!()};
            let state=super::catalog::state(&transaction)?;
            ensure!(state.activated && state.namespace==selection.selection.namespace
                && state.revision==selection.selection.expected_catalog_revision,
                "refresh catalog changed before atomic admission");
        }
        let (namespace, current): (String, i64) = transaction.query_row(''')
s=replace(s,'AND EXISTS(SELECT 1 FROM service_job_source source WHERE source.job=j.id)',
'''AND (EXISTS(SELECT 1 FROM service_job_source source WHERE source.job=j.id)
            OR EXISTS(SELECT 1 FROM service_configuration c WHERE c.job=j.id AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation')='refresh_references'))''')
write(store+'service_jobs.rs',s)
s=read(store+'service_catalog.rs')
marker='impl ServiceRegistry {\n    /// Administrative metadata only.'
helper='''/// Atomic counterpart of the native adapter's OS-host/full-scope review.
/// No source I/O and no authority is minted. The native adapter separately
/// authenticates the host; this binds its grant/backing/revision to insertion.
pub(super) fn require_refresh_admission(
    db:&Connection,project:airk_core::identity::Sha256Digest,
    selection:&airk_core::catalog_refresh::RefreshSelection,
)->Result<()> {
    selection.validate()?;
    let current=state(db)?;
    ensure!(current.activated && current.authority_contract==1
        && current.namespace==selection.selection.namespace
        && current.revision==selection.selection.expected_catalog_revision,
        "refresh catalog changed before admission");
    ensure!(selection.members.len()==1,"refresh requires one exact member");
    let index=operations::index_record(db,selection.members[0].index_id)?;
    ensure!(!index.tombstone && index.backing.binding==project
        && index.source_import==airk_core::index_catalog::SourceImportState::Complete,
        "refresh index backing is not active and complete");
    let grant=operations::read_persisted_grant(db,selection.selection.grant_id)?;
    grant.validate()?;
    ensure!(!grant.grant.revoked && grant.grant.revision==selection.selection.expected_grant_revision
        && grant.provenance.kind()==airk_core::grant_provenance::GrantProvenanceKind::UserReviewedProjectGrant,
        "refresh reviewed grant changed");
    let resolved=selection.selection.selection().resolve(&grant.grant)?;
    let member=resolved.member(index.id).ok_or_else(||anyhow::anyhow!("refresh member not in current grant"))?;
    ensure!(member.memberships.iter().all(|frozen|
        frozen.source_revision==index.source_revision
        && frozen.policy_epoch==index.policy.as_ref().map_or(0,|policy|policy.epoch.get())
        && frozen.policy_digest==index.policy.as_ref().map(|policy|policy.digest)),
        "refresh source/policy binding changed");
    let mut statement=db.prepare("SELECT id FROM index_sources WHERE index_id=?1 AND enabled=1 ORDER BY id LIMIT 18")?;
    let roots=statement.query_map([index.id.to_string()],|row|row.get::<_,String>(0))?
        .collect::<Result<Vec<_>,_>>()?;
    ensure!(!roots.is_empty() && roots.len()<=17,"refresh source inventory unavailable");
    for root in roots {
        let root:airk_core::index_catalog::SourceRootId=root.parse()?;
        ensure!(member.scope.iter().any(|atom|atom.source_root_id==root
            && atom.kind==airk_core::index_catalog::PathScope::Subtree && atom.path.is_empty()),
            "explicit full refresh requires whole-index authority; partial scope cannot widen");
    }
    Ok(())
}

'''
assert s.count(marker)==1
s=s.replace(marker,helper+marker)
write(store+'service_catalog.rs',s)
# Replace the provisional revision-only check with the owning full binding check.
jobs=(out/(store+'service_jobs.rs')).read_text()
old='''            let state=super::catalog::state(&transaction)?;
            ensure!(state.activated && state.namespace==selection.selection.namespace
                && state.revision==selection.selection.expected_catalog_revision,
                "refresh catalog changed before atomic admission");'''
jobs=replace(jobs,old,'            super::catalog::require_refresh_admission(&transaction,project,selection)?;')
write(store+'service_jobs.rs',jobs)
s=read(store+'service_configuration.rs')
needle='''pub(super) fn recover_results(
'''
helper='''fn validate_refresh_result(connection:&Connection,job:Sha256Digest,value:&serde_json::Value)->Result<()> {
    let Some(saved)=work(connection,job)? else {return Ok(())};
    let airk_core::service_configuration::ProjectConfiguration::RefreshReferences {selection}=&saved.request.configuration else {return Ok(())};
    let record=super::jobs::record(connection,job)?;
    ensure!(job==airk_core::catalog_source_change::source_job_id(record.project,selection.caller,&selection.key)?,
        ConfigurationResultCorrupt("refresh job lineage changed"));
    ensure!(selection.members.len()==1 && value["schema"]=="airk.reference-refresh-result/1"
        && value["selection_digest"]==serde_json::to_value(selection.identity()?)?
        && value["original_query_digest"]==serde_json::to_value(selection.original_query_digest)?
        && value["effective_mode"]==serde_json::to_value(selection.mode)?
        && value["effective_scope"]=="whole_index_explicitly_reviewed"
        && value["requery_executed"]==false,
        ConfigurationResultCorrupt("refresh receipt intent changed"));
    let before:airk_core::identity::RevisionToken=serde_json::from_value(value["before_publication"].clone())
        .context(ConfigurationResultCorrupt("refresh original publication missing"))?;
    let after:airk_core::identity::RevisionToken=serde_json::from_value(value["after_publication"].clone())
        .context(ConfigurationResultCorrupt("refresh resulting publication missing"))?;
    ensure!(before==selection.members[0].original_publication,
        ConfigurationResultCorrupt("refresh original publication changed"));
    before.validate_against(&after).context(ConfigurationResultCorrupt("refresh publication scope/order changed"))?;
    ensure!(value["status"]["publication"]["token"]==serde_json::to_value(after)?
        && value["report"]["generation"].as_u64()==Some(after.generation),
        ConfigurationResultCorrupt("refresh report/status publication mismatch"));
    Ok(())
}

'''
s=replace(s,needle,helper+needle)
old='''    Ok(Some(
        serde_json::from_slice(&bytes[8..8 + length])
            .context(ConfigurationResultCorrupt("invalid recorded JSON"))?,
    ))'''
new='''    let value=serde_json::from_slice(&bytes[8..8 + length])
        .context(ConfigurationResultCorrupt("invalid recorded JSON"))?;
    validate_refresh_result(connection,job,&value)?;
    Ok(Some(value))'''
s=replace(s,old,new)
needle='''        super::catalog::source_change::validate_configuration_result(&transaction, job, result)?;'''
s=replace(s,needle,needle+'\n        validate_refresh_result(&transaction,job,result)?;')
write(store+'service_configuration.rs',s)
cli='crates/airk-cli/src/'
s=read(cli+'lib.rs');s=replace(s,'mod catalog_split_control;','mod catalog_split_control;\nmod reference_refresh_control;');write(cli+'lib.rs',s)
write(cli+'reference_refresh_control.rs',(b/'p12-reference_refresh_control.proposed.rs').read_text())
s=read(cli+'service_management.rs')
s=replace(s,'            | ControlMethod::ConfigurationResult { .. }','            | ControlMethod::ConfigurationResult { .. }\n            | ControlMethod::ReferenceRefreshReceipt { .. }')
needle='''        ControlMethod::ConfigurationResult { id } => ControlResponse::Configuration {'''
s=replace(s,needle,'''        ControlMethod::ReferenceRefreshReceipt {caller,key} => {
            crate::reference_refresh_control::receipt(registry,caller,&key)?
        }
        ControlMethod::ConfigurationResult { id } => ControlResponse::Configuration {''')
old='''        } => ControlResponse::Job {
            job: registry.enqueue_configuration(
                project,
                caller,
                &key,
                &request,
                crate::runtime_pair::engine_identity()?,
            )?,
        },'''
new='''        } => ControlResponse::Job {
            job: if matches!(request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}) {
                crate::reference_refresh_control::admit(registry,project,caller,&key,&request)?
            } else {
                registry.enqueue_configuration(project,caller,&key,&request,crate::runtime_pair::engine_identity()?)?
            },
        },'''
s=replace(s,old,new);write(cli+'service_management.rs',s)
s=read(cli+'project_worker.rs')
old='''                            if work.request.prepare {
                                work.request.validate()?;'''
new='''                            if matches!(work.request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}) {
                                return crate::reference_refresh_control::execute(&kernel,&mut registry,&job_lease,job.id);
                            }
                            if work.request.prepare {
                                work.request.validate()?;'''
s=replace(s,old,new);write(cli+'project_worker.rs',s)
(b/'p12-native-backend-base.json').write_text(json.dumps(originals,indent=2)+'\n')
print(json.dumps({'files':len(list(out.rglob('*.rs'))),'base_files':len(originals)}))
