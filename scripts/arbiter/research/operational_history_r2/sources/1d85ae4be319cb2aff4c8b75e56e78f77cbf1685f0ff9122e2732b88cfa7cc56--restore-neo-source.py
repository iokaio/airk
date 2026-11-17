# SPDX-License-Identifier: Apache-2.0
"""Inspect public transfer inputs; archives require --finalize --freeze-from.

Artifact-only helper: no Cargo, signer, SSH or user configuration changes.
The authentic HEAD index stays unstaged while exact dirty bytes are restored.
"""
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile

sys.dont_write_bytecode = True
os.environ['GIT_OPTIONAL_LOCKS'] = '0'
os.environ['GIT_NO_REPLACE_OBJECTS'] = '1'
PUBLIC = {'.cargo','.config','.dockerignore','.editorconfig','.gitattributes','.github','.gitignore','AGENTS.md','build.ps1','build.sh','Cargo.lock','Cargo.toml','CLAUDE.md','CODE_OF_CONDUCT.md','compose.yaml','CONTRIBUTING.md','crates','deny.toml','Dockerfile','docs','examples','LICENSE','NOTICE','packages','README.md','rust-toolchain.toml','scripts','SECURITY.md','site','supply-chain','SUPPORT.md','third_party','THIRD_PARTY_NOTICES.md','tools','installers'}
FORBIDDEN = {'temp','target','artifacts','builds','node_modules','__pycache__','.git','.codex','.agents','.aws','.ssh','.airk','.claude','.idea','.vscode'}
PRIVATE_SUFFIXES = {'.pem','.key','.p12','.pfx','.p8','.sqlite','.sqlite3','.db','.db3'}
MAX_FILE, MAX_SOURCE, MAX_MODELS = 32*1024*1024, 256*1024*1024, 96*1024*1024
FROZEN = 'FROZEN_PUBLIC_SOURCE'
PACKAGE_NAMES = ('public-source.tar.gz','public-models.tar.gz','authentic-head.bundle','source-manifest.json','public-model-transfer-manifest.json','restore-neo-source.py')
GIT_CONTEXT = {'GIT_DIR','GIT_WORK_TREE','GIT_INDEX_FILE','GIT_COMMON_DIR','GIT_OBJECT_DIRECTORY','GIT_ALTERNATE_OBJECT_DIRECTORIES','GIT_REPLACE_REF_BASE','GIT_NAMESPACE','GIT_SHALLOW_FILE','GIT_GRAFT_FILE','GIT_CONFIG','GIT_CONFIG_COUNT','GIT_CONFIG_PARAMETERS','GIT_TEMPLATE_DIR','GIT_EXEC_PATH','GIT_CEILING_DIRECTORIES','GIT_DISCOVERY_ACROSS_FILESYSTEM'}
def git_environment(environment):
    return {key:value for key,value in environment.items() if key not in GIT_CONTEXT and not key.startswith(('GIT_CONFIG_KEY_','GIT_CONFIG_VALUE_'))}
def sha(data): return hashlib.sha256(data).hexdigest()
def canonical(value): return json.dumps(value,sort_keys=True,separators=(',',':')).encode('utf-8')

def plain(path, regular=False):
    path=Path(os.path.abspath(path))
    for part in (path,*path.parents):
        if part.exists() or part.is_symlink():
            row=part.lstat()
            if stat.S_ISLNK(row.st_mode) or getattr(row,'st_file_attributes',0)&0x400: raise ValueError('Redirected transfer path')
    if regular:
        row=path.lstat()
        if not stat.S_ISREG(row.st_mode) or row.st_nlink!=1: raise ValueError('Transfer input must be regular and single-link')
    return path

