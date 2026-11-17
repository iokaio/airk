# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,shutil,subprocess,tempfile
from pathlib import Path
run=Path(__file__).resolve().parent;runtime=run/'source/artifacts/neo/signed-gui1/runtime'
def sha(path):
 with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
main=runtime/'AIRK.app/Contents/MacOS/airk';image=runtime/'AIRK.app/Contents/MacOS/airk-launcher-image';before={str(p):sha(p) for p in [main,image]}
with tempfile.TemporaryDirectory(prefix='airk-negative-sealed-copy-') as temporary:
 owner=Path(temporary);copied=owner/'invalid-main-copy';shutil.copyfile(main,copied);copied.chmod(0o700)
 assert sha(copied)==before[str(main)]
 checked=subprocess.run(['/usr/bin/codesign','--verify','--strict','--verbose=2',str(copied)],capture_output=True,text=True,timeout=30)
 negative={'copied_sha256':sha(copied),'source_sha256':before[str(main)],'exit_code':checked.returncode,'stdout':checked.stdout,'stderr':checked.stderr,'executed':False}
 assert checked.returncode!=0 and 'Info.plist' in checked.stderr
 spec=importlib.util.spec_from_file_location('signer',run/'source/scripts/sign-native.py');signer=importlib.util.module_from_spec(spec);spec.loader.exec_module(signer)
 receipt=json.loads((runtime.parent/'signing.json').read_text());component=receipt['components']['AIRK.app/Contents/MacOS/airk-launcher-image']
 positive=signer.verify_copied_launcher(runtime,'1.8.12',component)
after={str(p):sha(p) for p in [main,image]};assert after==before
report={'schema':'airk.neo-sealed-main-negative-copy-proof/1','status':'passed','negative':negative,'approved_standalone_positive':positive,'owned_temporary_removed':not owner.exists(),'sources_unchanged':True,'target_absent':not (run/'target').exists(),'signing_or_keychain_changes':False}
(run/'evidence/sealed-main-negative-copy-proof.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
