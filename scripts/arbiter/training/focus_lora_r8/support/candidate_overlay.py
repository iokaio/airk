# SPDX-License-Identifier: Apache-2.0
"""Private reviewed candidate overlay: prepare now; apply only after milestone checkout.

Never commits, stages, checks out, builds, signs, publishes, deletes or changes ACLs.
Source lane: only explicit reviewed Git-public files, in-place over same-commit
human-owned candidate files. Generated lane: exactly handoff outputs and locked
public build dependencies, into absent candidate destinations. No ignored source
or private state is discovered/copied. Failures retain truthful partial evidence.
"""
import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys

HUMAN = 'S-1-5-21-3678370870-3858642417-826968105-1002'
HEX = re.compile(r'[0-9a-f]{40}')
PUBLIC_DIRS = frozenset(('.cargo','.config','.github','crates','docs','examples','fixtures',
    'installers','packages','scripts','site','supply-chain','third_party','tools'))
PUBLIC_ROOT = frozenset(('AGENTS.md','CLAUDE.md','CHANGELOG.md','CODE_OF_CONDUCT.md','CONTRIBUTING.md',
    'Cargo.lock','Cargo.toml','LICENSE','NOTICE','README.md','SECURITY.md','THIRD_PARTY_NOTICES.md',
    '.gitattributes','.gitignore','.dockerignore','.editorconfig','SUPPORT.md','compose.yaml','Dockerfile','build.ps1','build.sh','deny.toml',
    'rust-toolchain.toml'))
DEPENDENCY_SEEDS = ('@vscode/vsce','typescript','esbuild','jsonc-parser','@sigstore/verify',
    '@sigstore/bundle','@sigstore/core','@sigstore/protobuf-specs','snappyjs')


def private_output_root():
    value = os.environ.get('AIRK_OVERLAY_OUTPUT_ROOT')
    need(value is not None, 'AIRK_OVERLAY_OUTPUT_ROOT must name an existing private artifacts directory')
    path = Path(value).resolve(strict=True)
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    need(path.is_dir() and path.is_relative_to(root/'artifacts'), 'overlay output must remain in private artifacts')
    return path


def need(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()


def relative_name(value):
    path=PurePosixPath(value)
    need(isinstance(value,str) and value and '\\' not in value and ':' not in value
         and not path.is_absolute() and path.as_posix()==value
         and all(part not in ('','.', '..') for part in path.parts)
         and not any(ord(c)<32 for c in value) and not value.startswith('"'), 'unsafe relative name: '+repr(value))
    return path


def public_name(value):
    path=relative_name(value)
    return ('node_modules' not in path.parts and 'out' not in path.parts
        and (len(path.parts)==1 and value in PUBLIC_ROOT or len(path.parts)>1 and path.parts[0] in PUBLIC_DIRS))


def regular(path):
    info=path.lstat()
    need(stat.S_ISREG(info.st_mode) and info.st_nlink==1
         and not getattr(info,'st_file_attributes',0)&0x400, 'linked/special/aliased file: '+str(path))
    return info


def security(path):
    """Read exact owner/group/DACL and native file identity. No security writes."""
    need(os.name=='nt','this private helper is Windows-only')
    api=ctypes.WinDLL('advapi32',use_last_error=True);kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    api.GetNamedSecurityInfoW.argtypes=[wintypes.LPWSTR,wintypes.DWORD,wintypes.DWORD,
        ctypes.POINTER(ctypes.c_void_p),ctypes.POINTER(ctypes.c_void_p),ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(ctypes.c_void_p),ctypes.POINTER(ctypes.c_void_p)]
    api.GetNamedSecurityInfoW.restype=wintypes.DWORD
    api.ConvertSidToStringSidW.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)]
    api.ConvertSecurityDescriptorToStringSecurityDescriptorW.argtypes=[ctypes.c_void_p,wintypes.DWORD,
        wintypes.DWORD,ctypes.POINTER(ctypes.c_void_p),ctypes.POINTER(wintypes.DWORD)]
    kernel.LocalFree.argtypes=[ctypes.c_void_p];kernel.LocalFree.restype=ctypes.c_void_p
    owner=ctypes.c_void_p();descriptor=ctypes.c_void_p();owner_text=ctypes.c_void_p();sddl=ctypes.c_void_p()
    code=api.GetNamedSecurityInfoW(str(path),1,7,ctypes.byref(owner),None,None,None,ctypes.byref(descriptor))
    if code:raise OSError(code,'security observation failed: '+str(path))
    try:
        if not api.ConvertSidToStringSidW(owner,ctypes.byref(owner_text)):raise ctypes.WinError(ctypes.get_last_error())
        if not api.ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor,1,7,ctypes.byref(sddl),None):
            raise ctypes.WinError(ctypes.get_last_error())
        info=path.lstat()
        need(not path.is_symlink() and not getattr(info,'st_file_attributes',0)&0x400,'redirected path: '+str(path))
        return {'owner_sid':ctypes.wstring_at(owner_text),'sddl':ctypes.wstring_at(sddl),
                'file_id':[info.st_dev,info.st_ino]}
    finally:
        for pointer in (owner_text,sddl,descriptor):
            if pointer.value:kernel.LocalFree(pointer)


