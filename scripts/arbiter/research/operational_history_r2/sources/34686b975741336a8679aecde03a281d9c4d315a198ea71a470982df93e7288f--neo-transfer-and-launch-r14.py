# SPDX-License-Identifier: Apache-2.0
"""Verify reviewed R14 pins; --execute explicitly starts one unsigned Neo batch."""
import argparse,hashlib,json,os,re,subprocess,sys,time
from pathlib import Path

BASE=Path(__file__).resolve().parents[1]
RUNNER_BASE=Path(__file__).resolve().parent
HOST='tylerjensen@192.168.68.61'
KEY='C:/Users/tdotn/.ssh/airk_mac_build_ed25519'
PYTHON='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
ROOT='/Users/tylerjensen/airk-builds'


def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def launch():
    stamp=str(time.time_ns())
    evidence=BASE/('neo-transfer-r14-'+stamp);evidence.mkdir(mode=0o700)
    package_remote=ROOT+'/one-airk-1814-transfer-r14-'+stamp
    control_remote=ROOT+'/one-airk-1814-control-r14-'+stamp
    campaign=ROOT+'/one-airk-1814-final-r14-'+stamp
    receipt={'schema':1,'reviewed_pins_sha256':PINS_SHA256,'host':'192.168.68.61','source_transfer':'running','campaign':campaign,'remote_package':package_remote,'remote_control':control_remote,'native_source':NATIVE,'source_roster_sha256':ROSTER,'transfer_receipt_sha256':RECEIPT,'steps':[],'build_started':False,'review_gate':'primary and independent reviewer approved exact pins before --execute'}
    def save():(evidence/'owner-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    def run(stage,argv,script=None,timeout=900):
        print(stage,flush=True);row={'stage':stage,'argv':list(map(str,argv))};receipt['steps'].append(row);save();started=time.monotonic()
        result=subprocess.run(row['argv'],input=script,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=timeout,**({'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}))
        (evidence/(stage+'.stdout')).write_text(result.stdout);(evidence/(stage+'.stderr')).write_text(result.stderr)
        row.update(exit_code=result.returncode,elapsed_seconds=round(time.monotonic()-started,3));save()
        if result.returncode:raise RuntimeError(stage+' failed; preserve complete evidence')
        return result.stdout
    ssh=['ssh','-i',KEY,'-oBatchMode=yes','-oStrictHostKeyChecking=yes',HOST,PYTHON,'-']
    try:
        assert len(RECEIPT)==64 and NATIVE.get('sha256') and RUNNERS, 'Root-approved source freeze and independently pinned runner/package identities required'
        assert sha(PACKAGE/'transfer-receipt.json')==RECEIPT
        frozen=json.loads((PACKAGE/'transfer-receipt.json').read_text());manifest=json.loads((PACKAGE/'source-manifest.json').read_text())
        assert frozen['status']==manifest['status']=='FROZEN_PUBLIC_SOURCE' and manifest['final_freeze_required'] is False
        assert frozen['native_source']==manifest['native_source']==NATIVE and manifest['file_count']==PINNED_SOURCE_FILE_COUNT and manifest['source_roster_sha256']==ROSTER
        assert manifest['public_models_roster_sha256']==MODELS_ROSTER
        assert sha(PACKAGE/'restore-neo-source.py')==RESTORER
        expected={'public-source.tar.gz','public-models.tar.gz','authentic-head.bundle','source-manifest.json','public-model-transfer-manifest.json','restore-neo-source.py'}
        assert set(frozen['files'])==expected
        for name,row in frozen['files'].items():assert (PACKAGE/name).is_file() and not (PACKAGE/name).is_symlink() and (PACKAGE/name).stat().st_size==row['bytes'] and sha(PACKAGE/name)==row['sha256']
        for name,digest in RUNNERS.items():assert sha(RUNNER_BASE/name)==digest
        cfg={'package':package_remote,'control':control_remote,'campaign':campaign,'receipt_sha256':RECEIPT,'restorer_sha256':RESTORER,'native':NATIVE,'head':manifest['head'],'runners':RUNNERS,'python':PYTHON}
        code='from pathlib import Path\nimport json,os,platform,socket\ncfg=json.loads('+repr(json.dumps(cfg))+')\nassert os.getuid()==501 and platform.machine()=="arm64" and socket.gethostname()=="Tylers-MacBook-Neo.local"\nbase=Path('+repr(ROOT)+').resolve(strict=True)\nfor key in ("package","control"):\n p=Path(cfg[key]);assert p.parent==base and not p.exists() and not p.is_symlink();p.mkdir(mode=0o700)\nassert not Path(cfg["campaign"]).exists()\nprint(json.dumps({"owned_directories_created":True,"campaign_absent":True}))\n'
        readiness_code="""from pathlib import Path
import json,os,platform,resource,shutil,socket,subprocess
assert os.getuid()==501 and platform.machine()=='arm64' and socket.gethostname()=='Tylers-MacBook-Neo.local'
rows=subprocess.check_output(['/bin/ps','-axo','pid=,uid=,comm='],text=True).splitlines()
compiler_names={'cargo','rustc','rust-lld','ld','clang','clang++','cc','cc1','cc1plus','cmake','ninja','make'}
active=[]
for row in rows:
 fields=row.strip().split(None,2)
 if len(fields)==3 and fields[1]=='501' and Path(fields[2]).name in compiler_names:active.append({'pid':int(fields[0]),'uid':501,'image':fields[2]})
print(json.dumps({'read_only':True,'active_compilers':active,'fd_limits':list(resource.getrlimit(resource.RLIMIT_NOFILE)),'free_bytes':shutil.disk_usage('/Users/tylerjensen/airk-builds').free,'GUI_Code_qualified':False,'signing_started':False}))
assert not active, 'Compiler owner must finish before Neo batch; never stop foreign work'
assert resource.getrlimit(resource.RLIMIT_NOFILE)[1]>=4096
"""
        run('read-only-host-readiness',ssh,readiness_code,timeout=90)
        run('create-owned-destinations',ssh,code)
        for name in sorted(expected|{'transfer-receipt.json'}):run('copy-'+name,['scp','-i',KEY,'-oBatchMode=yes','-oStrictHostKeyChecking=yes',str(PACKAGE/name),HOST+':'+package_remote+'/'+name])
        for name in RUNNERS:run('copy-'+name,['scp','-i',KEY,'-oBatchMode=yes','-oStrictHostKeyChecking=yes',str(RUNNER_BASE/name),HOST+':'+control_remote+'/'+name])
        code='''from pathlib import Path
import hashlib,json,os,shutil,subprocess,time
cfg=json.loads(CONFIG)
def sha(p):
 with Path(p).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
package=Path(cfg['package']);control=Path(cfg['control']);campaign=Path(cfg['campaign'])
assert sha(package/'transfer-receipt.json')==cfg['receipt_sha256']
assert sha(package/'restore-neo-source.py')==cfg['restorer_sha256']
receipt=json.loads((package/'transfer-receipt.json').read_text())
assert receipt['status']=='FROZEN_PUBLIC_SOURCE' and receipt['native_source']==cfg['native']
assert {p.name for p in package.iterdir()}==set(receipt['files'])|{'transfer-receipt.json'}
for name,row in receipt['files'].items():
 p=package/name;assert p.is_file() and not p.is_symlink() and p.stat().st_size==row['bytes'] and sha(p)==row['sha256']
for name,digest in cfg['runners'].items():assert sha(control/name)==digest
environment={**os.environ,'PYTHONNOUSERSITE':'1','PYTHONDONTWRITEBYTECODE':'1'}
result=subprocess.run([cfg['python'],str(package/'restore-neo-source.py'),'--restore-from',str(package),'--campaign',str(campaign),'--expected-receipt-sha256',cfg['receipt_sha256']],capture_output=True,text=True,env=environment,timeout=600)
(control/'restore.stdout').write_text(result.stdout);(control/'restore.stderr').write_text(result.stderr)
assert result.returncode==0, 'Authentic public source restore failed; retain logs'
restored=json.loads((campaign/'restore-receipt.json').read_text())
assert restored['status']=='RESTORED_FROZEN_PUBLIC_SOURCE' and restored['native_source']==cfg['native'] and restored['head']==cfg['head']
git=subprocess.check_output(['git','-c','safe.directory='+str(campaign/'source'),'-C',str(campaign/'source'),'rev-parse','HEAD'],text=True).strip()
assert git==cfg['head']
for name,digest in cfg['runners'].items():
 shutil.copy2(control/name,campaign/name);assert sha(campaign/name)==digest
stdout=(campaign/'unsigned-owner.stdout').open('xb');stderr=(campaign/'unsigned-owner.stderr').open('xb')
argv=[cfg['python'],str(campaign/'neo-final-owner.py'),'--campaign',str(campaign),'--native-sha256',cfg['native']['sha256'],'--previous-runtime','/Users/tylerjensen/airk-builds/explorer-notary-1812-ee5ad77-20261107/source3c6e81c/artifacts/neo/signed-notarized-3c/runtime']
child=subprocess.Popen(argv,cwd=campaign,env=environment,stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr,start_new_session=True,close_fds=True)
stdout.close();stderr.close()
launch={'schema':1,'pid':child.pid,'pgid':os.getpgid(child.pid),'argv':argv,'campaign':str(campaign),'native_source':cfg['native'],'authentic_head':git,'runners':cfg['runners'],'started_unix':time.time(),'restore_receipt_sha256':sha(campaign/'restore-receipt.json')}
assert launch['pgid']==child.pid
(campaign/'unsigned-launch.json').write_text(json.dumps(launch,indent=2)+'\\n')
print(json.dumps(launch))
'''.replace('CONFIG',repr(json.dumps(cfg)),1)
        output=run('verify-restore-launch',ssh,code,timeout=900);launch=json.loads(output)
        receipt.update(source_transfer='verified_and_restored',build_started=True,launch=launch);save();print(json.dumps({'receipt':str(evidence/'owner-receipt.json'),'campaign':campaign,'launch':launch},indent=2),flush=True)
        (BASE/'neo-active-r14.json').write_text(json.dumps({'local_evidence':str(evidence),'campaign':campaign,'package':package_remote,'control':control_remote,'launch':launch},indent=2)+'\n')
    except BaseException as error:
        receipt['source_transfer']='failed_preserved';receipt['error']=str(error);save();raise



def validate_pins(value):
    if value.get('status')!='REVIEWED_READY_FOR_SINGLE_NEO_BATCH' or value.get('primary_reviewed') is not True or value.get('independent_reviewed') is not True or value.get('inputs_frozen') is not True or value.get('windows_stage3_complete') is not True:
        raise ValueError('Explicit primary/independent review and input freeze required before transfer')
    for key in ('transfer_receipt_sha256','restorer_sha256','source_roster_sha256','public_models_roster_sha256','launcher_sha256','windows_stage3_receipt_sha256'):
        if not re.fullmatch('[0-9a-f]{64}',str(value.get(key,''))):raise ValueError('Missing final frozen digest: '+key)
    native=value.get('native_source',{})
    if not re.fullmatch('[0-9a-f]{64}',str(native.get('sha256',''))) or not isinstance(native.get('files'),int) or native['files']<=0:
        raise ValueError('Missing final native identity')
    if not isinstance(value.get('source_file_count'),int) or value['source_file_count']<=0:
        raise ValueError('Missing final source count')
    runners=value.get('runners',{})
    if set(runners)!={'neo-final-owner.py'} or any(not re.fullmatch('[0-9a-f]{64}',str(digest)) for digest in runners.values()):
        raise ValueError('Exactly one reviewed unsigned owner required; no signed/GUI launch')
    raw_package=Path(value['package']).absolute()
    package=raw_package.resolve(strict=True)
    if raw_package.is_symlink() or raw_package!=package or package.parent!=BASE or not package.name.startswith('neo-frozen-transfer-r14'):
        raise ValueError('Package must be a reviewed R14 public export under the existing artifact root')
    return value

def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--approved-pins',type=Path,required=True)
    parser.add_argument('--execute',action='store_true',help='after final primary review, transfer and launch the single frozen unsigned batch')
    args=parser.parse_args(argv)
    if sys.flags.optimize:raise ValueError('Optimized Python would discard inherited source/ownership assertions')
    pins=validate_pins(json.loads(args.approved_pins.read_text()))
    if sha(Path(__file__))!=pins['launcher_sha256']:raise ValueError('Reviewed launcher changed')
    global PACKAGE,RECEIPT,RESTORER,NATIVE,ROSTER,PINNED_SOURCE_FILE_COUNT,RUNNERS,MODELS_ROSTER,PINS_SHA256
    PACKAGE=Path(pins['package']).resolve(strict=True)
    RECEIPT=pins['transfer_receipt_sha256'];RESTORER=pins['restorer_sha256'];NATIVE=pins['native_source'];ROSTER=pins['source_roster_sha256']
    PINNED_SOURCE_FILE_COUNT=pins['source_file_count'];RUNNERS=pins['runners'];MODELS_ROSTER=pins['public_models_roster_sha256'];PINS_SHA256=sha(args.approved_pins)
    if not args.execute:
        print(json.dumps({'status':'REVIEWED_PINS_ACCEPTED_NO_TRANSFER_OR_BUILD','package':str(PACKAGE),'pins_sha256':PINS_SHA256}))
        return 0
    launch()
    return 0

if __name__=='__main__':raise SystemExit(main())