def bounded_bytes(path, maximum=MAX_FILE):
    path=plain(path,True); before=path.stat()
    if before.st_size>maximum: raise ValueError('Transfer file exceeds budget')
    with path.open('rb') as source:
        opened=os.fstat(source.fileno());data=source.read(maximum+1);after=os.fstat(source.fileno())
    final=path.stat();identity=lambda x:(x.st_dev,x.st_ino,x.st_size,x.st_mtime_ns)
    # Windows Python path.stat/fstat expose different ctime meanings. Compare
    # ctime within each API, and the common physical identity across APIs.
    if len(data)!=before.st_size or identity(before)!=identity(opened) or identity(opened)!=identity(after) or identity(after)!=identity(final) or before.st_ctime_ns!=final.st_ctime_ns or opened.st_ctime_ns!=after.st_ctime_ns: raise ValueError('Transfer file changed during capture')
    return data

def json_file(path): return json.loads(bounded_bytes(path,4*1024*1024).decode('utf-8'))
def safe_name(name, public=True):
    if not isinstance(name,str) or not name or len(name)>4096 or '\\' in name or ':' in name or any(ord(c)<32 for c in name): raise ValueError('Unsafe transfer-relative path')
    parts=PurePosixPath(name).parts
    if PurePosixPath(name).is_absolute() or any(x in ('','.','..') for x in name.split('/')): raise ValueError('Transfer path escapes its root')
    for part in parts:
        p=part.lower()
        if p in FORBIDDEN or p in {'.env','.mcp.json'} or p.startswith('.env.') or PurePosixPath(p).suffix in PRIVATE_SUFFIXES or p.endswith(('-wal','-shm','.local.toml')): raise ValueError('Private/index/cache path in public transfer')
    if public and parts[0] not in PUBLIC: raise ValueError('Path outside explicit public source roots')
    return name

def git(root,*args,input=None):
    # Spill metadata output rather than allocating an unbounded communicate()
    # result. Exact tree/file counts and sizes are checked before bulk reads.
    with tempfile.TemporaryFile() as output,tempfile.TemporaryFile() as errors:
        subprocess.run(['git','-c','safe.directory='+Path(root).as_posix(),*args],cwd=root,input=input,stdout=output,stderr=errors,timeout=60,check=True,env=git_environment(os.environ))
        if output.tell()>128*1024*1024 or errors.tell()>1024*1024: raise ValueError('Git metadata exceeds budget')
        output.seek(0);return output.read(128*1024*1024+1)

def load(root,filename):
    spec=importlib.util.spec_from_file_location('airk_transfer_'+filename.replace('-','_'),plain(root/'scripts'/filename,True)); module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def head_rows(root):
    rows=[]
    for entry in git(root,'ls-tree','-r','-z','--full-tree','HEAD').split(b'\0'):
        if not entry: continue
        metadata,name=entry.split(b'\t',1);mode,kind,oid=metadata.decode('ascii').split(' ')
        if kind!='blob' or mode not in ('100644','100755'): raise ValueError('Unsupported HEAD link/submodule/mode')
        rows.append({'path':safe_name(name.decode('utf-8')),'mode':mode,'object':oid})
    return sorted(rows,key=lambda row:row['path'])

def check_index(root,heads):
    rows=[]
    for entry in git(root,'ls-files','--stage','-z').split(b'\0'):
        if not entry: continue
        metadata,name=entry.split(b'\t',1);mode,oid,stage=metadata.decode('ascii').split(' ')
        if stage!='0': raise ValueError('Unresolved index')
        rows.append({'path':safe_name(name.decode('utf-8')),'mode':mode,'object':oid})
    if sorted(rows,key=lambda row:row['path'])!=heads: raise ValueError('Staged changes require separate review; never unstage or fake HEAD')
    if any(not row.startswith(b'H ') for row in git(root,'ls-files','-v','-z').split(b'\0') if row): raise ValueError('Special index flags need separate review; never discard them silently')

