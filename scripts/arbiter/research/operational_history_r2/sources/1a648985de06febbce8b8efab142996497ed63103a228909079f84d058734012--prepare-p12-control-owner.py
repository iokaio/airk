# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
b=Path(__file__).resolve().parent;w=b/'p10-worktree';out=b/'p12-control-owner-proposal';out.mkdir(exist_ok=True);originals={}
def read(p):
 raw=(w/p).read_bytes();originals[p]=hashlib.sha256(raw).hexdigest();return raw.decode().replace('\r\n','\n')
def write(p,s):
 p=out/p;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,newline='\n')
def rep(s,a,z):
 assert s.count(a)==1,(a[:80],s.count(a));return s.replace(a,z)
core='crates/airk-core/src/'
s=read(core+'lib.rs');s=rep(s,'pub mod source_policy;','pub mod source_policy;\npub mod source_control;');write(core+'lib.rs',s)
write(core+'source_control.rs',(b/'p12-control_baseline.proposed.rs').read_text())
ing='crates/airk-ingest/src/'
s=read(ing+'lib.rs');s=rep(s,'mod policy_ignore;','mod policy_ignore;\npub mod control_snapshot;');write(ing+'lib.rs',s)
write(ing+'control_snapshot.rs',(b/'p12-control_snapshot.proposed.rs').read_text())
s=read(ing+'source_read.rs')
s=rep(s,'pub(crate) fn read(root: &Path, path: &Path, limit: usize, remaining: usize) -> Result<Vec<u8>> {','''/// Observe absence through the same no-follow pinned traversal as source
/// capture. Permission/type/redirect errors are uncertainty, never absence.
/// Does not enumerate directories or read bytes from an existing file.
pub(crate) fn missing_protected_path(path:&Path)->Result<bool> {
    ensure!(path.is_absolute(),"protected missing locator must be absolute");
    let root=path.ancestors().last().context("protected missing locator has no root")?;
    let relative=path.strip_prefix(root)?;
    match open(root,relative) {
        Ok(_)=>Ok(false),
        Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|error|error.kind()==std::io::ErrorKind::NotFound)=>Ok(true),
        Err(error)=>Err(error),
    }
}

pub(crate) fn read(root: &Path, path: &Path, limit: usize, remaining: usize) -> Result<Vec<u8>> {''')
write(ing+'source_read.rs',s)
s=read(ing+'policy_ignore.rs')
s=rep(s,'    root_blocked: bool,','    root_blocked: bool,\n    controls:Option<std::sync::Arc<crate::control_snapshot::ControlSnapshot>>,')
s=rep(s,'    fn local(root: &Path, policy: &SourcePolicy) -> Result<Self> {','''    pub(crate) fn tracked(root:&Path,policy:&SourcePolicy,controls:std::sync::Arc<crate::control_snapshot::ControlSnapshot>)->Result<Self> {
        let mut selected=Self::local_with_controls(root,policy,Some(controls))?;
        if !selected.root_blocked {selected.global=selected.global_rules()?;}
        Ok(selected)
    }
    fn local(root: &Path, policy: &SourcePolicy) -> Result<Self> {
        Self::local_with_controls(root,policy,None)
    }
    fn local_with_controls(root:&Path,policy:&SourcePolicy,controls:Option<std::sync::Arc<crate::control_snapshot::ControlSnapshot>>)->Result<Self> {''')
s=rep(s,'            root_blocked: false,','            root_blocked: false,\n            controls,')
s=rep(s,'        let root_text = selected','        if let Some(controls)=&selected.controls {controls.record_environment(&selected.root)?;}\n        let root_text = selected')
s=rep(s,'        let metadata = match fs::symlink_metadata(&path) {','        if self.controls.is_some() {crate::control_snapshot::qualify_locator(&path,false)?;}\n        let metadata = match fs::symlink_metadata(&path) {')
s=rep(s,'            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),','''            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(controls)=&self.controls {controls.absent_qualified(&path)?;}
                return Ok(None)
            },''')
s=rep(s,'        String::from_utf8(bytes)','        if let Some(controls)=&self.controls {controls.file(&path,&bytes)?;}\n        String::from_utf8(bytes)')
s=rep(s,'        let git = path.join(".git");','        let git = path.join(".git");\n        if self.controls.is_some() {crate::control_snapshot::qualify_locator(&git,true)?;}')
s=rep(s,'                ensure!(\n                    !git.join("commondir")','                if let Some(controls)=&self.controls {controls.directory_qualified(&git)?;}\n                ensure!(\n                    !git.join("commondir")')
s=rep(s,'            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),','''            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(controls)=&self.controls {controls.absent_qualified(&git)?;}
            },''')
s=rep(s,'        let rules = [','''        if let Some(controls)=self.controls.clone() {
            let commondir=git.join("commondir");
            crate::control_snapshot::qualify_locator(&commondir,false)?;
            ensure!(matches!(fs::symlink_metadata(&commondir),Err(error) if error.kind()==std::io::ErrorKind::NotFound), refused("ignore_control_git_reference"));
            controls.absent_qualified(&commondir)?;
        }
        let rules = [''')
