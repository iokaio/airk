# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk'); base=root/'artifacts/v190/p10-worktree'
out=root/'artifacts/v190/p10-full-scope-policy-metadata-proposed'; out.mkdir(exist_ok=True)
paths=['crates/airk-store/src/current/read_scope.rs','crates/airk-kernel/src/scoped_reads.rs','crates/airk-kernel/src/current/runtime.rs','crates/airk-kernel/src/current/search.rs','crates/airk-kernel/src/references.rs']
original={p:(base/p).read_text(encoding='utf-8') for p in paths}; source=dict(original)
def replace(p,a,b,n=1):
    assert source[p].count(a)==n,(p,a,source[p].count(a));source[p]=source[p].replace(a,b)
replace(paths[0],'''            || self.read_scope.get().is_some()
                && self
                    .read_scope_restricts_paths
                    .get()
                    .copied()
                    .unwrap_or(true)
    }
''','''            || self.selected_scope_restricts_paths()
    }
    /// Selected Project coverage of this exact publication, independently of
    /// permanent source-policy filtering. Read scope attachment computes this
    /// once from the published root/alias configuration; it grants no path access.
    pub fn selected_scope_restricts_paths(&self) -> bool {
        self.read_scope.get().is_some()
            && self.read_scope_restricts_paths.get().copied().unwrap_or(true)
    }
''')
replace(paths[1],'read.read_scope_fingerprint() == Some(self.fingerprint) && !read.restricts_paths(),','''read.read_scope_fingerprint() == Some(self.fingerprint)
                && !read.selected_scope_restricts_paths(),''')
replace(paths[2],'let disclose_readiness = !read.restricts_paths();','let disclose_readiness = !read.selected_scope_restricts_paths();')
replace(paths[3],'(!read.restricts_paths()).then_some(semantic_readiness)','(!read.selected_scope_restricts_paths()).then_some(semantic_readiness)')
replace(paths[3],'(!read.restricts_paths())\n                            .then_some(discovery_readiness)','(!read.selected_scope_restricts_paths())\n                            .then_some(discovery_readiness)')
replace(paths[4],'if !read.restricts_paths() {','if !read.selected_scope_restricts_paths() {',2)
anchor='''    #[test]
    fn source_projection_uses_exact_retained_aliases_and_only_selected_sources() -> Result<()> {'''
test='''    #[test]
    fn full_project_metadata_and_readiness_survive_actual_permanent_deny() -> Result<()> {
        use airk_core::source_policy::{SourcePolicy, SourceRoot, DenyRule, DenyReason, POLICY_SCHEMA};
        let temporary = tempfile::tempdir()?;
        let source = temporary.path().join("source");
        std::fs::create_dir_all(source.join("public"))?;
        std::fs::create_dir_all(source.join("private"))?;
        let allowed = b"pub struct AllowedMetadata;\\n";
        let denied = b"pub struct HiddenMetadata;\\n";
        std::fs::write(source.join("public/main.rs"), allowed)?;
        std::fs::write(source.join("private/hidden.rs"), denied)?;
        let admin = Kernel::open_service_collection(&source, Some(temporary.path().join("data")), "policy-metadata")?;
        admin.configure_vectors(false)?;
        admin.index()?;
        let registration = airk_core::service::ProjectRegistration::new(admin.root_id(),
            admin.data_root.to_str().unwrap().into(), admin.collection.clone())?;
        let mut authority = fixture(&admin.root_id());
        authority.backing = registration.binding;
        let source_id = authority.sources[0].source_root_id;
        let policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(), index_id: authority.index_id,
            owner_binding: registration.binding, namespace: "collections-1.4".into(),
            collection: admin.collection.clone(), epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot { id: source_id, canonical_path: registration.root,
                aliases: vec![], owner: true }], positive_includes: None,
            denies: vec![DenyRule { source_root_id: source_id, kind: PathScope::Subtree,
                path: "private".into(), rule_id: DenyId::from_identity(Incarnation::from_bytes([91;16])),
                reason: DenyReason::Trim, created_epoch: CatalogRevision::INITIAL, inherited_from: None }],
        };
        policy.validate()?;
        let bound = admin.with_expected_source_policy(Some(policy.clone()))?;
        let application = bound.apply_source_policy(&policy, Incarnation::from_bytes([92;16]))?;
        assert_eq!(application.removed_artifacts, 1);
        assert!(application.removal_publication.is_some());
        authority.grant.memberships[0].policy_epoch = policy.epoch.get();
        authority.grant.memberships[0].policy_digest = Some(policy.identity()?);
        let full = bound.with_scoped_reads(authority.clone())?;
        let read = full.current_reader()?;
        assert!(read.restricts_paths(), "permanent deny must keep Store filtering active");
        assert!(!read.selected_scope_restricts_paths());
        assert!(read.require_path("private/hidden.rs").unwrap_err().is::<ScopeReadRefused>());
        let token = crate::current::runtime::revision(read.token()?);
        drop(read);
        let summary = full.status_summary()?;
        assert_eq!(summary["files"], 1);
        assert_eq!(summary["source_policy"]["control_dependencies"],
            serde_json::to_value(airk_ingest::permanent_policy::control_dependencies())?);
        assert_eq!(full.status()?.files, 1);
        assert!(full.roots()?.indexed.is_empty());
        assert!(full.scoped_index_metadata(registration.binding, ScopedIndexMetadataView::ExtractionPolicy).is_ok());
        assert!(full.readiness_observation(token)?.is_some());
        let query = airk_core::SearchQuery { text: "AllowedMetadata".into(), path_prefix: None,
            limit: 10, max_bytes: 4096 };
        let result = full.search(&query)?;
        assert!(result.semantic_readiness.is_some() && result.discovery_readiness.is_some());
        assert!(!full.list_files("", 16, 8192)?.paths.iter().any(|path| path.starts_with("private/")));
        let mut subset_authority = authority.clone();
        subset_authority.grant.memberships[0].membership.scope[0].path = "public".into();
        let subset = bound.with_scoped_reads(subset_authority)?;
        assert!(subset.current_reader()?.selected_scope_restricts_paths());
        assert!(subset.status_summary().unwrap_err().is::<ScopeReadRefused>());
        assert!(subset.roots().unwrap_err().is::<ScopeReadRefused>());
        let result = subset.search(&query)?;
        assert!(result.semantic_readiness.is_none() && result.discovery_readiness.is_none());
        assert!(subset.readiness_observation(token)?.is_none());
        let mut stale = authority;
        stale.grant.memberships[0].policy_epoch = 0;
        stale.grant.memberships[0].policy_digest = None;
        assert!(bound.with_scoped_reads(stale)?.status_summary().unwrap_err().is::<ScopeReadRefused>());
        full.revoke_scoped_reads();
        assert!(full.status_summary().unwrap_err().is::<ScopeReadRefused>());
        assert_eq!(std::fs::read(source.join("public/main.rs"))?, allowed);
        assert_eq!(std::fs::read(source.join("private/hidden.rs"))?, denied);
        Ok(())
    }

'''
replace(paths[1],anchor,test+anchor)
for p in paths:
    dest=out/p;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(source[p],encoding='utf-8',newline='\n')
(out/'baseline.json').write_text(json.dumps({p:hashlib.sha256((base/p).read_bytes()).hexdigest() for p in paths},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'out':str(out),'files':len(paths)}))