def audit_history(root):
    # Check every reachable tree entry, including reused trees/blobs under a
    # second path; rev-list's one displayed name per object is insufficient.
    objects=git(root,'rev-list','--objects','--no-object-names','HEAD').splitlines()
    if len(objects)>100000: raise ValueError('Git history object budget exceeded')
    checked=git(root,'cat-file','--batch-check=%(objectname) %(objecttype) %(objectsize)',input=b'\n'.join(objects)+b'\n')
    trees=[];total=0
    for line in checked.splitlines():
        oid,kind,size=line.split(b' ')
        if kind==b'tree':
            size=int(size);total+=size
            if size>4*1024*1024 or total>64*1024*1024: raise ValueError('Git tree budget exceeded')
            trees.append(oid)
    data=git(root,'cat-file','--batch',input=b'\n'.join(trees)+b'\n');offset=0
    width=20 if git(root,'rev-parse','--show-object-format').strip()==b'sha1' else 32
    for oid in trees:
        end=data.index(b'\n',offset);actual,kind,size=data[offset:end].split(b' ')
        if actual!=oid or kind!=b'tree': raise ValueError('Crossed Git tree identity')
        size=int(size);body=data[end+1:end+1+size];cursor=0
        while cursor<len(body):
            space=body.index(b' ',cursor);zero=body.index(b'\0',space+1)
            safe_name(body[space+1:zero].decode('utf-8'),False);cursor=zero+1+width
        if cursor!=len(body): raise ValueError('Malformed Git tree')
        offset=end+1+size
        if data[offset:offset+1]!=b'\n': raise ValueError('Malformed Git tree framing')
        offset+=1
    if offset!=len(data): raise ValueError('Extra Git history bytes')
    return {'reachable_objects':len(objects),'reachable_trees':len(trees),'tree_bytes':total,'private_tree_entries':0}

def source_snapshot(root):
    heads=head_rows(root);check_index(root,heads);modes={r['path']:int(r['mode'],8)&0o777 for r in heads}
    names=sorted(set(x.decode('utf-8') for x in git(root,'ls-files','-z','--cached','--others','--exclude-standard').split(b'\0') if x))
    rows=[];seen=set();excluded=set();total=0
    for name in names:
        if PurePosixPath(name).parts[0] not in PUBLIC: excluded.add(PurePosixPath(name).parts[0]);continue
        safe_name(name);path=root/name
        if not path.exists() and not path.is_symlink(): continue
        data=bounded_bytes(path)
        if name.casefold() in seen: raise ValueError('Case-colliding source paths')
        seen.add(name.casefold());mode=modes.get(name,0o644) if os.name=='nt' else stat.S_IMODE(path.stat().st_mode)
        if mode not in (0o644,0o755): raise ValueError('Unsupported public source mode')
        total+=len(data)
        if len(rows)>=10000 or total>MAX_SOURCE: raise ValueError('Public source budget exceeded')
        rows.append({'path':name,'bytes':len(data),'sha256':sha(data),'mode':mode})
    native=load(root,'build-service-runtime.py').source_identity();status=git(root,'status','--porcelain=v1','-z','--untracked-files=all');current={r['path'] for r in rows}
    return {'schema':1,'head':git(root,'rev-parse','HEAD').decode().strip(),'tree':git(root,'rev-parse','HEAD^{tree}').decode().strip(),'object_format':git(root,'rev-parse','--show-object-format').decode().strip(),'dirty':bool(status),'git_status_sha256':sha(status),'index_matches_head':True,'head_files':heads,'deleted_from_head':[r for r in heads if r['path'] not in current],'native_source':native,'files':rows,'file_count':len(rows),'bytes':total,'source_roster_sha256':sha(canonical(rows)),'excluded_source_roots':sorted(excluded)}

def model_snapshot(root,directory):
    owner=load(root,'provision-local-model.py');expected={}
    for profile in owner.CATALOG['profiles']:
        assets={a['name']:(a['url'],a['bytes'],a['sha256']) for a in profile['assets']};owner.validate_asset_bindings(profile,assets)
        for name,(_url,size,digest) in assets.items(): expected[profile['id']+'/'+name]=(size,digest)
        payload=(json.dumps(profile['manifest'],indent=2)+'\n').encode('utf-8');expected[profile['id']+'/model.json']=(len(payload),sha(payload))
    actual=set()
    for path in directory.rglob('*'):
        plain(path)
        if path.is_file(): actual.add(path.relative_to(directory).as_posix())
    if actual!=set(expected): raise ValueError('Only exact pinned public model assets/receipts may be exported')
    rows=[]
    for name,(size,digest) in sorted(expected.items()):
        data=bounded_bytes(directory/name,MAX_MODELS)
        if len(data)!=size or sha(data)!=digest: raise ValueError('Model differs from frozen public catalog')
        rows.append({'path':name,'bytes':size,'sha256':digest,'mode':0o644})
    total=sum(r['bytes'] for r in rows)
    if total>MAX_MODELS: raise ValueError('Public model budget exceeded')
    return {'schema':1,'files':rows,'file_count':len(rows),'bytes':total,'model_roster_sha256':sha(canonical(rows)),'destination':'offline-model outside Cargo target'}

