# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

base = Path(__file__).parent
changes = {}
name = 'crates/airk-store/src/service_collection_integrity.rs'
old = (base / 'verify-source' / name).read_text(encoding='utf-8')
new = old.replace('pub mod verify;', 'pub mod verify;\n#[path = "service_collection_native_owner.rs"]\npub mod native_owner;')
new = new.replace('    pub verify: Option<verify::VerifyProgress>,', '''    pub verify: Option<verify::VerifyProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_owner: Option<native_owner::NativeOwnerEnrollment>,''')
new = new.replace('        let verify = previous.as_ref().and_then(|value| value.verify.clone());',
                  '        let verify = previous.as_ref().and_then(|value| value.verify.clone());\n        let native_owner = previous.as_ref().and_then(|value| value.native_owner.clone());')
new = new.replace('privacy, verify };', 'privacy, verify, native_owner };')
assert new != old and 'privacy, verify, native_owner' in new
changes[name] = (old, new)
name = 'crates/airk-store/src/service_collection_native_owner.rs'
changes[name] = ('', (base / 'native-owner.rs').read_text(encoding='utf-8'))
name = 'crates/airk-store/src/service_collection_census.rs'
old = (base / 'foundation-source' / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'owner-namespace-linux.rs').read_text(encoding='utf-8'))
name = 'crates/airk-store/src/service_census_windows.rs'
old = (base / 'foundation-source' / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'owner-namespace-windows.rs').read_text(encoding='utf-8'))
name = 'crates/airk-store/src/service_data_relocation.rs'
old = (base / 'audit-admission-source' / name).read_text(encoding='utf-8')
new = old + '''
/// Exact completed native move, not a path/default-location hint.
pub(crate) fn require_native_owner_move(control:&Path,source_key:airk_core::identity::Sha256Digest,destination:&Path)->Result<()> {
    require_settled(control)?;
    let value:Completed=serde_json::from_slice(&read(&control.join(RESULT),16384)?)?;
    let key=airk_core::identity::framed_digest(b"native-owner-data-root/1",
        &[value.source.to_str().context("non-UTF-8 native migration source")?.as_bytes()]);
    ensure!(key==source_key && selected(control,&value.source,destination)?,"native enrollment lacks exact completed relocation");
    Ok(())
}
'''
changes[name] = (old, new)
name = 'crates/airk-store/src/private_storage_windows.rs'
old = (base / 'foundation-source' / name).read_text(encoding='utf-8')
needle = '#[cfg(test)]\npub(super) mod tests {'
assert old.count(needle) == 1
new = old.replace(needle, (base / 'native-private-create-windows.rs').read_text(encoding='utf-8') + '\n' + needle)
new = new.replace('''    pub(crate) fn private_root(path: &std::path::Path) -> Result<()> {
        directory_acl(path, false)
    }''', '''    pub(crate) fn private_root(path: &std::path::Path) -> Result<()> {
        super::create_native_root(path)
    }''')
changes[name] = (old, new)
name = 'crates/airk-store/src/private_storage.rs'
old = (base / 'foundation-source' / name).read_text(encoding='utf-8')
changes[name] = (old, old + '''
#[cfg(windows)]
pub(crate) fn create_native_data_root(path:&std::path::Path)->Result<()> {
    windows::create_native_root(path)
}
''')
name = 'crates/airk-store/src/service_data_scope.rs'
old = (base / 'audit-admission-source' / name).read_text(encoding='utf-8')
needle = '''    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);'''
assert old.count(needle) == 1
new = old.replace(needle, '''    #[cfg(windows)]
    if !exists(&scope.data)? {
        fs::create_dir_all(scope.data.parent().context("native data parent missing")?)?;
        crate::private_storage::create_native_data_root(&scope.data)?;
    }
    #[cfg(not(windows))]
    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);''')
changes[name] = (old, new)
name = 'crates/airk-store/src/service_collection_tests.rs'
old = (base / 'verify-source' / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'native-owner-tests.rs').read_text(encoding='utf-8'))
name = 'crates/airk-kernel/src/diagnostics.rs'
old = (base / 'verify-source' / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'native-owner-kernel.rs').read_text(encoding='utf-8'))
name = 'crates/airk-cli/src/project_worker.rs'
old = (base / 'verify-source' / name).read_text(encoding='utf-8')
needle = '    storage: airk_store::service::data_scope::BoundDataRoot,'
assert old.count(needle) == 1
new = old.replace(needle, needle + '''
    // Foundation only: retained actual owner capability or explicit error;
    // ordinary tools keep every strict query check until F14 adopts consumers.
    _native_owner: Result<airk_store::service::collection::integrity::native_owner::NativeOwnerAuthority>,''')
needle = '    record_watch_roots(&kernel, &mut registry, &lease);'
assert new.count(needle) == 1
new = new.replace(needle, '''    let native_owner = kernel.prepare_native_owner(&registry,lease.clone(),&storage);
''' + needle)
needle = '        storage,\n        broker:'
assert new.count(needle) == 1
new = new.replace(needle, '        storage,\n        _native_owner: native_owner,\n        broker:')
changes[name] = (old, new)
patch = []
for name, (old, new) in changes.items():
    if old:
        original = base / 'native-owner-base' / name
        original.parent.mkdir(parents=True, exist_ok=True)
        original.write_text(old, encoding='utf-8', newline='\n')
    dest = base / 'native-owner-source' / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(new, encoding='utf-8', newline='\n')
    patch.extend(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                 fromfile='a/' + name if old else '/dev/null', tofile='b/' + name))
(base / 'native-owner-after-verify.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