def human_path(path, root):
    need(path.is_relative_to(root),'path escaped approved root: '+str(path))
    for current in [path,*path.parents]:
        if not current.is_relative_to(root):break
        observed=security(current)
        need(observed['owner_sid']==HUMAN,'foreign-owned output/input preserved: '+str(current))
    return security(path)


def identity():
    code=r'''$id=[System.Security.Principal.WindowsIdentity]::GetCurrent();$explorer=@(Get-CimInstance Win32_Process -Filter "name='explorer.exe'" | ForEach-Object {$owner=Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid;@{session=$_.SessionId;sid=$owner.Sid}});@{name=$id.Name;sid=$id.User.Value;quser=(@(quser) -join "`n");explorer=$explorer}|ConvertTo-Json -Compress -Depth 4'''
    done=subprocess.run(['pwsh','-NoProfile','-NonInteractive','-Command',code],check=True,capture_output=True,encoding='utf-8')
    value=json.loads(done.stdout)
    need(value['sid']==HUMAN,'effective execution SID differs from authorized human')
    short=value['name'].split('\\')[-1].lower()
    sessions=[]
    for line in value['quser'].splitlines():
        match=re.match(r'^\s*>?(\S+)\s+\S+\s+(\d+)\s+Active(?:\s|$)',line)
        if match and match[1].lower()==short:sessions.append(int(match[2]))
    need(any(row['sid']==HUMAN and row['session'] in sessions for row in value['explorer']),
         'logged-in human SID/session did not match effective identity')
    return value


def git(root,*args,input=None):
    return subprocess.check_output(['git','--no-optional-locks',*args],cwd=root,input=input)


def revision(root):
    return git(root,'rev-parse','HEAD').decode().strip()


def tree_rows(root):
    result={}
    for row in git(root,'ls-tree','-rz','--full-tree','HEAD').split(b'\0'):
        if not row:continue
        header,name=row.split(b'\t',1);mode,kind,blob=header.decode().split()
        rel=os.fsdecode(name);need(public_name(rel),'tracked non-public path requires review: '+rel)
        need(kind=='blob' and mode in ('100644','100755'),'nonregular tracked object: '+rel)
        result[rel]={'mode':mode,'blob':blob}
    return result


def normalized_blobs(root,names):
    """Read Git's actual clean normalization; no -w and no index/object mutation."""
    names=sorted(names)
    if not names:return {}
    wire=b''.join(os.fsencode(name)+b'\0' for name in names)
    attributes=git(root,'check-attr','-z','--stdin','filter',input=wire).split(b'\0')
    need(len(attributes)==len(names)*3+1,'incomplete Git attribute observation')
    need(all(attributes[i+2]==b'unspecified' for i in range(0,len(attributes)-1,3)),
         'external Git clean filter refused; no override or policy workaround')
    raw=git(root,'hash-object','--stdin-paths',input=b''.join(os.fsencode(name)+b'\n' for name in names))
    values=raw.decode().splitlines();need(len(values)==len(names),'incomplete normalized blob census')
    return dict(zip(names,values))


