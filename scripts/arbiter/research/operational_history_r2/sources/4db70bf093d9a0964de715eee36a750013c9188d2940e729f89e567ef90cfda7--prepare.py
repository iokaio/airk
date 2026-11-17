# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd()
changes={}
def load(name):
    return (root/name).read_text(encoding='utf-8')
def save(name,old,new):
    assert old != new,name
    changes[name]=(old,new)
name='crates/airk-kernel/src/current/enrichment.rs'
old=load(name)
new=old.replace('ArtifactChange, CapturedInput, CurrentRead, MAX_PENDING_INPUTS, resources::ResourceChange,','ArtifactChange, CapturedInput, CurrentRead, MAX_PENDING_INPUTS, collection::CollectionState,\n        resources::ResourceChange,',1)
anchor='''    if crate::shared_models::semantic_upgrade_pending(store, state.configuration.model.as_ref()) {'''
replacement='''    readiness_with_state(read, store, &state, policy)
}

/// Reuse scalar metadata from this exact authorized read; callers retain policy-read ordering.
pub(super) fn readiness_with_state(
    read: &CurrentRead,
    store: &FileStore,
    state: &CollectionState,
    policy: DefaultSearchPolicy,
) -> Result<SemanticReadiness> {
    if crate::shared_models::semantic_upgrade_pending(store, state.configuration.model.as_ref()) {'''
assert new.count(anchor)==1
new=new.replace(anchor,replacement,1)
anchor='''    let state = read
        .collection_state()?
        .context("current configuration missing")?;
    let c = &state.configuration;'''
assert new.count(anchor)==1
new=new.replace(anchor,'    let c = &state.configuration;',1)
save(name,old,new)
name='crates/airk-kernel/src/current/progress.rs'
old=load(name)
new=old.replace('current::{CurrentRead, resources::ResourceChange},','current::{CurrentRead, collection::CollectionState, resources::ResourceChange},',1)
anchor='''    Ok(DiscoveryReadiness {
        version: 1,
        state: DiscoveryState::Ready,'''
replacement='''    Ok(ready(total))
}

/// Keep work authoritative while reusing this read's already-authenticated scalar metadata.
pub(super) fn readiness_with_state(
    read: &CurrentRead,
    store: &FileStore,
    state: &CollectionState,
) -> Result<DiscoveryReadiness> {
    if let Some(work) = read_work(read, store)? {
        return Ok(work.readiness());
    }
    Ok(ready(state.counts.files))
}

fn ready(total: u64) -> DiscoveryReadiness {
    DiscoveryReadiness {
        version: 1,
        state: DiscoveryState::Ready,'''
assert new.count(anchor)==1
new=new.replace(anchor,replacement,1)
anchor='''        total_artifacts: Some(total),
        failure: None,
    })
}'''
assert new.count(anchor)==1
new=new.replace(anchor,'''        total_artifacts: Some(total),
        failure: None,
    }
}''',1)
save(name,old,new)
name='crates/airk-kernel/src/current/runtime.rs'
old=load(name)
anchor='''        let semantic = super::enrichment::readiness(&read, &self.store)?;
        let discovery = super::progress::readiness(&read, &self.store)?;'''
replacement='''        let policy = super::enrichment::read_policy(&read, &self.store)?;
        let semantic =
            super::enrichment::readiness_with_state(&read, &self.store, &state, policy)?;
        let discovery = super::progress::readiness_with_state(&read, &self.store, &state)?;'''
assert old.count(anchor)==1
new=old.replace(anchor,replacement,1)
save(name,old,new)
name='crates/airk-kernel/tests/current_runtime.rs'
old=load(name)
helper='''
/// Compare independently exposed scalar output with detailed publication-bound observations.
fn assert_summary_parity(kernel: &Kernel) -> Result<()> {
    let status = kernel.status()?;
    let summary = kernel.status_summary()?;
    assert_eq!(summary["index_state"], "available");
    assert_eq!(summary["files"], status.files);
    assert_eq!(summary["generation"], status.generation);
    assert_eq!(summary["indexed_vectors"], status.indexed_vectors);
    assert_eq!(
        summary["publication"]["token"],
        serde_json::to_value(&status.publication.as_ref().expect("published fixture").token)?
    );
    assert_eq!(
        summary["semantic_readiness"]["state"],
        serde_json::to_value(status.semantic_readiness.state)?
    );
    assert_eq!(
        summary["semantic_readiness"]["completed_artifacts"],
        status.semantic_readiness.completed_artifacts
    );
    assert_eq!(
        summary["semantic_readiness"]["selected_artifacts"],
        status.semantic_readiness.selected_artifacts
    );
    assert_eq!(
        summary["discovery_readiness"]["state"],
        serde_json::to_value(status.discovery_readiness.state)?
    );
    if status.semantic_readiness.state == SemanticState::Failed {
        assert_eq!(summary["health"], "needs_attention");
    }
    Ok(())
}
'''
new=old.replace('\n#[test]\n',helper+'\n#[test]\n',1)
# Extend three existing fixtures: vectors disabled, pending/ready, persisted semantic failure.
anchor='''    assert_eq!(before.files, 2);'''
assert new.count(anchor)==1
new=new.replace(anchor,anchor+'\n    assert_summary_parity(&kernel)?;',1)
anchor='''    let mut completed = kernel.status()?.semantic_readiness.completed_artifacts;'''
assert new.count(anchor)==1
new=new.replace(anchor,'    assert_summary_parity(&kernel)?;\n'+anchor,1)
anchor='''    assert_eq!(reopened.run_scheduled_enrichment()?.state, "idle");'''
assert new.count(anchor)==1
new=new.replace(anchor,'    assert_summary_parity(&reopened)?;\n'+anchor,1)
anchor='''    assert_eq!(status.semantic_readiness.state, SemanticState::Failed);'''
assert new.count(anchor)==1
new=new.replace(anchor,anchor+'\n    assert_summary_parity(&kernel)?;',1)
save(name,old,new)
patch=''.join(''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name)) for name,(old,new) in changes.items())
(root/'artifacts/optimizations/f02-draft/kernel.patch').write_text(patch,encoding='utf-8',newline='\n')
for name,(_,new) in changes.items():
    target=root/'artifacts/optimizations/f02-draft/source'/name
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(new,encoding='utf-8',newline='\n')
print('Draft patch generated for',len(changes),'files; tracked source unchanged.')
