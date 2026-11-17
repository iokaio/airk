# SPDX-License-Identifier: Apache-2.0
"""Prepare only a public source transfer roster; never copy source to Neo."""
import hashlib,importlib.util,json,subprocess,time
from pathlib import Path,PurePosixPath
ROOT=Path.cwd().resolve()
OUT=ROOT/'artifacts/local/one-airk-1814'/('neo-transfer-preparation-'+str(int(time.time())));OUT.mkdir()
def git(*args):return subprocess.check_output(['git','-c','safe.directory='+ROOT.as_posix(),*args],cwd=ROOT)
top={'.cargo','.config','.dockerignore','.editorconfig','.gitattributes','.github','.gitignore','AGENTS.md','build.ps1','build.sh','Cargo.lock','Cargo.toml','CLAUDE.md','CODE_OF_CONDUCT.md','compose.yaml','CONTRIBUTING.md','crates','deny.toml','Dockerfile','docs','examples','LICENSE','NOTICE','packages','README.md','rust-toolchain.toml','scripts','SECURITY.md','site','supply-chain','SUPPORT.md','third_party','THIRD_PARTY_NOTICES.md','tools'}
names=set(name.decode() for name in git('ls-files','-z','--cached','--others','--exclude-standard').split(b'\0') if name)
rows=[];excluded=[]
for name in sorted(names):
 parts=PurePosixPath(name).parts
 if parts[0] not in top:excluded.append(name);continue
 if any(part in {'temp','target','artifacts','node_modules','.git','.codex','.agents','.aws'} for part in parts):raise ValueError('Nonpublic source roster input: '+name)
 path=ROOT/name
 if not path.exists():continue
 if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(ROOT):raise ValueError('Unsafe source input: '+name)
 raw=path.read_bytes()
 if len(raw)>32*1024*1024:raise ValueError('Oversized public source input: '+name)
 rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
spec=importlib.util.spec_from_file_location('neo_preparation_runtime_builder',ROOT/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
report={'schema':1,'status':'PROVISIONAL_SOURCE_STILL_CHANGING_NO_TRANSFER_OR_BUILD_AUTHORITY','head':git('rev-parse','HEAD').decode().strip(),'tree':git('rev-parse','HEAD^{tree}').decode().strip(),'dirty':bool(git('status','--porcelain')),'native_source':builder.source_identity(),'files':rows,'file_count':len(rows),'bytes':sum(row['bytes'] for row in rows),'excluded_private_configuration_paths':excluded,'source_roster_sha256':hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(',',':')).encode()).hexdigest(),'final_freeze_required':True,'git_context':'Transfer authentic HEAD bundle then overwrite tracked/untracked public paths with exact captured worktree bytes; do not normalize line endings, commit, or invent a clean identity.','forbidden':'No temp/, private/index state, artifacts/, target/, node_modules/, .git worktree files, credential/config directories in public source archive.'}
(OUT/'provisional-source-manifest.json').write_text(json.dumps(report,indent=2)+'\n')
model=ROOT/'target/local-vector-model';assets=[]
for path in sorted(model.rglob('*')):
 if path.is_file():
  if path.is_symlink():raise ValueError('Model source redirect')
  raw=path.read_bytes();assets.append({'path':path.relative_to(model).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(OUT/'public-model-transfer-manifest.json').write_text(json.dumps({'schema':1,'source':str(model),'files':assets,'bytes':sum(row['bytes'] for row in assets),'destination':'offline-model outside Cargo target; verify with frozen provision-local-model.py without --download before use'},indent=2)+'\n')
print(json.dumps({'directory':str(OUT),'native_source_provisional':report['native_source'],'public_files':report['file_count'],'source_bytes':report['bytes'],'model_files':len(assets),'model_bytes':sum(row['bytes'] for row in assets),'excluded_config_count':len(excluded)},indent=2))