def write_json(path,value):
    with path.open('xb') as stream: stream.write((json.dumps(value,indent=2)+'\n').encode('utf-8'));stream.flush();os.fsync(stream.fileno())

def write_archive(filename,source,rows):
    with filename.open('xb') as raw:
        with gzip.GzipFile(filename='',fileobj=raw,mode='wb',mtime=0) as zipped:
            with tarfile.open(fileobj=zipped,mode='w|',format=tarfile.PAX_FORMAT) as archive:
                for row in rows:
                    data=bounded_bytes(source/row['path'],MAX_MODELS if row['path'].endswith('model.onnx') else MAX_FILE)
                    if len(data)!=row['bytes'] or sha(data)!=row['sha256']: raise ValueError('Input changed after freeze roster')
                    info=tarfile.TarInfo(row['path']);info.size=len(data);info.mode=row['mode'];info.mtime=0;info.uid=info.gid=0;info.uname=info.gname=''
                    archive.addfile(info,io.BytesIO(data))
        raw.flush();os.fsync(raw.fileno())

def archive_members(filename,rows,destination=None,public=True):
    expected={r['path']:r for r in rows};seen=set()
    if len(expected)!=len(rows): raise ValueError('Duplicate frozen manifest members')
    with tarfile.open(filename,'r|gz') as archive:
        for member in archive:
            name=safe_name(member.name,public);row=expected.get(name)
            if not row or name in seen or not member.isfile() or member.size!=row['bytes'] or member.mode!=row['mode'] or member.mode not in (0o644,0o755): raise ValueError('Archive member differs from frozen inventory')
            if member.size>(MAX_MODELS if not public else MAX_FILE): raise ValueError('Archive member budget exceeded')
            data=archive.extractfile(member).read(row['bytes']+1)
            if len(data)!=row['bytes'] or sha(data)!=row['sha256']: raise ValueError('Archive bytes differ')
            if destination is not None:
                target=plain(destination/name);target.parent.mkdir(parents=True,exist_ok=True)
                with target.open('xb') as stream: stream.write(data);stream.flush();os.fsync(stream.fileno())
                target.chmod(row['mode'])
            seen.add(name)
    if seen!=set(expected): raise ValueError('Archive omitted frozen inputs')

def package_files(directory):
    return {name:{'bytes':(directory/name).stat().st_size,'sha256':sha(bounded_bytes(directory/name,256*1024*1024))} for name in PACKAGE_NAMES}

