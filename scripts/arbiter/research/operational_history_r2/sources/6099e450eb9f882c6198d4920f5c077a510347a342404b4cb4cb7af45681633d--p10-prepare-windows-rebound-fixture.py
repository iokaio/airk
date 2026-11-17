# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk'); base=root/'artifacts/v190/p10-worktree'
out=root/'artifacts/v190/p10-windows-rebound-fixture-proposed'; out.mkdir(exist_ok=True)
path='crates/airk-store/src/service_catalog_setup_storage_windows_tests.rs'
raw=(base/path).read_bytes(); original=raw.decode('utf-8'); source=original.replace('\r\n','\n')
old='''    let retained = root.create_dir(OsStr::new("selected"))?;
    let original = retained.create_file(OsStr::new("owner"), b"previous owner")?;
    let prepared = retained.create_file(OsStr::new("prepared"), b"next owner")?;
    std::fs::rename(base.join("selected"), base.join("retained"))?;
    std::fs::create_dir(base.join("selected"))?;
    std::fs::write(base.join("selected/owner"), b"foreign source")?;
'''
new='''    let retained = root.create_dir(OsStr::new("selected"))?;
    // Windows refuses directory rename while descendant files have open
    // handles. Establish the actual pathname rebound before opening children;
    // subsequent metadata must still use this original directory descriptor.
    std::fs::rename(base.join("selected"), base.join("retained"))
        .context("rebind fixture directory before opening descendant files")?;
    std::fs::create_dir(base.join("selected"))
        .context("create foreign rebound fixture directory")?;
    std::fs::write(base.join("selected/owner"), b"foreign source")
        .context("write preserved foreign rebound fixture bytes")?;
    let original = retained.create_file(OsStr::new("owner"), b"previous owner")?;
    let prepared = retained.create_file(OsStr::new("prepared"), b"next owner")?;
'''
assert source.count(old)==1; source=source.replace(old,new)
diagnostic=original.replace('\r\n','\n')
old_call='''    std::fs::rename(base.join("selected"), base.join("retained"))?;'''
diagnostic_call='''    std::fs::rename(base.join("selected"), base.join("retained"))
        .context("rebind fixture directory before opening descendant files")?;'''
assert diagnostic.count(old_call)==1; diagnostic=diagnostic.replace(old_call,diagnostic_call)
# The diagnostic intentionally changes only the existing operation's error
# context: no handle lifetime/order, permissions or production behavior changes.
diagnostic_patch=''.join(difflib.unified_diff(original.splitlines(True),diagnostic.splitlines(True),fromfile='a/'+path,tofile='b/'+path))
(out/'diagnostic.patch').write_text(diagnostic_patch,encoding='utf-8',newline='\n')
after_diagnostic_patch=''.join(difflib.unified_diff(diagnostic.splitlines(True),source.splitlines(True),fromfile='a/'+path,tofile='b/'+path))
(out/'order-after-diagnostic.patch').write_text(after_diagnostic_patch,encoding='utf-8',newline='\n')
dest=out/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(source,encoding='utf-8',newline='\n')
patch=''.join(difflib.unified_diff(original.splitlines(True),source.splitlines(True),fromfile='a/'+path,tofile='b/'+path))
(out/'fixture.patch').write_text(patch,encoding='utf-8',newline='\n')
notes={
'status':'INERT test-only order correction proposal, no source edits/Cargo/native reruns',
'baseline_sha256':hashlib.sha256(raw).hexdigest(),'patch_sha256':hashlib.sha256(patch.encode()).hexdigest(),
'diagnostic_patch_sha256':hashlib.sha256(diagnostic_patch.encode()).hexdigest(),
'order_after_diagnostic_patch_sha256':hashlib.sha256(after_diagnostic_patch.encode()).hexdigest(),
'diagnostic_first':'diagnostic.patch adds ONLY context to the plain directory rename at current test line221. Root can run exact actual case to confirm this boundary first, then apply order-after-diagnostic.patch. fixture.patch is alternative original->final complete patch, not an additional patch after diagnostic.',
'observed':'Full P10 workspace R4: windows_owner_replace_uses_mutation_handle_on_exact_rebound_directory failed plain AccessDenied(oserror5), while anchored_owner_replace_refuses_changed_target_then_publishes_exact_owned_file and other native setup/purge/finalization tests passed.',
'root_cause_evidence':'Test currently creates two live descendant metadata file HANDLEs, then plain std::fs::rename of containing directory. All production NtCreateFile/NtSetInformationFile failures carry nt_result operation+NTSTATUS context, absent in reported error. Microsoft FILE_RENAME_INFORMATION special open-file rules explicitly disallow renaming directory containing a file with open handles. This is a concrete unsupported fixture ordering before the owning replacement API; no evidence production Ex replacement regressed.',
'official_reference':'https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information',
'scope':'Rename/rebind directory while retained directory pin alone exists; then create BOTH owner/prepared files through captured retained descriptor. Keep expected original HANDLE alive across existing-target POSIX replacement, identity verification, native exact-file check and foreign rebound bytes/count assertions. Add context to plain fixture rename/create/write. No ACL/share/delete/readonly relaxation or production mutation.',
'boundary_sibling_review':'NativeRoot::begin(service_census_windows.rs503+) obtains handle native_boundary. private_storage_windows.rs177+ authenticates token owner, inherited actual allow ACL and exact owner full-control template. Native default fixtures service_collection_tests.rs3454+ use ambient AIRK_PRIVATE_CENSUS_BASE/tempdir without ACL rewrite; controlled lifecycle fixture explicitly calls test_private_root. Root boundary failures therefore remain honest unsupported ambient policy until actual fixture ACL provenance audit; native safety rules must not be weakened. Peer/root owns actual ACL/base diagnosis.',
'validation':'Not executed. Root should independently review and run this actual focused Windows test after complete workspace census; existing passed production replacement tests retained. Source hashes and original failure log preserved.'}
(out/'review.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'patch_sha256':notes['patch_sha256'],'out':str(out)}))