def owner(root,filename):
    spec=importlib.util.spec_from_file_location('candidate_overlay_'+filename.replace('-','_'),root/'scripts'/filename)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module


def native_seal(root):
    return owner(root,'build-service-runtime.py').source_identity()


def source_rows(root,candidate):
    raw=git(root,'ls-files','-z','--cached','--others','--exclude-standard','--')
    names=sorted({os.fsdecode(name) for name in raw.split(b'\0') if name})
    rows=[];excluded=[];missing=[]
    for name in names:
        if not public_name(name):excluded.append(name);continue
        source=root/name
        if not os.path.lexists(source):missing.append(name);continue
        regular(source)
        need(source.resolve(strict=True).is_relative_to(root),'source escaped root: '+name)
        for parent in source.parents:
            if parent==root:break
            need(not parent.is_symlink() and not getattr(parent.lstat(),'st_file_attributes',0)&0x400,'redirected source parent')
        destination=candidate/name;before=None
        if os.path.lexists(destination):
            regular(destination);before={'sha256':sha(destination),'security':human_path(destination,candidate)}
        rows.append({'path':name,'sha256':sha(source),'bytes':source.stat().st_size,'candidate_before':before})
    return rows,excluded,missing


def resolve_dependency(package,request,entries):
    current=PurePosixPath(package)
    while True:
        choice=(current/'node_modules'/request).as_posix()
        if choice in entries:return choice
        if current==PurePosixPath('.'):break
        current=current.parent
    raise ValueError('locked dependency is unresolved: '+package+' -> '+request)


def dependency_closure(lock,snapshot):
    entries=lock['packages'];queue=['node_modules/'+name for name in DEPENDENCY_SEEDS]
    selected=set();skipped=[]
    while queue:
        key=queue.pop()
        relative_name(key);need(key.startswith('node_modules/'),'dependency key escaped approved public package root')
        if key in selected:continue
        need(key in entries,'required public dependency missing from lock: '+key)
        row=entries[key];need(not row.get('link'),'linked dependency is not an approved public byte copy')
        folder=snapshot/'packages/vscode'/key
        need(folder.is_dir() and not folder.is_symlink(),'required public dependency directory missing/linked: '+key)
        package=json.loads((folder/'package.json').read_text(encoding='utf-8'))
        name=key.rsplit('node_modules/',1)[-1]
        need(package.get('name')==name and package.get('version')==row.get('version'),'installed dependency identity differs: '+key)
        selected.add(key)
        for request in row.get('dependencies',{}):queue.append(resolve_dependency(key,request,entries))
        for request in row.get('optionalDependencies',{}):
            resolved=resolve_dependency(key,request,entries)
            if (snapshot/'packages/vscode'/resolved).is_dir():queue.append(resolved)
            else:skipped.append(resolved)
        for request in row.get('peerDependencies',{}):
            try:resolved=resolve_dependency(key,request,entries)
            except ValueError:
                need(row.get('peerDependenciesMeta',{}).get(request,{}).get('optional') is True,'required peer missing')
                continue
            if (snapshot/'packages/vscode'/resolved).is_dir():queue.append(resolved)
    return sorted(selected),sorted(set(skipped))


def files_below(folder,approved_root):
    human_path(folder,approved_root)
    rows=[]
    for parent,dirs,files in os.walk(folder,followlinks=False):
        parent=Path(parent)
        for name in dirs:human_path(parent/name,approved_root)
        for name in files:
            path=parent/name;regular(path);human_path(path,approved_root)
            rows.append({'path':path.relative_to(folder).as_posix(),'sha256':sha(path),'bytes':path.stat().st_size})
    return sorted(rows,key=lambda row:row['path'])


