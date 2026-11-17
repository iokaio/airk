# SPDX-License-Identifier: Apache-2.0
"""Prepare an inert, reviewable diff. Never mutate compiler inputs."""
import difflib,hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parent
root=base/'p10-worktree'
changes={}
def source(name):
    text=(root/name).read_text(encoding='utf-8')
    changes[name]=[text,text]
    return text
def save(name,text): changes[name][1]=text

name='crates/airk-store/src/current/postings.rs'
text=source(name)
needle='''        ensure!(
            prefix.len() <= 8192'''
replacement='''        self.lookup_paths_filtered(lookup, prefix, after, limit, |_| Ok(true))
    }
    /// Additional immutable candidate selection, after existing read/policy
    /// authority and before bounded pagination. Predicate errors refuse the
    /// lookup; they never establish an empty result. The predicate must not
    /// reenter this reader or read live sources.
    pub fn lookup_paths_filtered(
        &self,
        lookup: Lookup<'_>,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
        mut select: impl FnMut(&str) -> Result<bool>,
    ) -> Result<PathPage> {
        ensure!(
            prefix.len() <= 8192'''
assert text.count(needle)==1
text=text.replace(needle,replacement)
needle='''                    && after.is_none_or(|after| path.as_str() > after)
                {'''
assert text.count(needle)==1
text=text.replace(needle,'''                    && after.is_none_or(|after| path.as_str() > after)
                    && select(&path)?
                {''')
save(name,text)

name='crates/airk-kernel/src/split.rs'
text=source(name).replace('use airk_core::{discovery::DiscoveryOptions, verification::SourceCheckState};','use airk_core::verification::SourceCheckState;')
text=text.replace('use anyhow::{Result, ensure};','use anyhow::{Context, Result, ensure};')
needle='''        let expected = read.token()?;
        let deadline'''
assert text.count(needle)==1
text=text.replace(needle,'''        let expected = read.token()?;
        // Frozen native source projection from this exact retained publication;
        // never consult a mutable configured root or change PolicyRuntime.
        let configuration = read.collection_state()?.context("split witness lacks retained configuration")?.configuration;
        let primary = read.head().scope.root.clone();
        let deadline''')
start=text.index('            let page = self.list_files_page(')
end=text.index('            for path in &page.paths {',start)
text=text[:start]+'''            let page = read.lookup_paths_filtered(
                airk_store::current::postings::Lookup::Files,
                "",
                cursor.as_deref(),
                (limits.max_paths - examined).min(32),
                |path| {
                    let source = airk_store::roots::source_location(&primary, &configuration.roots, path)?;
                    let root = snapshot.baseline.roots.iter()
                        .find(|root| root.canonical_path == source.root)
                        .context("split witness retained source absent from frozen baseline")?;
                    Ok(snapshot.plan.remainder.allows_canonical(root.id, &source.relative_path))
                },
            )?;
            ensure!(Instant::now() < deadline, "split remainder witness budget exhausted; nonempty state unknown");
            let mut page_bytes = 0usize;
            let mut last_examined = cursor.clone();
            let mut byte_limited = false;
'''+text[end:]
needle='''            for path in &page.paths {
                examined += 1;'''
assert text.count(needle)==1
text=text.replace(needle,'''            for path in &page.paths {
                // Preserve the previous discovery page's 16KiB record budget.
                let record_bytes = serde_json::to_vec(path)?.len();
                ensure!(record_bytes <= 16384, "split witness path exceeds page byte budget; nonempty state unknown");
                if record_bytes > 16384 - page_bytes {
                    byte_limited = true;
                    break;
                }
                page_bytes += record_bytes;
                last_examined = Some(path.clone());
                examined += 1;''')
needle='''            cursor = page.page.cursor;
            if !page.truncated || cursor.is_none() {'''
assert text.count(needle)==1
text=text.replace(needle,'''            cursor = if byte_limited { last_examined } else { page.next_after };
            if cursor.is_none() {''')