# Tracked scans may have no permanent policy. This preserves ordinary admission
# while still forcing actual protected controls, never fabricating policy IDs.
s=rep(s,'    policy: SourcePolicy,','    policy: Option<SourcePolicy>,')
s=rep(s,'pub(crate) fn tracked(root:&Path,policy:&SourcePolicy,controls:', 'pub(crate) fn tracked(root:&Path,policy:Option<&SourcePolicy>,controls:')
s=rep(s,'Self::local_with_controls(root,policy,None)','Self::local_with_controls(root,Some(policy),None)')
s=rep(s,'fn local_with_controls(root:&Path,policy:&SourcePolicy,controls:', 'fn local_with_controls(root:&Path,policy:Option<&SourcePolicy>,controls:')
s=rep(s,'            policy: policy.clone(),','            policy: policy.cloned(),')
s=rep(s,'!crate::permanent_policy::allows_checked(policy, root_text, "", true)?;', 'policy.map(|policy|crate::permanent_policy::allows_checked(policy,root_text,"",true)).transpose()?.is_some_and(|allowed|!allowed);')
s=rep(s,'        crate::permanent_policy::qualify_discovery_directory(policy, root_text, "")?;', '        if let Some(policy)=policy {crate::permanent_policy::qualify_discovery_directory(policy,root_text,"")?;} else {crate::native_name_boundary::qualify(&selected.root,"",true)?;}')
s=rep(s,'crate::permanent_policy::allows_absolute_control_input_checked(&self.policy, &path)?', 'self.policy.as_ref().map(|policy|crate::permanent_policy::allows_absolute_control_input_checked(policy,&path)).transpose()?.unwrap_or(true)')
write(ing+'policy_ignore.rs',s)
# Add a tracked scanner without changing the public ordinary scanner's shape.
s=(out/(ing+'lib.rs')).read_text()
a='''        source_policy,
        lookup,
        emit,
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_observed_profiled('''
z='''        source_policy,
        lookup,
        emit,
    )
}

/// Native reviewed refresh only. All actual ignore inputs use protected capture;
/// absent/unsupported policy/configuration refuses qualification before scanning.
#[allow(clippy::too_many_arguments)]
pub fn scan_payloads_with_optional_control_snapshot(
    root:&Path,data_dir:&Path,previous:&Snapshot,roots:&BTreeMap<String,String>,
    limits:ScanLimits,policy:&adapters::profile::ExtractionPolicy,full_reindex:bool,
    execution:ExtractionExecution,paths:Option<&std::collections::BTreeSet<String>>,
    source_policy:Option<&airk_core::source_policy::SourcePolicy>,
    controls:Option<std::sync::Arc<control_snapshot::ControlSnapshot>>,
    lookup:impl FnMut(observations::ReuseRequest<'_>)->Result<Option<observations::VerifiedReuse>>,
    emit:impl FnMut(&airk_core::extraction::ExtractionProfile,&Artifact,&[u8],Option<&airk_core::extraction::ExtractionPayload>)->Result<Artifact>,
)->Result<(Snapshot,IndexReport,observations::ScanObservation)> {
    ensure!(controls.is_none() || policy.declared_configurations().is_empty(),"tracked refresh requires reviewed full fallback for declared extraction dependencies");
    limits.validate()?;execution.validate()?;
    scan_observed_profiled_controlled(root,data_dir,previous,roots,limits,policy,full_reindex,execution,paths,source_policy,controls,lookup,emit)
}

#[allow(clippy::too_many_arguments)]
fn scan_observed_profiled('''
s=rep(s,a,z)
# Preserve original function as adapter; clone entire original signature and forward.
start=s.index('fn scan_observed_profiled(');body=s.index('    if let Some(source_policy) = source_policy {',start)
sig=s[start:body]
controlled_sig=sig.replace('fn scan_observed_profiled(','fn scan_observed_profiled_controlled(').replace('    mut lookup: impl FnMut(','    controls:Option<std::sync::Arc<control_snapshot::ControlSnapshot>>,\n    mut lookup: impl FnMut(')
adapter=sig.replace('    mut lookup: impl FnMut(','    lookup: impl FnMut(')+'''    scan_observed_profiled_controlled(root,data_dir,previous,roots,limits,policy,full_reindex,execution,paths,source_policy,None,lookup,emit)
}

#[allow(clippy::too_many_arguments)]
'''
s=s[:start]+adapter+controlled_sig+s[body:]
s=rep(s,'discovery_selected_with_policy(source, &data_dir, selected, source_policy.cloned())','discovery_selected_with_controls(source, &data_dir, selected, source_policy.cloned(),controls.clone())')
# Ordinary/header planning retain original adapter. Tracked path forbids declared
# configs above, so no unobserved header reads can be certified.
a='''fn discovery_selected_with_policy(
    root: &Path,
    data_dir: &Path,
    selected: Option<Vec<String>>,
    policy: Option<airk_core::source_policy::SourcePolicy>,
) -> impl Iterator<Item = Result<ignore::DirEntry>> {'''
z=a+'''
    discovery_selected_with_controls(root,data_dir,selected,policy,None)
}

fn discovery_selected_with_controls(
    root:&Path,data_dir:&Path,selected:Option<Vec<String>>,
    policy:Option<airk_core::source_policy::SourcePolicy>,
    controls:Option<std::sync::Arc<control_snapshot::ControlSnapshot>>,
)->impl Iterator<Item=Result<ignore::DirEntry>> {'''
s=rep(s,a,z)
s=rep(s,'    let restricted = policy.as_ref().is_some_and(permanent_policy::restricts);','    let restricted = controls.is_some() || policy.as_ref().is_some_and(permanent_policy::restricts);')
s=rep(s,'        match policy_ignore::ProtectedIgnore::new(root, policy.as_ref().unwrap()) {','''        let captured=match controls {
            Some(controls)=>policy_ignore::ProtectedIgnore::tracked(root,policy.as_ref(),controls),
            None=>policy_ignore::ProtectedIgnore::new(root,policy.as_ref().unwrap()),
        };
        match captured {''')
write(ing+'lib.rs',s)
(b/'p12-control-owner-base.json').write_text(json.dumps(originals,indent=2)+'\n')
print(json.dumps({'owner_files':len(list(out.rglob('*.rs'))),'baseline_files':len(originals),'status':'INERT actual matcher capture hooks; Kernel resource/positive selected integration next'}))