def readonly_allowlist(root,snapshot,outputs):
    package=owner(root,'package-extension.py');compiled=snapshot/'packages/vscode/out'
    available={row['path'] for row in outputs}
    expected=set(package.MODULES+package.BUNDLES)
    need(expected<=available,'reviewed compiled outputs miss package allowlist entries')
    imports=0
    for name in package.MODULES:
        raw=(compiled/name).read_bytes()
        need(len(raw)<=4_194_304 and not package.native_content('out/'+name,raw,0o100644),
             'invalid/native compiled runtime input: '+name)
        for match in re.finditer(r'''\b(?:require|import)\s*\(\s*(["'])(\.[^"']*)\1\s*\)''',raw.decode('utf-8')):
            need(package.resolve_import(name,match[2]) is not None,'runtime import escaped owning allowlist: '+name)
            imports+=1
    package.verify_pilot_runtime((compiled/'uiPilotRuntime.js').read_bytes(),root=snapshot)
    need(package.resolve_import('workbenchContracts.js','./generated/other') is None,
         'negative generated-module allowlist check unexpectedly admitted extra')
    need(package.resolve_import('extension.js','./test/private.test') is None,
         'negative test-module allowlist check unexpectedly admitted extra')
    jsonc=snapshot/'packages/vscode/node_modules/jsonc-parser'
    need(all((jsonc/name).is_file() for name in package.JSONC_FILES),'required public parser closure missing')
    return {'checked_actual_modules':len(package.MODULES),'required_actual_bundles':len(package.BUNDLES),
        'local_import_edges':imports,'transferred_outputs':len(outputs),
        'not_shipped_by_allowlist':sorted(available-expected),'negative_extra_generated_and_test_imports_refused':True,
        'scope':'read-only actual import/allowlist and pilot-seal check, no staging/Node compilation; full canonical actual-artifact tests remain required after transfer'}


def ui_rows(root,handoff,approved):
    regular(handoff);need(sha(handoff)==approved,'reviewed handoff bytes differ')
    document=json.loads(handoff.read_text(encoding='utf-8'));snapshot=Path(document['source_root'])
    need(snapshot==handoff.parent.parent/'source' and snapshot.is_relative_to(root/'artifacts'),
         'UI snapshot is outside the explicitly reviewed artifact root')
    need(document['sid']==HUMAN and document['editor_output_count']==223,'UI handoff identity/count differs')
    manifest=handoff.parent.parent/relative_name(document['input_manifest'])
    regular(manifest);need(sha(manifest)==document['input_manifest_sha256'],'UI input manifest differs')
    inputs=json.loads(manifest.read_text(encoding='utf-8'))
    checked=[]
    for row in inputs['files']:
        name=row['path'];relative_name(name)
        if not name.startswith(('packages/vscode/','packages/ui/','packages/admin/','packages/contracts-gen/')):continue
        need(public_name(name),'UI source manifest contains non-public source')
        for base in (snapshot,root):
            regular(base/name);need(sha(base/name)==row['sha256'],'UI reviewed raw source drift: '+name)
        checked.append(name)
    for row in document['derived_tracked']:
        need(public_name(row['path']),'non-public generated tracked input')
        need(sha(root/row['path'])==sha(snapshot/row['path'])==row['sha256'],'generated tracked input drift')
    for row in document['admin_assets']:
        name=relative_name(row['path']).as_posix()
        need('/' not in name,'unexpected nested Admin asset')
        rel='crates/airk-cli/admin-assets/'+name
        need(sha(root/rel)==sha(snapshot/rel)==row['sha256'],'sealed Admin asset drift')
    observed=files_below(snapshot/'packages/vscode/out',snapshot)
    wanted=[{key:row[key] for key in ('path','sha256','bytes')} for row in document['editor_outputs']]
    need(observed==sorted(wanted,key=lambda row:row['path']) and len(observed)==223,'complete compiled223 roster differs')
    need(all(PurePosixPath(row['path']).suffix in ('.js','.css','.json') for row in observed),'unexpected compiled suffix')
    lock_path=snapshot/'packages/vscode/package-lock.json';lock=json.loads(lock_path.read_text(encoding='utf-8'))
    selected,skipped=dependency_closure(lock,snapshot)
    dependencies={}
    for package in selected:
        folder=snapshot/'packages/vscode'/package
        for parent,dirs,files in os.walk(folder,followlinks=False):
            parent=Path(parent);dirs[:]=[name for name in dirs if name!='node_modules']
            human_path(parent,snapshot)
            for name in files:
                path=parent/name;regular(path);human_path(path,snapshot)
                rel=path.relative_to(snapshot/'packages/vscode').as_posix();relative_name(rel)
                dependencies[rel]={'path':rel,'sha256':sha(path),'bytes':path.stat().st_size}
    return {'handoff':str(handoff),'handoff_sha256':approved,'input_manifest':str(manifest),
        'input_manifest_sha256':sha(manifest),'snapshot':str(snapshot),'reviewed_ui_source_count':len(checked),
        'reviewed_ui_sources':checked,'compiled_outputs':observed,'dependency_packages':selected,
        'missing_other_platform_optional_packages':skipped,'dependency_files':sorted(dependencies.values(),key=lambda row:row['path']),
        'package_lock_sha256':sha(lock_path),'readonly_allowlist':readonly_allowlist(root,snapshot,observed),
        'destinations':['packages/vscode/out','packages/vscode/node_modules'],
        'privacy':'Only explicit compiled outputs and locked public package files are authorized ignored build inputs; no .bin links, caches, credentials, fixtures or other ignored trees copied.'}


