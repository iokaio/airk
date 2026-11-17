# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,difflib,hashlib
root=Path(r'D:/code/Github/airk');out=root/'artifacts/v190/post-p14-deleted-source-r2-proposed'
def edit(rel,fn):
    raw=(root/rel).read_bytes();p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
    p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(fn(raw.decode('utf-8')),encoding='utf-8',newline='\n')
def split(s):
    pos=s.index('pub(super) fn require_policy_change(')
    helper='''// Generic retirement must not strand an admitted split's paid children or
// destroy either participant's recovery authority, including before cutover.
pub(super) fn require_retirement(db: &Connection, index: IndexId) -> Result<()> {
    if !available(db)? { return Ok(()); }
    let active: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_split_participants WHERE index_id=?1 AND active=1)",
        [index.to_string()], |row| row.get(0))?;
    ensure!(!active, "active split owns this participant; finish exact split recovery before retirement");
    Ok(())
}

'''
    return s[:pos]+helper+s[pos:]
edit('crates/airk-store/src/service_catalog_split.rs',split)
def source(s):
    needle='''        } => {
            let (saved, definition, source, tombstone, _) = registration(db, *id)?;
'''
    assert needle in s
    return s.replace(needle,'''        } => {
            super::split::require_retirement(db, *id)?;
            let (saved, definition, source, tombstone, _) = registration(db, *id)?;
''',1)
edit('crates/airk-store/src/service_catalog_source_change.rs',source)
def tests(s):
    return s+'''
#[test]
fn generic_retirement_refuses_both_live_split_participants_before_any_storage_effect() -> Result<()> {
    let (_owner, mut registry, request, baseline, plan) = fixture()?;
    let state = registry.catalog_reserve_split(&request, &baseline, &plan)?;
    assert!(require_retirement(&registry.connection, request.original_index).is_err());
    assert!(require_retirement(&registry.connection, request.destination.index_id).is_err());
    require_retirement(&registry.connection, IndexId::from_identity(Incarnation::from_bytes([199; 16])))?;
    assert_eq!(registry.catalog_split(state.receipt.operation_id)?.context("saved split missing")?, state);
    Ok(())
}
'''
edit('crates/airk-store/src/service_catalog_split_tests.rs',tests)
# Remove unknown accessor assert rather than invent an observation API; compare source state through actual snapshot accessor.
p=out/'source/crates/airk-store/src/service_catalog_split_tests.rs';s=p.read_text(encoding='utf-8');s=s.replace('    assert_eq!(registry.catalog_split(state.receipt.operation_id)?.context("saved split missing")?, state);','    assert_eq!(registry.catalog_active_split_for_binding(request.original_registration.binding)?.context("saved split missing")?, state);');p.write_text(s,encoding='utf-8',newline='\n')
patch=[];manifest=[]
for p in sorted((out/'source').rglob('*.rs')):
    rel=p.relative_to(out/'source').as_posix();base=out/'base'/rel
    old=base.read_text(encoding='utf-8') if base.exists() else '';new=p.read_text(encoding='utf-8')
    patch.append('diff --git a/'+rel+' b/'+rel+'\n')
    if not base.exists():patch.append('new file mode 100644\n')
    patch.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+rel if base.exists() else '/dev/null',tofile='b/'+rel))
    manifest.append({'path':rel,'base_sha256':hashlib.sha256(base.read_bytes()).hexdigest() if base.exists() else None,'proposed_sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
(out/'missing-data.patch').write_text(''.join(patch),encoding='utf-8',newline='\n');(out/'base-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(manifest),'sha256':hashlib.sha256((out/'missing-data.patch').read_bytes()).hexdigest()}))
