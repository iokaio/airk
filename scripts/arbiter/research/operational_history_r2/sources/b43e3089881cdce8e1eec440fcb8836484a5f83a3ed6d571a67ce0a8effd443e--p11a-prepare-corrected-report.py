# SPDX-License-Identifier: Apache-2.0
"""Corrected P11a Core/Kernel proposal only; never alter compiler inputs."""
import difflib,hashlib,json,subprocess
from pathlib import Path
base=Path(__file__).resolve().parent; root=base/'p10-worktree'; changes={}
def read(name):
    value=(root/name).read_text(encoding='utf-8'); changes[name]=[value,value]; return value
def replace(value,old,new):
    assert value.count(old)==1,old
    return value.replace(old,new)
oldpatch=(base/'p11a-family-reporting.patch').read_text(encoding='utf-8')
section=oldpatch.split('+++ b/crates/airk-core/src/profile_families.rs\n',1)[1]
old='\n'.join(line[1:] for line in section.splitlines() if line.startswith('+') and not line.startswith('+++'))+'\n'
prefix=old[:old.index('#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub struct ProfileRevision')]
prefix=prefix.replace('identity::Sha256Digest, reply::ReplyDetail','identity::{RevisionToken, Sha256Digest}').replace('extraction::IndexDetail','extraction::{ExtractionProfile, IndexDetail}')
retention=old[old.index('fn retention('):old.index('/// Pure projection over')]
core=prefix+'''
/// These are COLLECTION POLICY identities, never effective per-file profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionPolicyRevision { pub detail: IndexDetail, pub policy_digest: Sha256Digest }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum CollectionPolicyUnavailable { SelectedScope }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag="state",rename_all="snake_case",deny_unknown_fields)]
pub enum CollectionPolicyObservation {
    Known { revision: CollectionPolicyRevision },
    Unavailable { reason: CollectionPolicyUnavailable },
}
impl From<CollectionPolicyRevision> for CollectionPolicyObservation {
    fn from(revision:CollectionPolicyRevision)->Self {Self::Known {revision}}
}
impl CollectionPolicyObservation {
    pub fn known(self)->Option<CollectionPolicyRevision> {match self {Self::Known {revision}=>Some(revision),Self::Unavailable{..}=>None}}
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedFileProfile {
    pub adapter_id: String, pub adapter_version: String,
    pub effective_profile_digest: Sha256Digest,
    pub legacy_detail: IndexDetail,
    pub legacy_detail_was_absent: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRevision { pub publication: RevisionToken, pub path: String, pub source_hash: Sha256Digest }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyObservation { pub family: Family, pub support: Support, pub retention: Retention, pub reply_inclusion: ReplyInclusion }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ConfiguredFileProfileObservation { UnknownSourceInputsMayBeRequired }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileFamilyReport {
    pub schema: String,
    pub basis: String,
    pub file: FileRevision,
    pub applied_file: AppliedFileProfile,
    pub configured_collection_policy: CollectionPolicyObservation,
    pub published_collection_policy: CollectionPolicyObservation,
    pub pending_collection_reindex: Option<bool>,
    pub configured_file_profile: ConfiguredFileProfileObservation,
    pub retention_basis: String,
    pub live_source_checked: bool,
    pub families: Vec<FamilyObservation>,
}
'''+retention+'''
/// Pure bounded metadata projection. Retention means policy eligibility, not
/// observed presence/count/completeness. No callback or source reader is accepted.
pub fn report(file: FileRevision, profile: &ExtractionProfile,
    configured: CollectionPolicyObservation, published: CollectionPolicyObservation,
    support: &BTreeMap<Family, Support>, reply: &BTreeMap<Family, ReplyInclusion>) -> Result<ProfileFamilyReport> {
    ensure!(!file.path.is_empty() && file.path.len()<=8192 && !file.path.contains('\\0'),"invalid family report file identity");
    ensure!([&profile.adapter_id,&profile.adapter_version].into_iter().all(|value|!value.is_empty() && value.len()<=128 && value.bytes().all(|byte|byte.is_ascii_graphic())),"invalid bounded adapter identity");
    let detail=IndexDetail::from_profile(profile)?;
    let value=ProfileFamilyReport {
        schema:"airk.profile-families/1".into(),basis:"metadata_only".into(),file,
        applied_file:AppliedFileProfile {adapter_id:profile.adapter_id.clone(),adapter_version:profile.adapter_version.clone(),effective_profile_digest:profile.identity()?,legacy_detail:detail,legacy_detail_was_absent:!profile.options.contains_key("index_detail")},
        configured_collection_policy:configured,published_collection_policy:published,
        pending_collection_reindex:configured.known().zip(published.known()).map(|(configured,published)|configured!=published),
        configured_file_profile:ConfiguredFileProfileObservation::UnknownSourceInputsMayBeRequired,
        retention_basis:"policy_eligibility_not_record_presence_or_completeness".into(),live_source_checked:false,
        families:Family::ALL.into_iter().map(|family|FamilyObservation {family,support:support.get(&family).copied().unwrap_or(Support::Unknown),retention:retention(family,detail),reply_inclusion:reply.get(&family).copied().unwrap_or(ReplyInclusion::Unknown)}).collect(),
    };
    ensure!(serde_json::to_vec(&value)?.len()<=32768,"complete family report exceeds bound");
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Incarnation;
    fn profile(detail:Option<IndexDetail>)->ExtractionProfile {
        ExtractionProfile {contract_version:crate::extraction::EXTRACTION_CONTRACT_VERSION,adapter_id:"fixture".into(),adapter_version:"1".into(),language:"text".into(),dialect:None,grammar_identity:None,mapping_version:1,chunk_version:1,
            options:detail.map(|detail|BTreeMap::from([("index_detail".into(),detail.as_str().into())])).unwrap_or_default(),configuration_inputs:BTreeMap::new()}
    }
    fn file()->FileRevision {FileRevision {publication:RevisionToken {incarnation:Incarnation::from_bytes([1;16]),generation:1,revision:Sha256Digest::from_bytes([2;32])},path:"manifest.toml".into(),source_hash:Sha256Digest::from_bytes([3;32])}}
    fn row(value:&ProfileFamilyReport,family:Family)->&FamilyObservation {value.families.iter().find(|row|row.family==family).unwrap()}
    #[test]
    fn all_legacy_details_keep_support_retention_and_actual_reply_independent()->Result<()> {
        for detail in [IndexDetail::Essential,IndexDetail::AgentLean,IndexDetail::Balanced,IndexDetail::Detailed] {
            for reply in [None,Some(ReplyInclusion::Selected),Some(ReplyInclusion::Omitted)] {
                let configured=CollectionPolicyRevision {detail:IndexDetail::Detailed,policy_digest:Sha256Digest::from_bytes([4;32])};
                let published=CollectionPolicyRevision {detail,policy_digest:Sha256Digest::from_bytes([5;32])};
                let mut replies=reply.map(|reply|BTreeMap::from([(Family::TablesRows,reply)])).unwrap_or_default();
                replies.insert(Family::ManifestPackagesDependencies,ReplyInclusion::Omitted);
                let value=report(file(),&profile(Some(detail)),configured.into(),published.into(),&BTreeMap::from([(Family::TablesRows,Support::Supported),(Family::ManifestPackagesDependencies,Support::Supported)]),&replies)?;
                assert_eq!(value.families.len(),18);
                assert_eq!(value.applied_file.effective_profile_digest,profile(Some(detail)).identity()?);
                assert_eq!(value.configured_collection_policy.known().unwrap().policy_digest,configured.policy_digest);
                assert_eq!(value.pending_collection_reindex,Some(true));
                assert_eq!(row(&value,Family::TablesRows).retention,if detail.retains_fact("table") {Retention::Retained}else{Retention::NotRetained});
                assert_eq!(row(&value,Family::TablesRows).support,Support::Supported);
                assert_eq!(row(&value,Family::TablesRows).reply_inclusion,reply.unwrap_or(ReplyInclusion::Unknown));
                assert_eq!(row(&value,Family::ManifestPackagesDependencies).retention,if detail.retains_fact("package_dependency") {Retention::Retained}else{Retention::NotRetained});
                assert_eq!(row(&value,Family::ManifestPackagesDependencies).support,Support::Supported);
                assert_eq!(row(&value,Family::ManifestPackagesDependencies).reply_inclusion,ReplyInclusion::Omitted);
                assert_eq!(row(&value,Family::StandaloneDatesBounds).retention,if detail.retains_fact("date") {Retention::Retained}else{Retention::NotRetained});
                assert_eq!(row(&value,Family::DeclarationContinuity).retention,if detail.tracks_continuity(){Retention::Selected}else{Retention::NotRetained});
                assert_eq!(row(&value,Family::DocumentUnitContinuity).retention,if detail==IndexDetail::Detailed{Retention::Selected}else{Retention::NotRetained});
                for family in [Family::Vectors,Family::MappedRegions,Family::ImportedAssertions,Family::OcrTextFacts] {assert_eq!(row(&value,family).support,Support::Unknown);assert_eq!(row(&value,family).retention,Retention::Unknown);}
            }
        }
        Ok(())
    }
    #[test]
    fn missing_historical_detail_is_detailed_without_rewriting_profile_identity()->Result<()> {
        let profile=profile(None);
        let policy=CollectionPolicyRevision {detail:IndexDetail::AgentLean,policy_digest:Sha256Digest::from_bytes([9;32])};
        let value=report(file(),&profile,policy.into(),policy.into(),&BTreeMap::new(),&BTreeMap::new())?;
        assert_eq!(value.applied_file.legacy_detail,IndexDetail::Detailed);
        assert!(value.applied_file.legacy_detail_was_absent);
        assert_eq!(value.applied_file.effective_profile_digest,profile.identity()?);
        assert_eq!(value.pending_collection_reindex,Some(false));
        Ok(())
    }
    #[test]
    fn unavailable_collection_policy_preserves_file_rows_without_inventing_pending_state()->Result<()> {
        let profile=profile(Some(IndexDetail::AgentLean));
        let unavailable=CollectionPolicyObservation::Unavailable {reason:CollectionPolicyUnavailable::SelectedScope};
        let known=CollectionPolicyRevision {detail:IndexDetail::Detailed,policy_digest:Sha256Digest::from_bytes([8;32])};
        let value=report(file(),&profile,unavailable,known.into(),&BTreeMap::new(),&BTreeMap::new())?;
        assert_eq!(value.pending_collection_reindex,None);
        assert_eq!(value.configured_collection_policy,unavailable);
        assert_eq!(value.applied_file.effective_profile_digest,profile.identity()?);
        assert_eq!(value.families.len(),18);
        Ok(())
    }
}
'''
changes['crates/airk-core/src/profile_families.rs']=['',core]
name='crates/airk-core/src/lib.rs'; text=read(name); text=replace(text,'pub mod publication;','pub mod profile_families;\npub mod publication;'); changes[name][1]=text
name='crates/airk-kernel/src/lib.rs'; text=read(name)
text=replace(text,'pub struct FileInfo {','pub struct FileInfo {\n    #[serde(skip_serializing_if="Option::is_none")]\n    pub profile_families: Option<airk_core::profile_families::ProfileFamilyReport>,')
text=replace(text,'    pub fn file_info(&self, path: &str) -> Result<FileInfo> {\n        self.current_file_info(self.current_reader()?.as_ref(), path)\n    }','''    pub fn file_info(&self, path: &str) -> Result<FileInfo> {
        self.file_info_with_profile_families(path,false)
    }
    pub fn file_info_with_profile_families(&self,path:&str,include_profile_families:bool)->Result<FileInfo> {
        self.current_file_info_with_profile_families(self.current_reader()?.as_ref(),path,include_profile_families)
    }''')