def prepare(args):
    actor=identity();root=args.root.resolve(strict=True);candidate=args.candidate.resolve(strict=True)
    human_path(candidate,candidate);need(args.output.parent.resolve(strict=True)==private_output_root(),'plan output must stay in this approved private helper directory');human_path(args.output.parent.resolve(strict=True),private_output_root())
    need(root!=candidate and candidate.is_relative_to(root/'artifacts/v190/final-plan'),'candidate boundary differs')
    need(not args.output.exists(),'fresh private plan path required')
    before_native=native_seal(root)
    rows,excluded,missing=source_rows(root,candidate)
    for rel in ('packages/vscode/out','packages/vscode/node_modules'):
        need(not os.path.lexists(candidate/rel),'existing generated destination preserved: '+rel)
    commit=revision(root);candidate_commit=revision(candidate)
    ready=bool(args.commit)
    if ready:
        need(HEX.fullmatch(args.commit) and commit==candidate_commit==args.commit,'root/candidate must already be at exact reviewed milestone commit')
        tree=tree_rows(root)
        need(set(tree)=={row['path'] for row in rows} and not excluded and not missing,'postcommit source file set is not exactly committed/public/present')
        normalized=normalized_blobs(root,tree)
        need(all(normalized[name]==value['blob'] for name,value in tree.items()),'root normalized bytes differ from committed tree')
        candidate_normalized=normalized_blobs(candidate,tree)
        need(all(candidate_normalized[name]==value['blob'] for name,value in tree.items()),'candidate normalized bytes differ before overlay')
    ui=ui_rows(root,args.handoff.resolve(strict=True),args.approved_handoff_sha256)
    after_native=native_seal(root)
    source_drift=[row['path'] for row in rows if not (root/row['path']).is_file() or sha(root/row['path'])!=row['sha256']]
    final_names={os.fsdecode(name) for name in git(root,'ls-files','-z','--cached','--others','--exclude-standard','--').split(b'\0') if name}
    final_present={name for name in final_names if public_name(name) and (root/name).is_file()}
    names_unchanged=final_present=={row['path'] for row in rows}
    if ready:
        need(before_native==after_native and not source_drift and names_unchanged,'postcommit consumed-source freeze changed during preparation; no applicable plan emitted')
    plan={'schema':'airk-private-candidate-overlay-plan/1','created_utc':datetime.now(timezone.utc).isoformat(),
        'phase':'postcommit_ready' if ready else 'precommit_review_draft','commit':args.commit,
        'root':str(root),'candidate':str(candidate),'observed_root_head':commit,'observed_candidate_head':candidate_commit,
        'identity':actor,'native_root_source_identity':after_native,
        'native_root_source_before':before_native,'source_drift_during_prepare':source_drift,'source_name_set_unchanged':names_unchanged,
        'source_files':rows,'excluded_nonpublic_names':excluded,'tracked_missing_root_names':missing,
        'source_file_count':len(rows),'raw_overlay_count':sum(row['candidate_before'] is None or row['candidate_before']['sha256']!=row['sha256'] for row in rows),
        'ui':ui,'helper_sha256':sha(Path(__file__)),
        'limits':['Draft cannot apply: regenerate after milestone commit and explicit candidate checkout.','Git-normalized identity and native raw C7 identity are checked separately. Committing tracked deletions may change the genuine C7 source seal even without present-byte changes.','No checkout/stage/commit/build/npm/sign/publication is performed by this helper. Native/package/actual allowlist qualification remains root-owned.']}
    with args.output.open('x',encoding='utf-8',newline='\n') as stream:
        json.dump(plan,stream,indent=2);stream.write('\n')
    print(json.dumps({'plan':str(args.output),'sha256':sha(args.output),'phase':plan['phase'],
        'source_files':len(rows),'raw_overlay_count':plan['raw_overlay_count'],'compiled_outputs':len(ui['compiled_outputs']),
        'locked_dependency_packages':len(ui['dependency_packages']),'dependency_files':len(ui['dependency_files']),
        'native_source_identity':plan['native_root_source_identity'],'excluded_count':len(excluded),'tracked_missing_count':len(missing)}))
    return plan


