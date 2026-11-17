# SPDX-License-Identifier: Apache-2.0
"""Read-only exact historical command-template proof; never run a wrapper."""
import hashlib
import json
from pathlib import Path, PureWindowsPath
import re
import subprocess

ROOT=Path(__file__).resolve().parents[4]
HERE=Path(__file__).resolve().parent
GENERATOR_REV='e5b4e6dbfd929ab6835bbd7bf652c0de66bfd8d3^'
GENERATOR_PATH='packages/vscode/src/cliInstall.ts'
LITERAL='@echo off\\r\\n"${launcher}" %*\\r\\nexit /b %errorlevel%\\r\\n'

def digest(raw):return hashlib.sha256(raw).hexdigest()
def git(*args):return subprocess.check_output(['git','-C',str(ROOT),*args])

def reproduce(raw,literal):
    expected='@echo off\\r\\n"${launcher}" %*\\r\\nexit /b %errorlevel%\\r\\n'
    if literal!=expected:raise ValueError('historical generator template changed')
    match=re.fullmatch(rb'@echo off\r\n"([^"\r\n]+)" %\*\r\nexit /b %errorlevel%\r\n',raw)
    if not match:raise ValueError('not the full Windows launcher template')
    launcher=match[1].decode('utf-8')
    if not PureWindowsPath(launcher).is_absolute() or any(c in launcher for c in '"%!'):
        raise ValueError('parameter violates historical Windows launcher guard')
    template=literal.replace('\\r','\r').replace('\\n','\n')
    emitted=template.replace('${launcher}',launcher).encode('utf-8')
    if emitted!=raw:raise ValueError('incomplete template reproduction')
    return launcher

def main():
    commit=git('rev-parse',GENERATOR_REV).decode().strip()
    subprocess.run(['git','-C',str(ROOT),'merge-base','--is-ancestor',commit,'HEAD'],check=True)
    source=git('show',commit+':'+GENERATOR_PATH);oid=git('rev-parse',commit+':'+GENERATOR_PATH).decode().strip()
    text=source.decode('utf-8');anchor="const content=platform==='win32'?`"+LITERAL+'`'
    if text.count(anchor)!=1:raise ValueError('template not uniquely bound to historical generator')
    if "/[\"%!]/.test(launcher)" not in text:raise ValueError('historical parameter policy missing')
    inventory=(ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z/source-broad-delta-r1.json').read_bytes()
    if digest(inventory)!='ce0b4e7eb0a4755796c4cb8d325165276b749deec49f0963b0af4fec8718d5fc':raise ValueError('reviewed inventory changed')
    rows=[{'source':r['path'],'original_sha256':r['source_sha256']} for r in json.loads(inventory)['lanes']['component_review']];proof=[]
    for row in rows:
        if not row['source'].endswith('/service-cli/bin/airk.cmd'):continue
        raw=(ROOT/row['source']).read_bytes()
        if digest(raw)!=row['original_sha256']:raise ValueError('source changed')
        launcher=reproduce(raw,LITERAL)
        proof.append({'source':row['source'],'source_sha256':digest(raw),'source_bytes':len(raw),'disposition':'generated_template','reason':'Entire output exactly reconstructs a reachable Git-owned template with the historical parameter refusal policy; parameter is recovered from observed output, not an execution authorization.','generated':{'git_generator':{'commit':commit,'path':GENERATOR_PATH,'blob_oid':oid,'sha256':digest(source),'bytes':len(source)},'template_literal':LITERAL,'template_sha256':digest(LITERAL.encode()),'parameters':{'launcher':launcher},'reproduced_sha256':digest(raw)},'evidence':[{'path':Path(__file__).resolve().relative_to(ROOT).as_posix(),'sha256':digest(Path(__file__).read_bytes()),'bytes':Path(__file__).stat().st_size}]})
    if len(proof)!=24:raise ValueError('expected exact24 historical wrappers')
    print(json.dumps({'schema':'airk.source-disposition/1','code_executed':False,'qualification':False,'files':proof},indent=2))
if __name__=='__main__':main()