def produce(args):
    root=plain(args.root).resolve(strict=True);base=plain(root/'artifacts/local/one-airk-1.8.14/clean-break-r37-neo').resolve(strict=True);output=plain(args.output)
    if plain(git(root,'rev-parse','--show-toplevel').decode().strip()).resolve(strict=True)!=root: raise ValueError('Selected source is not the exact repository root')
    if output.parent!=base or output.exists(): raise ValueError('Choose a fresh immediate child of the owned transfer artifact directory')
    source=source_snapshot(root);models_root=plain(args.model_dir or root/'target/local-vector-model').resolve(strict=True);models=model_snapshot(root,models_root)
    helper_bytes=bounded_bytes(Path(__file__))
    source.update(public_models_roster_sha256=models['model_roster_sha256'],export_helper_sha256=sha(helper_bytes),history_audit=audit_history(root),status=FROZEN if args.finalize else 'PROVISIONAL_SOURCE_STILL_CHANGING_NO_TRANSFER_OR_BUILD_AUTHORITY',final_freeze_required=not args.finalize)
    if args.finalize:
        if not args.freeze_from: raise ValueError('--finalize requires a reviewed --freeze-from inspection manifest')
        frozen=json_file(args.freeze_from)
        for key in ('head','tree','object_format','native_source','files','head_files','deleted_from_head','source_roster_sha256','public_models_roster_sha256','git_status_sha256','export_helper_sha256'):
            if source[key]!=frozen.get(key): raise ValueError('Reviewed freeze no longer matches: '+key)
    output.mkdir(mode=0o700)
    if args.finalize:
        write_archive(output/'public-source.tar.gz',root,source['files']);write_archive(output/'public-models.tar.gz',models_root,models['files'])
        git(root,'bundle','create',str(output/'authentic-head.bundle'),'HEAD');git(root,'bundle','verify',str(output/'authentic-head.bundle'))
        if git(root,'bundle','list-heads',str(output/'authentic-head.bundle')).decode().splitlines()!=[source['head']+' HEAD']: raise ValueError('Bundle does not contain exactly authentic HEAD')
        with (output/'restore-neo-source.py').open('xb') as stream: stream.write(helper_bytes);stream.flush();os.fsync(stream.fileno())
        archive_members(output/'public-source.tar.gz',source['files']);archive_members(output/'public-models.tar.gz',models['files'],public=False)
    after=source_snapshot(root)
    for key in ('head','tree','object_format','native_source','files','head_files','deleted_from_head','git_status_sha256'):
        if after[key]!=source[key]: raise ValueError('Source changed during capture: '+key)
    if model_snapshot(root,models_root)!=models: raise ValueError('Models changed during capture')
    if sha(bounded_bytes(Path(__file__)))!=source['export_helper_sha256']: raise ValueError('Export helper changed during capture')
    write_json(output/'source-manifest.json',source);write_json(output/'public-model-transfer-manifest.json',models);receipt_sha=None
    if args.finalize:
        receipt={'schema':1,'status':FROZEN,'native_source':source['native_source'],'files':package_files(output),'transfer_performed':False,'build_performed':False}
        write_json(output/'transfer-receipt.json',receipt);receipt_sha=sha(bounded_bytes(output/'transfer-receipt.json'))
    print(json.dumps({'directory':str(output),'status':source['status'],'head':source['head'],'native_source':source['native_source'],'public_files':source['file_count'],'source_roster_sha256':source['source_roster_sha256'],'public_model_files':models['file_count'],'public_models_roster_sha256':models['model_roster_sha256'],'transfer_receipt_sha256':receipt_sha,'transfer_performed':False,'build_performed':False},indent=2))