def same_security(before,after):
    return before==after


def copy_existing(source,destination,row,candidate):
    before=human_path(destination,candidate);regular(destination)
    need(before==row['candidate_before']['security'] and sha(destination)==row['candidate_before']['sha256'],
         'candidate changed since explicit overlay plan: '+row['path'])
    regular(source)
    need(sha(source)==row['sha256'],'source drift before write: '+row['path'])
    with source.open('rb') as incoming,destination.open('r+b') as output:
        opened=os.fstat(output.fileno())
        need([opened.st_dev,opened.st_ino]==before['file_id'] and security(destination)==before,
             'opened candidate file changed before first write; preserved')
        while chunk:=incoming.read(1024*1024):output.write(chunk)
        output.truncate();output.flush();os.fsync(output.fileno())
    need(same_security(before,security(destination)),'owner/ACL/native file identity changed during in-place overlay')
    need(sha(destination)==sha(source)==row['sha256'],'raw overlay digest mismatch')


def copy_new(source,destination,row,candidate):
    need(not os.path.lexists(destination),'fresh generated destination appeared; preserve it')
    missing=[];parent=destination.parent
    while not parent.exists():missing.append(parent);parent=parent.parent
    human_path(parent,candidate)
    for folder in reversed(missing):folder.mkdir();human_path(folder,candidate)
    regular(source);need(security(source)['owner_sid']==HUMAN,'foreign generated source preserved')
    need(sha(source)==row['sha256'],'generated source drift before copy')
    with source.open('rb') as incoming,destination.open('xb') as output:
        created=security(destination);opened=os.fstat(output.fileno())
        need(created['owner_sid']==HUMAN and created['file_id']==[opened.st_dev,opened.st_ino],
             'new destination identity not proven before first content write')
        while chunk:=incoming.read(1024*1024):output.write(chunk)
        output.flush();os.fsync(output.fileno())
    human_path(destination,candidate)
    need(sha(destination)==row['sha256'],'generated copy digest mismatch')