text=replace(text,'mod extraction;','mod extraction;\nmod profile_families;'); changes[name][1]=text
name='crates/airk-kernel/src/current/runtime.rs'; text=read(name)
needle='''    pub(crate) fn current_file_info(&self, read: &CurrentRead, path: &str) -> Result<FileInfo> {
        let artifact'''
text=replace(text,needle,'''    pub(crate) fn current_file_info(&self, read: &CurrentRead, path: &str) -> Result<FileInfo> {
        self.current_file_info_with_profile_families(read,path,false)
    }
    pub(crate) fn current_file_info_with_profile_families(&self,read:&CurrentRead,path:&str,include_profile_families:bool)->Result<FileInfo> {
        let artifact''')
text=replace(text,'        Ok(FileInfo {','''        let profile_families=if include_profile_families {
            Some(self.current_profile_families(read,path,&artifact.content_hash,profile,&state.configuration)?)
        } else {None};
        Ok(FileInfo {
            profile_families,'''); changes[name][1]=text
name='crates/airk-kernel/src/current/conditional.rs'; text=read(name)
needle='''    ) -> Result<ConditionalRead<FileInfo>> {
        let pinned = self.current_reader()?;'''
text=replace(text,needle,'''    ) -> Result<ConditionalRead<FileInfo>> {
        self.file_info_conditional_with_profile_families(path,options,false)
    }
    pub fn file_info_conditional_with_profile_families(&self,path:&str,options:&EvidenceReadOptions,include_profile_families:bool)->Result<ConditionalRead<FileInfo>> {
        let pinned = self.current_reader()?;''')