def restore(args):
    package=plain(args.restore_from).resolve(strict=True);raw=bounded_bytes(package/'transfer-receipt.json')
    if not args.expected_receipt_sha256 or sha(raw)!=args.expected_receipt_sha256: raise ValueError('Independently pinned receipt SHA256 required')
    receipt=json.loads(raw)
    if receipt.get('schema')!=1 or receipt.get('status')!=FROZEN or receipt.get('files')!=package_files(package): raise ValueError('Package differs from pinned receipt')
    source=json_file(package/'source-manifest.json');models=json_file(package/'public-model-transfer-manifest.json')
    if source.get('status')!=FROZEN or source.get('final_freeze_required') is not False or source['source_roster_sha256']!=sha(canonical(source['files'])) or models['model_roster_sha256']!=sha(canonical(models['files'])) or source['public_models_roster_sha256']!=models['model_roster_sha256']: raise ValueError('Complete frozen source/models required')
    if sha(bounded_bytes(package/'restore-neo-source.py'))!=source['export_helper_sha256']: raise ValueError('Restore helper differs from frozen reviewed code')
    archive_members(package/'public-source.tar.gz',source['files']);archive_members(package/'public-models.tar.gz',models['files'],public=False)
    campaign=plain(args.campaign)
    if campaign.exists() or not campaign.parent.is_dir(): raise ValueError('Choose a new campaign under an existing owned parent')
    campaign.mkdir(mode=0o700);target=campaign/'source';target.mkdir(mode=0o700);template=campaign/'empty-git-template';template.mkdir(mode=0o700)
    environment={**git_environment(os.environ),'GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':os.devnull,'GIT_TERMINAL_PROMPT':'0'}
    subprocess.run(['git','init','--template='+str(template),'--object-format='+source['object_format'],str(target)],env=environment,check=True,capture_output=True)
    subprocess.run(['git','-C',str(target),'fetch','--no-tags',str(package/'authentic-head.bundle'),'HEAD'],env=environment,check=True,capture_output=True)
    subprocess.run(['git','-C',str(target),'update-ref','--no-deref','HEAD',source['head']],env=environment,check=True)
    subprocess.run(['git','-C',str(target),'read-tree','HEAD'],env=environment,check=True)
    empty=target/'.git/airk-empty-excludes';empty.write_bytes(b'');git(target,'config','--local','core.excludesFile',str(empty));git(target,'config','--local','core.autocrlf','false')
    archive_members(package/'public-source.tar.gz',source['files'],target);offline=campaign/'offline-model';offline.mkdir(mode=0o700);archive_members(package/'public-models.tar.gz',models['files'],offline,False)
    if git(target,'rev-parse','HEAD').decode().strip()!=source['head'] or git(target,'rev-parse','HEAD^{tree}').decode().strip()!=source['tree']: raise ValueError('Restored Git identity differs')
    check_index(target,source['head_files'])
    for row in source['deleted_from_head']:
        if (target/row['path']).exists() or (target/row['path']).is_symlink(): raise ValueError('Deleted tracked source resurrected')
    restored=source_snapshot(target)
    for key in ('head','tree','object_format','dirty','head_files','deleted_from_head','files','native_source','source_roster_sha256'):
        if restored[key]!=source[key]: raise ValueError('Restored source identity differs; do not build: '+key)
    if model_snapshot(target,offline)!=models: raise ValueError('Restored model identity differs')
    subprocess.run([sys.executable,str(target/'scripts/provision-local-model.py'),'--destination',str(offline)],check=True,env={**environment,'PYTHONDONTWRITEBYTECODE':'1'})
    shutil.copyfile(package/'source-manifest.json',campaign/'source-manifest.json');shutil.copyfile(package/'public-model-transfer-manifest.json',campaign/'public-model-transfer-manifest.json')
    write_json(campaign/'restore-receipt.json',{'schema':1,'status':'RESTORED_FROZEN_PUBLIC_SOURCE','head':source['head'],'tree':source['tree'],'native_source':source['native_source'],'source_roster_sha256':source['source_roster_sha256'],'transfer_receipt_sha256':args.expected_receipt_sha256,'authentic_head_index':True,'dirty_overlay_not_staged_or_committed':True,'build_performed':False})
    print(json.dumps({'campaign':str(campaign),'source':str(target),'offline_models':str(offline),'native_source':source['native_source'],'build_performed':False},indent=2))

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path);p.add_argument('--output',type=Path);p.add_argument('--model-dir',type=Path);p.add_argument('--finalize',action='store_true');p.add_argument('--freeze-from',type=Path);p.add_argument('--restore-from',type=Path);p.add_argument('--campaign',type=Path);p.add_argument('--expected-receipt-sha256');args=p.parse_args()
    # Frozen builder.source_identity() invokes Git itself. Clear only this
    # helper process's context overrides too, never the caller's environment.
    for key in list(os.environ):
        if key not in git_environment(os.environ): os.environ.pop(key,None)
    if args.restore_from:
        if not args.campaign or args.finalize or args.output or args.freeze_from: p.error('Restore needs campaign/pinned receipt, without producer options')
        restore(args)
    else:
        args.root=args.root or Path(__file__).resolve().parents[3]
        if not args.output: p.error('Choose a fresh --output artifact directory; inspection is default')
        produce(args)

if __name__=='__main__':
    try: main()
    except (ValueError,OSError,subprocess.SubprocessError) as error:
        print('Source transfer refused: '+str(error),file=sys.stderr);raise SystemExit(1)