def apply(args):
    actor=identity();regular(args.plan)
    need(sha(args.plan)==args.approved_plan_sha256,'independently reviewed plan bytes differ')
    plan=json.loads(args.plan.read_text(encoding='utf-8'))
    need(plan['schema']=='airk-private-candidate-overlay-plan/1' and plan['phase']=='postcommit_ready',
         'draft plan cannot apply; regenerate after milestone commit and explicit candidate checkout')
    need(plan['helper_sha256']==sha(Path(__file__)),'reviewed helper changed since plan')
    need(HEX.fullmatch(args.commit or '') and plan['commit']==args.commit,'exact reviewed milestone commit required')
    root=Path(plan['root']);candidate=Path(plan['candidate']);ui=plan['ui'];snapshot=Path(ui['snapshot'])
    human_path(candidate,candidate);need(args.receipt.parent.resolve(strict=True)==private_output_root(),'receipt output must stay in this approved private helper directory');human_path(args.receipt.parent.resolve(strict=True),private_output_root())
    need(not args.receipt.is_relative_to(candidate) and not args.receipt.exists(),'fresh external private receipt required')
    need(revision(root)==revision(candidate)==args.commit,'root/candidate HEAD differs from reviewed milestone')
    tree=tree_rows(root);need(tree_rows(candidate)==tree,'candidate committed tree differs')
    need(set(tree)=={row['path'] for row in plan['source_files']},'reviewed public source set differs from exact commit')
    root_normalized=normalized_blobs(root,tree)
    need(all(root_normalized[name]==row['blob'] for name,row in tree.items()),
         'root normalized source differs from committed tree')
    before_normalized=normalized_blobs(candidate,tree)
    need(all(before_normalized[name]==row['blob'] for name,row in tree.items()),'candidate normalized source already dirty')
    need(native_seal(root)==plan['native_root_source_identity'],'root raw C7 source changed since reviewed plan')
    need(ui_rows(root,Path(ui['handoff']),ui['handoff_sha256'])==ui,'reviewed UI/compiled/dependency closure changed')
    for row in plan['source_files']:
        need(public_name(row['path']) and row['candidate_before'] is not None,'source path not already placed by Git checkout')
        source=root/row['path'];destination=candidate/row['path']
        regular(source);regular(destination)
        need(sha(source)==row['sha256'],'root raw source drifted: '+row['path'])
        need(sha(destination)==row['candidate_before']['sha256']
             and human_path(destination,candidate)==row['candidate_before']['security'],
             'candidate source/owner/ACL changed after plan: '+row['path'])
    for name in ui['destinations']:need(not os.path.lexists(candidate/name),'existing ignored output/cache preserved: '+name)
    started={'kind':'started','utc':datetime.now(timezone.utc).isoformat(),'actor':actor,
        'command':[sys.executable,*sys.argv],'plan_sha256':args.approved_plan_sha256,'commit':args.commit,
        'root_source':plan['native_root_source_identity'],'candidate_source_before':native_seal(candidate),
        'passed':False,'source_copied':0,'compiled_copied':0,'dependency_copied':0}
    with args.receipt.open('x',encoding='utf-8',newline='\n') as stream:
        def record(value):
            stream.write(json.dumps(value)+'\n');stream.flush();os.fsync(stream.fileno())
        record(started)
        try:
            for row in plan['source_files']:
                if row['sha256']==row['candidate_before']['sha256']:continue
                copy_existing(root/row['path'],candidate/row['path'],row,candidate)
                started['source_copied']+=1
                record({'kind':'source_raw_overlay','path':row['path'],'sha256':row['sha256'],'owner_acl_file_id_preserved':True})
            actual=native_seal(candidate)
            need(actual==native_seal(root)==plan['native_root_source_identity'],'genuine raw native C7 seals differ after overlay')
            for label,rows,source_base,destination_base in (
                ('compiled',ui['compiled_outputs'],snapshot/'packages/vscode/out',candidate/'packages/vscode/out'),
                ('dependency',ui['dependency_files'],snapshot/'packages/vscode',candidate/'packages/vscode')):
                for index,row in enumerate(rows,1):
                    relative_name(row['path'])
                    if label=='dependency':need(row['path'].startswith('node_modules/'),'dependency escaped explicit build-only root')
                    copy_new(source_base/row['path'],destination_base/row['path'],row,candidate)
                    started[label+'_copied']+=1
                    if index%128==0:record({'kind':label+'_progress','copied':index,'total':len(rows)})
            need(files_below(candidate/'packages/vscode/out',candidate)==ui['compiled_outputs'],'candidate compiled223 roster differs')
            for row in ui['dependency_files']:
                need(sha(candidate/'packages/vscode'/row['path'])==row['sha256'],'candidate public dependency differs')
            after=normalized_blobs(candidate,tree)
            need(all(after[name]==row['blob'] for name,row in tree.items()),'normalized Git bytes changed during raw overlay')
            need(all(sha(candidate/row['path'])==sha(root/row['path'])==row['sha256'] for row in plan['source_files']),
                 'raw source equality changed during transfer')
            need(native_seal(candidate)==native_seal(root)==actual,'C7 identity changed after generated transfer')
            need(sha(Path(__file__))==plan['helper_sha256'],'helper changed during transfer')
            record(dict(started,kind='completed',utc=datetime.now(timezone.utc).isoformat(),passed=True,
                candidate_source_after=actual,normalized_git_tree_unchanged=True,
                scope='raw source/input transfer only; no build/package/installed qualification'))
        except BaseException as error:
            record(dict(started,kind='failed_retained',utc=datetime.now(timezone.utc).isoformat(),passed=False,
                error={'type':type(error).__name__,'detail':str(error)},cleanup='none; partial owned candidate retained; no rollback/replay'))
            raise
    print(json.dumps({'receipt':str(args.receipt),'passed':True,'source_copied':started['source_copied'],
        'compiled_copied':started['compiled_copied'],'dependency_copied':started['dependency_copied']}))


