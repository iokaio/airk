# SPDX-License-Identifier: Apache-2.0
"""One approved B2 eight-image data preservation; no native execution or build."""
from pathlib import Path
import hashlib,json,os,shutil,struct,datetime
B=Path(__file__).resolve().parent
R=Path(r'D:\code\Github\airk')
E=R/'artifacts/v190/final-plan/exec-20261116T052942Z'
RECEIPT=E/'current-b2-test-image-cargo-json-r1.json'
LOG=E/'current-b2-test-image-cargo-json-r1.log'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
GROUP='18ee803e50e57e63af1e69057b6dcafb1829c69e5cf2d250f89256157e558bbe'
COMMIT='dda749cbdf77b3e2106003ff10a683ec3aec3202'
PACKAGES=['airk-service','airk-inference','airk-retrieval','airk-kernel','airk-mcp','airkctl','airk-cli']
def sha(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pin(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)}
def pe(p):
    with p.open('rb') as f:
        head=f.read(64);assert len(head)==64 and head[:2]==b'MZ'
        offset=int.from_bytes(head[60:64],'little');assert 64<=offset<=1<<20
        f.seek(offset);h=f.read(26);assert len(h)==26 and h[:4]==b'PE\0\0'
        machine=int.from_bytes(h[4:6],'little');magic=int.from_bytes(h[24:26],'little')
        assert (machine,magic)==(0x8664,0x20b)
        return {'machine_u16':machine,'optional_header_magic_u16':magic}
receipt=json.loads(RECEIPT.read_text(encoding='utf-8'))
assert receipt['schema']=='airk-readiness-command/1' and receipt['status']=='completed' and receipt['passed'] is True
assert receipt['exit_code']==receipt['runner_exit_code']==0
for key in ('inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged'):assert receipt[key] is True
assert receipt['changed_groups']==[] and receipt['groups']['native']['sha256']==GROUP
assert receipt['log']==LOG.name
assert receipt['command'][1:]==['test','--workspace','--all-features','--no-run','--message-format=json','--locked','--timings']
rows=[]
for line in LOG.read_text(encoding='utf-8').splitlines():
    if line.lstrip().startswith('{'):rows.append(json.loads(line))
assert len([r for r in rows if r.get('reason')=='build-finished' and r.get('success') is True])==1
ort=[r for r in rows if r.get('reason')=='build-script-executed' and r.get('package_id','').endswith('#ort-sys@2.0.0-rc.10')]
assert ort and all(r['linked_libs']==['static=onnxruntime'] and r['linked_paths']==[r'native=D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r5\aggregate'] for r in ort)
selected=[]
for package in PACKAGES+['airk-cli::combined_reads']:
    pkg=package.split('::')[0];kind='test' if '::' in package else 'lib';name='combined_reads' if kind=='test' else pkg.replace('-','_')
    matches=[r for r in rows if r.get('reason')=='compiler-artifact' and r.get('profile',{}).get('test') is True
       and r.get('executable') and r.get('target',{}).get('kind')==[kind] and r['target'].get('name')==name
       and Path(r.get('manifest_path','')).resolve()==(R/'crates'/pkg/'Cargo.toml').resolve()]
    assert len(matches)==1,(package,len(matches))
    row=matches[0];assert rows.count(row)==1
    selected.append((package,row))
owners={r['path']:r['owner_sid'] for r in json.loads((B/'source-owners.json').read_text(encoding='utf-8'))}
# Validate every original before creating any preserved image.
for package,row in selected:
    p=Path(row['executable']);s=p.stat();assert owners[str(p)]==SID and p.is_file() and not p.is_symlink()
    assert s.st_nlink==1 and 0<s.st_size<=4<<30 and p.resolve().is_relative_to(Path(receipt['target_directory']).resolve())
    pe(p)
images=B/'images';images.mkdir(exist_ok=False)
result=[];bindings=[]
for package,row in selected:
    original=Path(row['executable']);before=original.stat();observed=sha(original);target=images/original.name
    with original.open('rb') as source,target.open('xb') as dest:shutil.copyfileobj(source,dest,1<<20)
    assert sha(target)==observed==sha(original)
    after=original.stat();assert (before.st_ino,before.st_size,before.st_mtime_ns)==(after.st_ino,after.st_size,after.st_mtime_ns)
    assert target.stat().st_nlink==1 and target.stat().st_ino!=original.stat().st_ino
    machine=pe(target)
    image={'binary_id':package,'path':str(target),'bytes':target.stat().st_size,'sha256':observed,
       'original_path':str(original),'original_file_owner_sid':owners[str(original)],'preserved_file_owner_sid':SID}
    result.append(image);bindings.append({'materialized_image':image,'actual_cargo_artifact':row,'pe':machine})
    print(json.dumps({'preserved':package,'path':str(target),'sha256':observed}),flush=True)
assert sha(RECEIPT)==pin(RECEIPT)['sha256']
inventory={'schema':'airk.preserved-native-test-images/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'source_commit':COMMIT,'native_group':GROUP,'actual_build_receipt':{'path':str(RECEIPT),'sha256':sha(RECEIPT)},
 'images':result,'scope':'Actual all-feature Cargo no-run test-image inventory and unchanged byte copies; no testcase counts inferred, selected model execution, qualification, signing or publication.'}
(B/'native-test-images-b2-r1.json').write_text(json.dumps(inventory,indent=2)+'\n',encoding='utf-8')
side={'schema':'airk.private-B2-image-preservation-binding/1','inventory':pin(B/'native-test-images-b2-r1.json'),
 'build_receipt':pin(RECEIPT),'cargo_json':pin(LOG),'actual_static_ORT_rows':ort,'images':bindings,
 'testcases_listed_or_run':False,'NN_invocations':0,'artifact_or_target_approval':'pending explicit Root review; none invented'}
(B/'cargo-image-bindings.json').write_text(json.dumps(side,indent=2)+'\n',encoding='utf-8')