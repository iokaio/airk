# SPDX-License-Identifier: Apache-2.0
"""Capture one public shallow commit plus the exact tested working bytes, never the original .git or ignored state."""
import hashlib,importlib.util,io,json,os,subprocess,sys,tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];out=ROOT/'artifacts/v190'/sys.argv[1]
assert out.parent==ROOT/'artifacts/v190' and out.name.startswith('p6f-neo-full-public-') and not out.exists()
environment=dict(os.environ,GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=ROOT.as_posix())
def git(*args,cwd=ROOT):return subprocess.check_output(['git',*args],cwd=cwd,env=environment)
assert not git('status','--porcelain','--untracked-files=no').strip(),'Commit the complete reviewed public source before full capture'
commit=git('rev-parse','HEAD').decode().strip();tree=git('rev-parse','HEAD^{tree}').decode().strip()
entries={}
for entry in git('ls-tree','-r','-z','HEAD').split(b'\0'):
 if not entry:continue
 metadata,raw=entry.split(b'\t',1);mode,kind,oid=metadata.split();name=raw.decode();assert mode in [b'100644',b'100755'] and kind==b'blob',(name,mode,kind)
 assert not name.startswith(('temp/','artifacts/','.codex/','.aws/','.git/')) and name not in ['.mcp.json'],name
 source=ROOT/name;assert source.is_file() and not source.is_symlink() and source.resolve().is_relative_to(ROOT),name
 entries[name]={'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'mode':int(mode,8)}
spec=importlib.util.spec_from_file_location('native_builder',ROOT/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);native=builder.source_identity()
expected=json.loads((ROOT/'artifacts/v190/p6f-resume-final-release-runtime-r5/airk-build.json').read_bytes())['source'];assert native==expected
out.mkdir();clone=out/'public-git'
subprocess.run(['git','clone','--no-local','--no-tags','--depth','1','--single-branch','--no-checkout',ROOT.as_uri(),str(clone)],env=environment,check=True)
assert git('rev-parse','HEAD',cwd=clone).decode().strip()==commit
assert git('rev-list','--count','HEAD',cwd=clone).strip()==b'1'
all_objects=set(git('cat-file','--batch-all-objects','--batch-check=%(objectname)',cwd=clone).splitlines());reachable={line.split()[0] for line in git('rev-list','--objects','HEAD',cwd=clone).splitlines()};assert all_objects==reachable,'Unexpected nonpublic or historical Git objects'
archive=out/'source.tar.gz'
with tarfile.open(archive,'w:gz') as packed:
 for name,record in sorted(entries.items()):
  body=(ROOT/name).read_bytes();assert hashlib.sha256(body).hexdigest()==record['sha256'];entry=tarfile.TarInfo(name);entry.size=len(body);entry.mode=0o755 if record['mode']==0o100755 else 0o644;entry.mtime=int((ROOT/name).stat().st_mtime);packed.addfile(entry,io.BytesIO(body))
 for path in sorted((clone/'.git').rglob('*')):
  if not path.is_file():continue
  assert not path.is_symlink();name=path.relative_to(clone).as_posix()
  if name.startswith(('.git/logs/','.git/hooks/')):continue
  body=path.read_bytes();entry=tarfile.TarInfo(name);entry.size=len(body);entry.mode=0o600;entry.mtime=int(path.stat().st_mtime);packed.addfile(entry,io.BytesIO(body))
assert builder.source_identity()==native
assert all(hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==record['sha256'] for name,record in entries.items())
assert git('rev-parse','HEAD').decode().strip()==commit and not git('status','--porcelain','--untracked-files=no').strip()
record={'schema':'airk.public-full-source-snapshot/1','commit':commit,'tree':tree,'native_source':native,'public_files':entries,'source_file_count':len(entries),'git_history_depth':1,'git_objects_are_exactly_current_public_commit_objects':True,'original_git_or_ignored_state_copied':False,'unrelated_untracked_notes_excluded':['docs/research/new-issues.md'],'working_bytes_preserved':True,'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest()}
(out/'snapshot.json').write_text(json.dumps(record,indent=2));print(json.dumps({k:v for k,v in record.items() if k!='public_files'}))