def self_test(output):
    tests=[]
    def check(name,condition):
        need(condition,'private helper regression failed: '+name);tests.append(name)
    for name in ('artifacts/private/state.json','temp/source.txt','.airk/index.db','.codex/config.toml',
                 'packages/vscode/node_modules/package/file.js','packages/vscode/out/extension.js'):
        check('source-refuses-'+name,not public_name(name))
    for name in ('../escape','/absolute','C:/profile','a\\b','a/../b','a\nchild'):
        try:relative_name(name)
        except ValueError:check('unsafe-relative-refused-'+repr(name),True)
        else:raise AssertionError('unsafe relative accepted')
    check('public-source-retained',public_name('packages/vscode/src/nativeMetadataCodec.js'))
    check('reviewed-public-site-source-retained',public_name('site/index.html'))
    check('same-security-only',same_security({'owner_sid':HUMAN,'sddl':'D:','file_id':[1,2]},
                                           {'owner_sid':HUMAN,'sddl':'D:','file_id':[1,2]}))
    for key,value in (('owner_sid','foreign'),('sddl','D:changed'),('file_id',[1,3])):
        prior={'owner_sid':HUMAN,'sddl':'D:','file_id':[1,2]};changed=dict(prior,**{key:value})
        check('metadata-change-refused-'+key,not same_security(prior,changed))
    entries={'node_modules/root':{},'node_modules/dep':{},'node_modules/root/node_modules/dep':{}}
    check('nested-node-resolution',resolve_dependency('node_modules/root','dep',entries)=='node_modules/root/node_modules/dep')
    check('hoisted-node-resolution',resolve_dependency('node_modules/other','dep',entries)=='node_modules/dep')
    value={'kind':'private-helper-pure-tests','selected':len(tests),'passed':len(tests),'failed':0,'tests':tests,
        'limits':'pure selector/metadata checks only; no candidate or fixture writes, Git mutation, staging or build'}
    with output.open('x',encoding='utf-8',newline='\n') as stream:json.dump(value,stream,indent=2);stream.write('\n')
    print(json.dumps(value))


def main():
    os.environ['PYTHONDONTWRITEBYTECODE']='1'
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    plan=sub.add_parser('prepare')
    for name in ('root','candidate','handoff','output'):plan.add_argument('--'+name,type=Path,required=True)
    plan.add_argument('--approved-handoff-sha256',required=True)
    plan.add_argument('--commit',help='postcommit readiness requires both worktrees already at this exact commit')
    action=sub.add_parser('apply')
    action.add_argument('--plan',type=Path,required=True);action.add_argument('--approved-plan-sha256',required=True)
    action.add_argument('--commit',required=True);action.add_argument('--receipt',type=Path,required=True)
    test=sub.add_parser('self-test');test.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.command=='prepare':prepare(args)
    elif args.command=='apply':apply(args)
    else:
        identity();need(args.output.parent.resolve(strict=True)==private_output_root(),'pure-test output escaped approved private helper directory');human_path(args.output.parent.resolve(strict=True),private_output_root());self_test(args.output)


if __name__=='__main__':main()