save(name,text)

name='crates/airk-store/src/current/postings_tests.rs'
text=source(name)
needle='''    let page = read.lookup_paths(Lookup::Files, "", None, 1)?;
    assert_eq!(page.paths, ["src/a.txt"]);'''
assert text.count(needle)==1
text=text.replace(needle,'''    let selected = read.lookup_paths_filtered(Lookup::Files, "", None, 1, |path| Ok(path == "src/b.txt"))?;
    assert_eq!(selected.paths, ["src/b.txt"]);
    assert!(selected.next_after.is_none());
    assert!(read.lookup_paths_filtered(Lookup::Files, "", None, 1, |_| Ok(false))?.paths.is_empty());
    assert!(read.lookup_paths_filtered(Lookup::Files, "", None, 1, |_| anyhow::bail!("candidate projection refused")).is_err());
    let page = read.lookup_paths(Lookup::Files, "", None, 1)?;
    assert_eq!(page.paths, ["src/a.txt"]);''')
needle='''    drop(read);
    let writer = fixture.store.lock()?;'''
position=text.index(needle,text.index('fn catalog_postings_follow_replacement_deletion_and_exact_case'))
text=text[:position]+'''    read.attach_read_scope(std::sync::Arc::new(airk_core::scoped_reads::CompiledReadScope {
        fingerprint: airk_core::identity::Sha256Digest::from_bytes([90; 32]),
        atoms: vec![airk_core::scoped_reads::CompiledPathAtom { kind: airk_core::index_catalog::PathScope::Exact, path: "src/a.txt".into() }],
        source_atoms: vec![], external_aliases: vec![],
    }))?;
    let mut examined = Vec::new();
    assert!(read.lookup_paths_filtered(Lookup::Files, "", None, 1, |path| {
        examined.push(path.to_owned());
        Ok(path == "src/b.txt")
    })?.paths.is_empty());
    assert_eq!(examined, ["src/a.txt"], "existing read scope must run before candidate selection");
'''+text[position:]
save(name,text)

name='crates/airk-cli/src/catalog_split_control_tests.rs'
text=source(name)
needle='''    pub(crate) temporary: tempfile::TempDir,
'''
assert text.count(needle)==1
text=text.replace(needle,'')
needle='''    pub(crate) engine: Sha256Digest,
}'''
assert text.count(needle)==1
text=text.replace(needle,'''    pub(crate) engine: Sha256Digest,
    // Rust drops fields in declaration order: close SQLite before TempDir.
    pub(crate) temporary: tempfile::TempDir,
}''')
save(name,text)

