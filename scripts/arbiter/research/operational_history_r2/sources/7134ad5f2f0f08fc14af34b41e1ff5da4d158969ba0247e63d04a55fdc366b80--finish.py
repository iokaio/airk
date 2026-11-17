# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import runpy,difflib
r=runpy.run_path('artifacts/optimizations/f02-draft/prepare.py')
root=Path.cwd();changes=r['changes']
name='crates/airk-kernel/src/current/indexing/tests.rs'
old=(root/name).read_text(encoding='utf-8')
extra='''

#[test]
fn scalar_summary_preserves_policy_semantic_and_discovery_corruption_precedence() -> Result<()> {
    use airk_core::readiness::DefaultSearchPolicy;
    let root = model_data_directory()?;
    let data = model_data_directory()?;
    write_static_test_model(&data.path().join("models/local"))?;
    std::fs::write(root.path().join("fixture.txt"), "authenticate original\\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    kernel.set_default_semantic_search_policy(DefaultSearchPolicy::LexicalWhilePending)?;
    let read = kernel.current_reader()?;
    let state = read.collection_state()?.expect("published state");
    let mut resources = Vec::new();
    for key in ["semantic-policy", "semantic-work", "scan-work"] {
        let reference = read.resource_reference(key)?.expect("fixture resource");
        let path = kernel.store.directory().join("current-resources")
            .join(format!("{}.json", reference.digest));
        let original = std::fs::read(&path)?;
        let mut corrupt = original.clone();
        corrupt[0] ^= 1;
        std::fs::write(&path, corrupt)?;
        resources.push((path, original));
    }
    let error = kernel.status_summary().unwrap_err();
    assert!(!format!("{error:#}").contains("read current semantic work"));
    // The policy error wins over corrupt semantic and discovery work.
    assert!(super::super::enrichment::read_policy(&read, &kernel.store).is_err());
    std::fs::write(&resources[0].0, &resources[0].1)?;
    let error = kernel.status_summary().unwrap_err();
    assert!(format!("{error:#}").contains("read current semantic work"));
    // Standalone readiness retains policy -> state -> upgrade -> semantic work.
    assert!(super::super::enrichment::readiness(&read, &kernel.store).is_err());
    std::fs::write(&resources[1].0, &resources[1].1)?;
    assert!(kernel.status_summary().is_err());
    assert!(super::super::progress::readiness(&read, &kernel.store).is_err());
    assert!(super::super::progress::readiness_with_state(&read, &kernel.store, &state).is_err());
    std::fs::write(&resources[2].0, &resources[2].1)?;
    assert!(kernel.status_summary().is_ok());
    Ok(())
}
'''
changes[name]=(old,old+extra)
patch=''.join(''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name)) for name,(old,new) in changes.items())
(root/'artifacts/optimizations/f02-draft/kernel.patch').write_text(patch,encoding='utf-8',newline='\n')
for name,(_,new) in changes.items():
    target=root/'artifacts/optimizations/f02-draft/source'/name
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(new,encoding='utf-8',newline='\n')

