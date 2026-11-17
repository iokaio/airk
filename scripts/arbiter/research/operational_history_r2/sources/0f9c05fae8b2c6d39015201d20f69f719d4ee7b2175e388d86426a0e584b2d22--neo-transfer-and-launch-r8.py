# SPDX-License-Identifier: Apache-2.0
"""Verify the authorized frozen public R8 package, restore, and launch one Neo batch."""
import hashlib,json,os,subprocess,sys,time
from pathlib import Path

BASE=Path(__file__).resolve().parent
PACKAGE=BASE/'neo-frozen-transfer-r8'
HOST='tylerjensen@192.168.68.61'
KEY='C:/Users/tdotn/.ssh/airk_mac_build_ed25519'
PYTHON='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
ROOT='/Users/tylerjensen/airk-builds'
RECEIPT='b02787113de4f036e7ddf15f08145d57bba3fd49b9f1bb80b1f7658be34b9736'
RESTORER='cbac59e5284d71c59706013c0c44db482b510bbf66fdefaf68933770ec4e92ca'
NATIVE={'sha256':'6723387a63f6daff54e74d40a78992b59ff7648e22467686904d5d016fe36b06','files':1114}
ROSTER='336405cb7eb18407ef920d25c064f6c985cbf2de69c89b40a97af37c7ed0ac5b'
RUNNERS={'neo-final-owner.py':'6e17d507fdf24a1a9710a87be7e8beafd150de011b45beeb368f2e9404fb1126','neo-final-signed-owner.py':'1bcb181c9729009425ccf8bf822c052bd39473e8341dfd257ea7977e9e25e675'}

def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def main():
    stamp=str(time.time_ns())
    evidence=BASE/('neo-transfer-r8-'+stamp);evidence.mkdir(mode=0o700)
    package_remote=ROOT+'/one-airk-1814-transfer-r8-'+stamp
    control_remote=ROOT+'/one-airk-1814-control-r8-'+stamp
    campaign=ROOT+'/one-airk-1814-final-r8-'+stamp
    receipt={'schema':1,'host':'192.168.68.61','source_transfer':'running','campaign':campaign,'remote_package':package_remote,'remote_control':control_remote,'native_source':NATIVE,'source_roster_sha256':ROSTER,'transfer_receipt_sha256':RECEIPT,'steps':[],'build_started':False}
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
        assert sha(PACKAGE/'transfer-receipt.json')==RECEIPT
        frozen=json.loads((PACKAGE/'transfer-receipt.json').read_text());manifest=json.loads((PACKAGE/'source-manifest.json').read_text())
        assert frozen['status']==manifest['status']=='FROZEN_PUBLIC_SOURCE' and manifest['final_freeze_required'] is False
        assert frozen['native_source']==manifest['native_source']==NATIVE and manifest['file_count']==2058 and manifest['source_roster_sha256']==ROSTER
        assert manifest['public_models_roster_sha256']=='c8e33ba41ebb92f3cc4f65cf1e28e945cfa148a3dbb30b01e99e6e69e0d7868e'
        assert sha(PACKAGE/'restore-neo-source.py')==RESTORER
        expected={'public-source.tar.gz','public-models.tar.gz','authentic-head.bundle','source-manifest.json','public-model-transfer-manifest.json','restore-neo-source.py'}
        assert set(frozen['files'])==expected
        for name,row in frozen['files'].items():assert (PACKAGE/name).is_file() and not (PACKAGE/name).is_symlink() and (PACKAGE/name).stat().st_size==row['bytes'] and sha(PACKAGE/name)==row['sha256']
        for name,digest in RUNNERS.items():assert sha(BASE/name)==digest
        cfg={'package':package_remote,'control':control_remote,'campaign':campaign,'receipt_sha256':RECEIPT,'restorer_sha256':RESTORER,'native':NATIVE,'head':manifest['head'],'runners':RUNNERS,'python':PYTHON}
        code='from pathlib import Path\nimport json,os,platform,socket\ncfg=json.loads('+repr(json.dumps(cfg))+')\nassert os.getuid()==501 and platform.machine()=="arm64" and socket.gethostname()=="Tylers-MacBook-Neo.local"\nbase=Path('+repr(ROOT)+').resolve(strict=True)\nfor key in ("package","control"):\n p=Path(cfg[key]);assert p.parent==base and not p.exists() and not p.is_symlink();p.mkdir(mode=0o700)\nassert not Path(cfg["campaign"]).exists()\nprint(json.dumps({"owned_directories_created":True,"campaign_absent":True}))\n'
        run('create-owned-destinations',ssh,code)
        for name in sorted(expected|{'transfer-receipt.json'}):run('copy-'+name,['scp','-i',KEY,'-oBatchMode=yes','-oStrictHostKeyChecking=yes',str(PACKAGE/name),HOST+':'+package_remote+'/'+name])
        for name in RUNNERS:run('copy-'+name,['scp','-i',KEY,'-oBatchMode=yes','-oStrictHostKeyChecking=yes',str(BASE/name),HOST+':'+control_remote+'/'+name])
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
        (BASE/'neo-active-r8.json').write_text(json.dumps({'local_evidence':str(evidence),'campaign':campaign,'package':package_remote,'control':control_remote,'launch':launch},indent=2)+'\n')
    except BaseException as error:
        receipt['source_transfer']='failed_preserved';receipt['error']=str(error);save();raise

if __name__=='__main__':main()
