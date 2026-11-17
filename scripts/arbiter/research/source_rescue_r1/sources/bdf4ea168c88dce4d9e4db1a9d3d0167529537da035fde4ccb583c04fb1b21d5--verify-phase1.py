# SPDX-License-Identifier: Apache-2.0
import collections, datetime, hashlib, importlib.util, json, pathlib, subprocess, sys
P=pathlib.Path(__file__).resolve().parent
plan=json.loads((P/'plan.json').read_text()); C=pathlib.Path(plan['candidate_root']); R=pathlib.Path(plan['runtime_dir'])
def load(p): return json.loads(p.read_text(encoding='utf-8-sig'))
def sha(p):
    with p.open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest()
def put(p,v):
    with p.open('x',encoding='utf-8',newline='\n') as f: json.dump(v,f,indent=2); f.write('\n')
def mod(name):
    spec=importlib.util.spec_from_file_location(name.replace('-','_'),C/'scripts'/f'{name}.py'); m=importlib.util.module_from_spec(spec); sys.modules[spec.name]=m; spec.loader.exec_module(m); return m
def inventory(root):
    result={}
    for p in root.rglob('*'):
        assert not p.is_symlink(),str(p)
        if p.is_file(): result[p.relative_to(root).as_posix()]={'sha256':sha(p),'bytes':p.stat().st_size}
    return result
def source_seals():
    pins=load(P/'candidate-input-pins.json')
    for key,group in pins.items():
        base=C if key=='sources' else C/'packages/vscode/out'
        for name,digest in group.items(): assert sha(base/name)==digest,name
    assert subprocess.check_output(['git','status','--porcelain'],cwd=C)==b''
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=C,text=True).strip()==plan['candidate_head']
    for row in load(P/'preserved-vsix-pins.json').values(): assert sha(pathlib.Path(row['path']))==row['sha256']
    return {k:len(v) for k,v in pins.items()}
old=set(load(P/'phase1-before-runs.json')['prior_local_runs']); runs=[p for p in (C/'artifacts/local-service-installer').iterdir() if p.is_dir() and str(p) not in old]; assert len(runs)==1
run=runs[0]; wpath=P/'driver/windows-installer-r3.json'; w=load(wpath); lp=run/'receipt.json'; local=load(lp)
for key in ('passed','inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged'): assert w[key] is True,key
assert w['exit_code']==0 and not w['evidence_errors'] and not w['interrupted']
assert local['result']=='built' and local['signed'] and not local['dirty'] and local['commit']==plan['candidate_head']
assert local['input_runtime']['source']==plan['C7']
signer=mod('sign-native'); adapter=mod('local_signing'); package=mod('package-native'); legal=mod('license-notices')
paths={'airk':run/'qualified-runtime/airk.exe','airkctl':run/'qualified-runtime/airkctl.exe','uninstaller':run/'signed/uninstaller.exe','installer':pathlib.Path(local['installer'])}
signatures={}
for component,binary in paths.items():
    receipt_path=run/f'signing-{component}.json'; doc=load(receipt_path); signer.local_component(doc)
    assert doc['commit']==plan['candidate_head']; assert sha(binary)==doc['binary_sha256']
    if component in ('airk','airkctl'): assert doc['source_sha256']==plan['runtime_images'][component+'.exe']
    if component=='uninstaller': assert doc['source_sha256']==local['signing']['uninstaller_unsigned_sha256']
    if component=='installer': assert sha(binary)==local['installer_sha256']
    verification=signer.verify(binary,doc,str(adapter.signtool_path()))
    signatures[component]={'path':str(binary),'sha256':sha(binary),'bytes':binary.stat().st_size,'source_sha256':doc['source_sha256'],'receipt_sha256':sha(receipt_path),'verification':verification}
original=inventory(R); tested=inventory(run/'tested-runtime'); qualified=inventory(run/'qualified-runtime'); assert original==tested; assert len(original)==846
assert original.keys()==qualified.keys(); changed={k for k in original if original[k]!=qualified[k]}; assert changed=={'airk.exe','airkctl.exe','airk-install.json','airk-runtime.json','airk-build.json'},changed
package.validate_runtime(R,local['version'],local['target'],plan['C7']); package.validate_runtime(run/'qualified-runtime',local['version'],local['target'],plan['C7'])
for name,digest in w['runtime_seal']['files'].items(): assert sha(R/name)==digest,name
layout=run/'layout'; payload=load(layout/'payload-inventory.json'); full=inventory(layout); assert len(payload['files'])==843 and len(full)==844
assert payload['files']==local['files']; assert sha(layout/'payload-inventory.json')==local['payload_inventory_sha256']; assert {k:v for k,v in full.items() if k!='payload-inventory.json'}==payload['files']
research={}
for src,(dest,digest) in legal.RESEARCH_COMPANIONS.items():
    points=[C/src,R/'notices'/dest,run/'tested-runtime/notices'/dest,run/'qualified-runtime/notices'/dest,layout/'notices/dependencies'/dest]
    assert all(sha(q)==digest for q in points),dest
    research[dest]={'sha256':digest,'bytes':points[0].stat().st_size,'five_source_runtime_layout_locations_exact':True}
assert len(research)==11
pins=source_seals()
proof={'schema':'airk.private.windows-installer-candidate-r3-phase1-proof/1','observed_utc':datetime.datetime.now(datetime.UTC).isoformat(),'passed':True,'local_run':str(run),'outer_receipt':{'path':str(wpath),'sha256':sha(wpath),'elapsed_seconds':w['elapsed_seconds']},'local_receipt':{'path':str(lp),'sha256':sha(lp)},'source':plan['C7'],'profile':local['input_runtime']['profile'],'signatures':signatures,'runtime_files':len(original),'layout_files':843,'layout_complete_files':844,'research_companions':research,'payload_inventory_sha256':sha(layout/'payload-inventory.json'),'source_pin_counts_unchanged':pins,'original_runtime_and_root_vsix_unchanged':True,'candidate_clean':True,'steps':local['steps'],'scope':'Phase1 signing and verification only. No adoption/Cargo/ZIP, installer execution, user install/trust change or signed full-client acceptance.'}
put(P/'phase1-artifact-proof.json',proof)
print(json.dumps({'passed':True,'local_run':str(run),'installer':signatures['installer'],'elapsed_seconds':w['elapsed_seconds']}))
