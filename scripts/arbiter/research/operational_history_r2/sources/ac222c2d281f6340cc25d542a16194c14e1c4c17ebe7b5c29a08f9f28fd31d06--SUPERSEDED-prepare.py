# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd();changes={}
name='crates/airk-kernel/src/current/enrichment.rs';old=(root/name).read_text(encoding='utf-8')
a='''        let current = PendingArtifact {
            path: path.clone(),
            source_hash: artifact.content_hash.parse()?,
            extraction_object: retained.object.parse()?,
        };
        if changes.contains_key(path)
            || previous_pending.get(path.as_str()).is_some_and(|prior| {
                prior.source_hash == current.source_hash
                    && prior.extraction_object == current.extraction_object
            })
        {
            pending.push(current);
        }'''
b='''        // Preserve binding validation for completed records, but clone a path only
        // when this exact record belongs in the durable pending queue.
        let source_hash = artifact.content_hash.parse()?;
        let extraction_object = retained.object.parse()?;
        if changes.contains_key(path)
            || previous_pending.get(path.as_str()).is_some_and(|prior| {
                prior.source_hash == source_hash && prior.extraction_object == extraction_object
            })
        {
            pending.push(PendingArtifact {
                path: path.clone(),
                source_hash,
                extraction_object,
            });
        }'''
assert old.count(a)==1;changes[name]=(old,old.replace(a,b,1))
name='crates/airk-kernel/src/current/indexing/tests.rs';old=(root/name).read_text(encoding='utf-8')
extra='''

#[test]
fn semantic_work_preparation_keeps_pending_bindings_and_validates_completed_records() -> Result<()> {
    let root = model_data_directory()?;
    let data = model_data_directory()?;
    write_static_test_model(&data.path().join("models/local"))?;
    for path in ["a.txt", "b.txt"] {
        std::fs::write(root.path().join(path), "authenticate original")?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let read = kernel.current_reader()?;
    let state = read.collection_state()?.expect("published configuration");
    let selection = &state.configuration.vector_selection;
    let pipeline = state.configuration.model.as_ref().expect("fixture model").pipeline_digest;
    let snapshot = kernel.current_scan_projection(&read)?;
    let prior = enrichment::read_work(&read, &kernel.store)?.expect("semantic work");
    assert!(prior.pending.is_empty());
    let ready = enrichment::prepare_work(&snapshot, &Default::default(), Some(&prior), selection, pipeline, false)?;
    assert!(ready.pending.is_empty());
    assert_eq!((ready.selected_artifacts, ready.completed_artifacts), (2, 2));
    let artifact = &snapshot.artifacts["a.txt"];
    let changes = std::collections::BTreeMap::from([("a.txt".into(),
        ArtifactChange::Upsert(Box::new(CapturedInput::new(artifact.clone()))))]);
    let pending = enrichment::prepare_work(&snapshot, &changes, Some(&prior), selection, pipeline, false)?;
    assert_eq!((pending.selected_artifacts, pending.completed_artifacts), (2, 1));
    assert_eq!(pending.pending.len(), 1);
    assert_eq!(pending.pending[0].path, "a.txt");
    assert_eq!(pending.pending[0].source_hash.to_string(), artifact.content_hash);
    let resumed = enrichment::prepare_work(&snapshot, &Default::default(), Some(&pending), selection, pipeline, false)?;
    assert_eq!(resumed.pending[0].path, pending.pending[0].path);
    assert_eq!(resumed.pending[0].source_hash, pending.pending[0].source_hash);
    assert_eq!(resumed.pending[0].extraction_object, pending.pending[0].extraction_object);
    assert_eq!(resumed.completed_artifacts, 1);
    let mut damaged = snapshot;
    damaged.artifacts.get_mut("b.txt").unwrap().content_hash = "invalid-fixture-hash".into();
    assert!(enrichment::prepare_work(&damaged, &changes, Some(&prior), selection, pipeline, false).is_err());
    Ok(())
}
'''
changes[name]=(old,old+extra)
patch=''.join(''.join(difflib.unified_diff(a.splitlines(True),b.splitlines(True),fromfile='a/'+n,tofile='b/'+n)) for n,(a,b) in changes.items())
(root/'artifacts/optimizations/f18-draft/lazy-pending-path.patch').write_text(patch,encoding='utf-8',newline='\n')
for n,(_,body) in changes.items():
 p=root/'artifacts/optimizations/f18-draft/source'/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(body,encoding='utf-8',newline='\n')
