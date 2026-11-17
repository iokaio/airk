# SPDX-License-Identifier: Apache-2.0
"""Prepare a private frozen public-source packet only; no SSH/Cargo/sign/build."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[3]
REMOTE_BASE='7ca445f8c56194243a99f314d693df22f5923197'
def load(name,file):
    spec=importlib.util.spec_from_file_location(name,ROOT/file)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
owner=load('mac_source_receipt_owner','scripts/dev/receipt_runner.py')
runtime=load('mac_source_runtime_owner','scripts/build-service-runtime.py')
GIT=['git','-c','safe.directory='+ROOT.as_posix()]
def git(*arguments):return subprocess.check_output([*GIT,*arguments],cwd=ROOT)
def head():return git('rev-parse','HEAD').decode().strip()
def snapshot():
    groups=owner.observe(['native','harness','fixtures','frontend'])
    files={name:value for group in groups.values() for name,value in group['files'].items()}
    for path in owner.tracked_files(('docs','README.md','CONTRIBUTING.md','AGENTS.md','CLAUDE.md','.gitignore','.gitattributes','.gitmodules')):
        files[path.relative_to(ROOT).as_posix()]=owner.digest(path) if path.is_file() else 'missing'
    return {'head':head(),'groups':groups,'files':files,'native_source_identity':runtime.source_identity(),
            'preparer_sha256':owner.digest(Path(__file__))}
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,default=ROOT/'artifacts/advanced-split/mac-delivery-frozen-source-r1')
    parser.add_argument('--prepare-reviewed',action='store_true')
    parser.add_argument('--expected-source-sha256')
    args=parser.parse_args();before=snapshot()
    source_sha=hashlib.sha256(json.dumps(before,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    out=args.out.resolve()
    if not out.is_relative_to(ROOT/'artifacts') or out==ROOT/'artifacts':raise ValueError('Private output required')
    if not args.prepare_reviewed:
        print(json.dumps({'source_sha256':source_sha,'source':before,'out':str(out),'executed':False},indent=2));return 0
    if args.expected_source_sha256!=source_sha:raise ValueError('Reviewed source freeze differs')
    if subprocess.run([*GIT,'diff','--cached','--quiet'],cwd=ROOT).returncode:
        raise ValueError('Do not modify/stage user work; source packet requires the reviewed unstaged-overlay strategy')
    if out.exists():raise ValueError('Fresh output required')
    out.mkdir(parents=True)
    modes={}
    for entry in git('ls-files','--stage','-z').split(b'\0'):
        if entry:
            header,name=entry.split(b'\t',1);modes[os.fsdecode(name)]=int(header.split()[0],8)
    rows={}
    with tarfile.open(out/'public-source.tar.gz','w:gz') as archive:
        for name,expected in sorted(before['files'].items()):
            path=ROOT/name
            if set(Path(name).parts)&{'..','.git','artifacts','temp','.airk','node_modules','target'}:
                raise ValueError('Private/redirected source path refused: '+name)
            if path.is_symlink() or any(parent.is_symlink() for parent in path.parents):raise ValueError('Source alias refused')
            if expected=='missing':rows[name]={'sha256':'missing'};continue
            data=path.read_bytes()
            if hashlib.sha256(data).hexdigest()!=expected:raise ValueError('Source changed during capture: '+name)
            member=tarfile.TarInfo(name);member.size=len(data);member.mode=0o755 if modes.get(name)==0o100755 else 0o644
            archive.addfile(member,io.BytesIO(data));rows[name]={'sha256':expected,'bytes':len(data),'mode':member.mode}
    if snapshot()!=before:raise ValueError('Source changed during capture; retained incomplete evidence, do not transfer')
    subprocess.run([*GIT,'bundle','create',str(out/'source.bundle'),'HEAD','^'+REMOTE_BASE],cwd=ROOT,check=True)
    if snapshot()!=before:raise ValueError('Source/HEAD changed during Git bundle capture')
    manifest={'schema':'airk.private-independent-mac-source/1','source_sha256':source_sha,'head':before['head'],
        'remote_base':REMOTE_BASE,'source_identity':before['native_source_identity'],'groups':before['groups'],'files':rows,
        'public_archive_sha256':owner.digest(out/'public-source.tar.gz'),'git_bundle_sha256':owner.digest(out/'source.bundle'),
        'source_remains_unchanged':True,'source_copy_build_sign_install_publish_executed':False}
    (out/'source.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'source_sha256':source_sha,'files':len(rows),'source_json_sha256':owner.digest(out/'source.json'),
        'archive_sha256':manifest['public_archive_sha256'],'git_bundle_sha256':manifest['git_bundle_sha256']}))
    return 0
if __name__=='__main__':raise SystemExit(main())