name='crates/airk-cli/src/project_worker_split.rs'
text=source(name)
assert text.rstrip().endswith('}')
test='''
    #[test]
    fn actual_witness_reaches_remainder_after_more_than_256_selected_prefix_files() -> Result<()> {
        let mut fixture = crate::catalog_split_control::tests::native_fixture(false)?;
        let root = Path::new(&fixture.original.root);
        for index in 0..300 {
            std::fs::write(root.join(format!("docs/selected-{index:04}.md")), format!("selected document {index}\\n"))?;
        }
        let original = Kernel::open_service_collection(root, Some(Path::new(&fixture.original.data_root).to_path_buf()), &fixture.original.collection)?;
        original.configure_vectors(false)?;
        original.index()?;
        let original_token = original.status()?.publication.context("original publication absent")?.token;
        drop(original);
        let response = crate::catalog_split_control::execute(&mut fixture.registry, &fixture.control,
            &airk_core::service::ControlMethod::CatalogSplitDraft { selection: Box::new(fixture.selection.clone()) })?
            .context("canonical split draft route missing")?;
        let airk_core::service::ControlResponse::CatalogSplitDraft { draft } = response else { anyhow::bail!("draft response differs") };
        let response = crate::catalog_split_control::execute(&mut fixture.registry, &fixture.control,
            &airk_core::service::ControlMethod::CatalogSplitApply { draft })?.context("canonical apply missing")?;
        let airk_core::service::ControlResponse::CatalogSplitAccepted { snapshot } = response else { anyhow::bail!("apply response differs") };
        let operation = snapshot.receipt.operation_id;
        let mut building = None;
        for _ in 0..12 {
            let current = fixture.registry.catalog_split_status(operation)?.context("split disappeared")?;
            if current.receipt.stage == SplitStage::Building { building=Some(current); break; }
            crate::catalog_split_control::advance_pending(&mut fixture.registry, &fixture.control, fixture.engine, operation)?;
        }
        let building=building.context("split never reached actual build")?;
        let build=building.receipt.jobs.last().context("build job missing")?.job;
        let destination=&building.request.destination_registration;
        let mut destination_lease=ProjectLease::acquire(&fixture.control,destination.binding)?;
        fixture.registry.admit_worker(&mut destination_lease)?;
        fixture.registry.claim_job_id(&destination_lease,build)?.context("build not claimable")?;
        let observer=Kernel::open_existing_service_observer(destination)?;
        let outcome=execute(&observer,&mut fixture.registry,&destination_lease,build,fixture.engine,&fixture.control)?;
        fixture.registry.complete_job(&destination_lease,build,outcome)?;
        let validated=fixture.registry.catalog_complete_split_job(operation,building.receipt_digest,build)?;
        assert_eq!(validated.receipt.stage,SplitStage::DestinationValidated);
        drop(observer);
        drop(destination_lease);
        crate::catalog_split_control::advance_pending(&mut fixture.registry,&fixture.control,fixture.engine,operation)?;
        let pending=fixture.registry.catalog_split_status(operation)?.context("witness state missing")?;
        let witness=pending.receipt.jobs.last().context("witness child missing")?;
        assert_eq!(witness.purpose,SplitJobPurpose::OriginalWitness);
        let mut lease=ProjectLease::acquire(&fixture.control,fixture.original.binding)?;
        fixture.registry.admit_worker(&mut lease)?;
        fixture.registry.claim_job_id(&lease,witness.job)?.context("witness not claimable")?;
        let original=Kernel::open_existing_service_observer(&fixture.original)?;
        let outcome=execute(&original,&mut fixture.registry,&lease,witness.job,fixture.engine,&fixture.control)?;
        let result: SplitJobResult=serde_json::from_value(fixture.registry.configuration_result(witness.job)?.context("opaque witness result absent")?)?;
        let proof=result.remainder_witness.context("positive native source proof absent")?;
        assert_eq!(proof.logical_path,"remainder.md");
        assert_eq!(proof.source_relative_path,"remainder.md");
        assert_eq!(proof.publication,original_token);
        assert_eq!(proof.job,Some(witness.job));
        assert!(proof.bytes_read>0);
        fixture.registry.complete_job(&lease,witness.job,outcome)?;
        let witnessed=fixture.registry.catalog_complete_split_job(operation,pending.receipt_digest,witness.job)?;
        assert_eq!(witnessed.receipt.remainder_witness,Some(proof));
        assert_eq!(original.status()?.publication.context("original publication disappeared")?.token,original_token);
        assert_eq!(std::fs::read(root.join("remainder.md"))?,b"original positive remainder\\n");
        drop(original);
        drop(lease);
        let temporary=fixture.temporary.path().to_path_buf();
        drop(fixture);
        assert!(!temporary.exists(),"SQLite handles must close before TempDir cleanup on Windows");
        Ok(())
    }
'''
text=text.rstrip()[:-1]+test+'}\n'
save(name,text)

patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name,tofile='b/'+name)) for name,(old,new) in changes.items())
(base/'p10-witness-filter-review.patch').write_text(patch,encoding='utf-8')
(base/'p10-witness-filter-review-base.json').write_text(json.dumps({name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in changes},indent=2)+'\n',encoding='utf-8')
print('Prepared inert patch:',len(changes),'files; no compiler inputs changed')