text=replace(text,'''        let payload = self.current_file_info(&pinned, &path)?;
        self.current_conditional(&pinned, read, options, "file_info/1", payload)''','''        let payload = self.current_file_info_with_profile_families(&pinned,&path,include_profile_families)?;
        if include_profile_families {
            self.current_conditional(&pinned,read,options,("file_info/profile_families/1",true),payload)
        } else {
            self.current_conditional(&pinned,read,options,"file_info/1",payload)
        }'''); changes[name][1]=text
kernel='''// SPDX-License-Identifier: Apache-2.0
//! Per-file metadata projection; no source selection/extraction/producer executes.
use crate::Kernel;
use airk_core::{extraction::ExtractionProfile,profile_families::{self,CollectionPolicyRevision,CollectionPolicyObservation,CollectionPolicyUnavailable,Family,FileRevision,ProfileFamilyReport,ReplyInclusion,Support}};
use airk_store::current::{CurrentRead,collection::CollectionConfiguration};
use anyhow::Result;
use std::collections::BTreeMap;
impl Kernel {
    pub(crate) fn current_profile_families(&self,read:&CurrentRead,path:&str,hash:&str,profile:&ExtractionProfile,configuration:&CollectionConfiguration)->Result<ProfileFamilyReport> {
        let registry=airk_ingest::adapters::registry::AdapterRegistry::baseline();
        // A permitted per-file report does not grant whole-index settings.
        // Only this typed scope refusal becomes an unavailable observation;
        // I/O/integrity/revocation errors retain their normal failure behavior.
        let (configured,published)=match self.require_index_metadata(read) {
            Ok(())=>{
                let policy=match self.store.extraction_policy_bytes(&self.root_id())? {
                    Some(bytes)=>crate::extraction::ExtractionPolicy::parse(&bytes,&registry)?,
                    None=>crate::extraction::ExtractionPolicy {version:3,detail:Some(configuration.index_detail),rules:vec![]},
                };
                (CollectionPolicyObservation::from(CollectionPolicyRevision {detail:policy.detail.unwrap_or_default(),policy_digest:crate::extraction::policy_digest(&policy)?}),
                 CollectionPolicyObservation::from(CollectionPolicyRevision {detail:configuration.index_detail,policy_digest:configuration.extraction_policy_digest}))
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>()=>{
                let unavailable=CollectionPolicyObservation::Unavailable {reason:CollectionPolicyUnavailable::SelectedScope};
                (unavailable,unavailable)
            }
            Err(error)=>return Err(error),
        };
        let mut support=BTreeMap::from([(Family::SavedIdentity,Support::Supported)]);
        if let Some(descriptor)=registry.get(&profile.adapter_id).filter(|descriptor|descriptor.profile.adapter_version==profile.adapter_version) {
            let observed=|value|if value {Support::Supported}else{Support::Unsupported};
            support.insert(Family::LexicalCandidates,observed(descriptor.capabilities.lexical));
            support.insert(Family::Declarations,observed(descriptor.capabilities.definitions));
            support.insert(Family::Imports,observed(descriptor.capabilities.imports));
            support.insert(Family::WrittenCalls,observed(descriptor.capabilities.call_candidates));
        }
        // FileInfo carries metadata/counts, never underlying facts/text/vectors.
        // Final renderer identity projection is unobserved here; keep it unknown.
        let reply=Family::ALL.into_iter().filter(|family|*family!=Family::SavedIdentity).map(|family|(family,ReplyInclusion::Omitted)).collect();
        profile_families::report(FileRevision {publication:crate::current::runtime::revision(read.token()?),path:path.into(),source_hash:hash.parse()?},profile,
            configured,published,&support,&reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{evidence::{EvidenceReadOptions,EvidenceReadStatus},extraction::IndexDetail};
    use anyhow::Context;
    #[test]
    fn native_per_file_reports_preserve_applied_identity_after_config_change_and_source_loss()->Result<()> {
        let temporary=tempfile::tempdir()?;
        let base=temporary.path().canonicalize()?;
        let root=base.join("source");let data=base.join("data");std::fs::create_dir(&root)?;
        for (path,bytes) in [("code.rs","pub fn retained() {}\\n"),("Cargo.toml","[package]\\nname=\\"fixture\\"\\nversion=\\"0.1.0\\"\\n"),("table.md","| key | value |\\n| --- | --- |\\n| one | two |\\n"),("requirements.md","# Requirements\\nREQ-1: retained metadata must preserve identity.\\n")] {std::fs::write(root.join(path),bytes)?;}
        let kernel=Kernel::open_service_collection(&root,Some(data),"families")?;
        kernel.configure_vectors(false)?;
        kernel.configure_index_detail(IndexDetail::AgentLean)?;
        kernel.index_service_sources()?;
        let pinned=kernel.current_reader()?;
        let expected=pinned.artifact("Cargo.toml")?.context("manifest absent")?.retained.context("manifest retained metadata absent")?.profile.context("manifest profile absent")?.identity()?;
        let publication=crate::current::runtime::revision(pinned.token()?);drop(pinned);
        let legacy=kernel.file_info_conditional("Cargo.toml",&Default::default())?;
        assert!(serde_json::to_value(legacy.content.as_ref().unwrap())?.get("profile_families").is_none());
        let configured=kernel.configure_index_detail(IndexDetail::Detailed)?;
        let quota=kernel.collection_quota()?;let before=quota.status()?;
        let mut profiles=std::collections::BTreeSet::new();
        for path in ["code.rs","Cargo.toml","table.md","requirements.md"] {
            let info=kernel.file_info_with_profile_families(path,true)?;
            let report=info.profile_families.context("opt-in report absent")?;
            assert_eq!(report.file.publication,publication);
            assert_eq!(report.applied_file.legacy_detail,IndexDetail::AgentLean);
            assert_eq!(report.configured_collection_policy.known().unwrap().policy_digest,configured.digest);
            assert_eq!(report.configured_collection_policy.known().unwrap().detail,IndexDetail::Detailed);
            assert_eq!(report.pending_collection_reindex,Some(true));
            assert_eq!(report.families.len(),18);
            assert!(!report.live_source_checked);
            profiles.insert(report.applied_file.effective_profile_digest);
        }
        assert!(profiles.len()>=2,"mixed adapters must retain independent file profiles");
        let changed=kernel.file_info_conditional_with_profile_families("Cargo.toml",&EvidenceReadOptions {if_evidence:legacy.read.validator,..Default::default()},true)?;
        assert_eq!(changed.read.status,EvidenceReadStatus::Content);
        assert_eq!(changed.content.as_ref().unwrap().profile_families.as_ref().unwrap().applied_file.effective_profile_digest,expected);
        let unchanged=kernel.file_info_conditional_with_profile_families("Cargo.toml",&EvidenceReadOptions {if_evidence:changed.read.validator,..Default::default()},true)?;
        assert_eq!(unchanged.read.status,EvidenceReadStatus::NotModified);
        std::fs::rename(&root,base.join("unavailable-originals"))?;
        let missing=kernel.file_info_conditional_with_profile_families("Cargo.toml",&Default::default(),true)?;
        assert_eq!(serde_json::to_value(missing.content)?,serde_json::to_value(changed.content)?);
        let after=quota.status()?;
        assert_eq!(before.used_bytes,after.used_bytes);assert_eq!(before.reserved_bytes,after.reserved_bytes);
        Ok(())
    }
}
'''
scoped_test='''
    #[test]
    fn authorized_partial_scope_report_skips_private_policy_bytes_and_keeps_file_identity()->Result<()> {
        use airk_core::{identity::{Incarnation,Sha256Digest},index_catalog::*,grant_provenance::GrantProvenance,scoped_reads::{ScopeReadRefused,SourceDescriptor}};
        let temporary=tempfile::tempdir()?;let base=temporary.path().canonicalize()?;
        let root=base.join("source");let data=base.join("data");std::fs::create_dir_all(root.join("public"))?;
        std::fs::write(root.join("public/one.rs"),"pub struct Authorized;\\n")?;
        std::fs::write(root.join("secret.rs"),"pub struct OutsideGrant;\\n")?;
        let kernel=Kernel::open_service_collection(&root,Some(data),"scoped-families")?;
        kernel.configure_vectors(false)?;kernel.index_service_sources()?;
        let read=kernel.current_reader()?;let token=crate::current::runtime::revision(read.token()?);
        let artifact=read.artifact("public/one.rs")?.context("authorized artifact absent")?;
        let expected=artifact.retained.as_ref().and_then(|value|value.profile.as_ref()).context("profile absent")?.identity()?;
        let hash:Sha256Digest=artifact.content_hash.parse()?;drop(read);
        let project=QueryProjectId::from_identity(Incarnation::from_bytes([1;16]));
        let index=IndexId::from_identity(Incarnation::from_bytes([2;16]));
        let source=SourceRootId::from_identity(Incarnation::from_bytes([3;16]));
        let authority=crate::scoped_reads::ScopedReadAuthority {
            schema:1,backing:airk_core::service::ProjectRegistration::new(kernel.root_id(),kernel.data_root.to_str().unwrap().into(),kernel.collection.clone())?.binding,
            index_id:index,project_ids:vec![project],provenance:GrantProvenance::UserReviewedProjectGrant,
            grant:ReviewedGrant {id:GrantId::from_identity(Incarnation::from_bytes([4;16])),host:Sha256Digest::from_bytes([5;32]),revision:CatalogRevision::INITIAL,revoked:false,
                memberships:vec![GrantedMembership {membership:Membership {project_id:project,index_id:index,revision:CatalogRevision::INITIAL,scope:vec![ScopeAtom {source_root_id:source,kind:PathScope::Exact,path:"public/one.rs".into()}]},source_revision:CatalogRevision::INITIAL,policy_epoch:0,policy_digest:None}]},
            sources:vec![SourceDescriptor {source_root_id:source,original_root:kernel.root_id()}],
        };
        let scoped=kernel.with_scoped_reads(authority)?;
        // Any attempt to read private collection settings now fails, rather
        // than returning plausible metadata. The permitted per-file path must
        // never enter that branch under a partial Project grant.
        std::fs::write(kernel.store.directory().join("extraction-policy.json"),b"malformed private policy metadata")?;
        let full_error=kernel.file_info_with_profile_families("public/one.rs",true).err().context("real private metadata error was downgraded")?;
        assert!(!full_error.is::<ScopeReadRefused>());
        std::fs::rename(&root,base.join("unavailable-originals"))?;
        let options=EvidenceReadOptions {revision:Some(token),known_hash:Some(hash),..Default::default()};
        let basic=scoped.file_info_conditional("public/one.rs",&options)?;
        let result=scoped.file_info_conditional_with_profile_families("public/one.rs",&EvidenceReadOptions {if_evidence:basic.read.validator,..options.clone()},true)?;
        assert_eq!(result.read.status,EvidenceReadStatus::Content);
        let report=result.content.as_ref().and_then(|value|value.profile_families.as_ref()).context("authorized report suppressed")?;
        let unavailable=CollectionPolicyObservation::Unavailable {reason:CollectionPolicyUnavailable::SelectedScope};
        assert_eq!(report.configured_collection_policy,unavailable);assert_eq!(report.published_collection_policy,unavailable);
        assert_eq!(report.pending_collection_reindex,None);
        assert_eq!(report.applied_file.effective_profile_digest,expected);
        assert_eq!(report.file.publication,token);assert_eq!(report.file.source_hash,hash);assert_eq!(report.file.path,"public/one.rs");
        assert_eq!(report.families.len(),18);assert!(!report.live_source_checked);
        let repeated=scoped.file_info_conditional_with_profile_families("public/one.rs",&EvidenceReadOptions {if_evidence:result.read.validator,..options},true)?;
        assert_eq!(repeated.read.status,EvidenceReadStatus::NotModified);
        let error=scoped.file_info_with_profile_families("secret.rs",true).err().context("outside-grant file was reported")?;
        assert!(error.is::<ScopeReadRefused>());
        Ok(())
    }
'''
kernel=kernel.rstrip()[:-1]+scoped_test+'}\n'
changes['crates/airk-kernel/src/profile_families.rs']=['',kernel]
scratch=base/'p11a-corrected-report-formatted';scratch.mkdir(exist_ok=True);copies=[]
for name,(_old,new) in changes.items():
    copy=scratch/(name.split('/')[1]+'-'+Path(name).name);copy.write_text(new,encoding='utf-8');copies.append(copy)
options=[str(Path.home()/'.cargo/bin/rustfmt.exe'),'--edition','2024','--config','skip_children=true']
subprocess.run([*options,*map(str,copies)],check=True,cwd=root);subprocess.run([*options,'--check',*map(str,copies)],check=True,cwd=root)
for (name,pair),copy in zip(changes.items(),copies):pair[1]=copy.read_text(encoding='utf-8')
patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name if old else '/dev/null',tofile='b/'+name)) for name,(old,new) in changes.items())
(base/'p11a-corrected-report-review.patch').write_text(patch,encoding='utf-8')
(base/'p11a-corrected-report-review-base.json').write_text(json.dumps({name:hashlib.sha256((root/name).read_bytes()).hexdigest() if old else None for name,(old,_new) in changes.items()},indent=2)+'\n',encoding='utf-8')
print('Prepared inert corrected P11a report:',len(changes),'files; no compiler inputs changed')
