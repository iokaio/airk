# SPDX-License-Identifier: Apache-2.0
"""Prepare an inert exact-funded-staging disposal inventory correction."""
import difflib,hashlib,json,subprocess
from pathlib import Path
base=Path(__file__).resolve().parent; root=base/'p10-worktree'; changes={}
def read(name):
    text=(root/name).read_text(encoding='utf-8'); changes[name]=[text,text]; return text
def replace(text,old,new):
    assert text.count(old)==1,old
    return text.replace(old,new)
name='crates/airk-store/src/quota/funding.rs'; text=read(name)
needle='''    /// Exact native claim/writer custody may close dead capture handles before'''
new='''    /// Exact deferred staging inventory for a canceled participant. Recognition
    /// is not deletion authority: only native disposed-current reconciliation
    /// may remove these bytes or release their still-held capacity.
    pub(crate) fn operation_staging_for_disposal(&self, owner: &FundingUse) -> Result<std::collections::BTreeSet<PathBuf>> {
        let record = self.observe_funding(owner.operation_id)?;
        check_use(&record, owner)?;
        ensure!(owner.phase.purpose == SplitJobPurpose::DestinationCleanup
            && owner.index_id == record.request.destination_index, "staging disposal requires exact destination cleanup");
        let registration = record.request.registration(owner.index_id).context("staging participant absent")?;
        let mut paths = std::collections::BTreeSet::new();
        for child in record.children.iter().filter(|child| child.index_id == owner.index_id) {
            if let FundingChildKind::Reservation { staging_name } = &child.kind {
                ensure!(child.closed && child.phase == owner.phase, "funded staging still has an open or foreign capture");
                paths.insert(staging_path(self, &registration.collection, &self.control_path().join("reservations").join(staging_name))?);
            }
        }
        ensure!(paths.len() <= MAX_CHILDREN, "funded staging inventory exceeds bound");
        Ok(paths)
    }

'''+needle
text=replace(text,needle,new); changes[name][1]=text
name='crates/airk-store/src/service_collection_purge.rs'; text=read(name)
needle='''        let _usage = crate::usage::deletion_guard(&directory)?;
        collection.sweep_metadata_scratch()?;'''
new='''        // A dead funded capture's exact stage remains owned until the native
        // payload-disposal checkpoint below. Recognize ONLY the admitted ledger
        // paths; do not classify a prefix as safe or delete staging early.
        let admitted_staging = if let Some(funding) = funding {
            let permit = split_access.context("funded staging disposal requires claimed cleanup access")?;
            let store = FileStore::new(&directory).with_split_access(permit.clone())?.with_operation_funding(funding.clone())?;
            Quota::for_store(&store)?.operation_staging_for_disposal(funding)?
        } else {
            std::collections::BTreeSet::new()
        };
        let _usage = crate::usage::deletion_guard(&directory)?;
        collection.sweep_metadata_scratch()?;'''
text=replace(text,needle,new)
needle='''            if name.starts_with("split-state-") && retained_split {'''
new='''            if admitted_staging.contains(&path) {
                ensure!(plain(&path)?.is_dir(), "invalid exact funded staging directory");
                continue;
            }
'''+needle
text=replace(text,needle,new)
needle='''        let gates = directory.join(".current-gates");'''
new='''        // Last-incarnation disposal normally removed these through canonical
        // journal/quota reconciliation on the prior turn. Never report complete
        // from a missing incarnation if its actual funded staging still exists.
        for stage in &admitted_staging {
            match fs::symlink_metadata(stage) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => anyhow::bail!("admitted funded staging awaits native disposal settlement"),
            }
        }
'''+needle
text=replace(text,needle,new); changes[name][1]=text
name='crates/airk-cli/src/project_worker_split.rs'; text=read(name)
needle='''        let observer =
            Kernel::open_existing_service_observer(&building.request.destination_registration)?;
        let outcome = execute('''
if text.count(needle)!=1:
    # Parent rustfmt may keep the constructor on one line.
    needle='''        let observer = Kernel::open_existing_service_observer(&building.request.destination_registration)?;
        let outcome = execute('''
assert text.count(needle)==1
new='''        // A same-prefix entry with no exact admitted child remains unowned.
        // Preflight must preserve both it and all current recovery evidence.
        let logical = staging.parent().context("crash stage collection absent")?;
        let unknown = logical.join(format!(".quota-stage-r-funded-{}",Sha256Digest::from_bytes([0;32])));
        assert_ne!(unknown,staging);
        std::fs::create_dir(&unknown)?;
        std::fs::write(unknown.join("foreign-source"),b"unadmitted entry remains unchanged")?;
        let native_current = airk_store::service::collection::Collection::existing(
            Path::new(&building.request.destination_registration.data_root),
            &building.request.destination_registration.root,&building.request.destination_registration.collection,
        )?.select_for_diagnostics()?.directory().join("current.sqlite3");
        assert!(native_current.is_file());
        let observer = Kernel::open_existing_service_observer(&building.request.destination_registration)?;
        let error=execute(&observer,&mut fixture.registry,&lease,cleanup,fixture.engine,&fixture.control).err().context("unadmitted staging was accepted")?;
        assert!(format!("{error:#}").contains("unknown removal entry is preserved"),"{error:#}");
        assert!(native_current.is_file(),"unknown staging must refuse before payload deletion");
        assert_eq!(std::fs::read(unknown.join("foreign-source"))?,b"unadmitted entry remains unchanged");
        assert_eq!(std::fs::read(staging.join("owned-sentinel"))?,b"preserved crash evidence");
        std::fs::remove_file(unknown.join("foreign-source"))?;
        std::fs::remove_dir(&unknown)?;
        let outcome = execute('''
text=replace(text,needle,new); changes[name][1]=text
scratch=base/'p10-funded-stage-purge-formatted'; scratch.mkdir(exist_ok=True)
copies=[]
for name,(_old,new) in changes.items():
    copy=scratch/Path(name).name; copy.write_text(new,encoding='utf-8'); copies.append(copy)
options=[str(Path.home()/'.cargo/bin/rustfmt.exe'),'--edition','2024','--config','skip_children=true']
subprocess.run([*options,*map(str,copies)],check=True,cwd=root)
subprocess.run([*options,'--check',*map(str,copies)],check=True,cwd=root)
for (name,pair),copy in zip(changes.items(),copies): pair[1]=copy.read_text(encoding='utf-8')
patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name,tofile='b/'+name)) for name,(old,new) in changes.items())
(base/'p10-funded-stage-purge-review.patch').write_text(patch,encoding='utf-8')
(base/'p10-funded-stage-purge-review-base.json').write_text(json.dumps({name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in changes},indent=2)+'\n',encoding='utf-8')
print('Prepared inert funded-stage purge patch:',len(changes),'files; no compiler inputs changed')
