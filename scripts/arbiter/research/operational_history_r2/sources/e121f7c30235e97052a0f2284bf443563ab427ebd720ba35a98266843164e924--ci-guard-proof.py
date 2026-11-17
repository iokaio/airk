# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,subprocess
from pathlib import Path
directory=Path(__file__).resolve().parent;run=directory.parent.parent
helper=directory/'ci-macos-sign.py'
report={'schema':'airk.neo-ci-certificate-guard-proof/1','helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'operations':'Read-only public certificate and valid-identity metadata queries; public embedded certificate extraction. No signing/import/unlock/password/keychain setting changes.'}
spec=importlib.util.spec_from_file_location('ci_guard',helper);ci=importlib.util.module_from_spec(spec);spec.loader.exec_module(ci)
try:
 identity=ci.imported_identity(Path.home()/'Library/Keychains/airk-signing.keychain-db');report['imported_identity']=identity
 runtime=run/'source/artifacts/neo/signed-gui1/runtime';checks=[]
 for name in ['AIRK.app','AIRK.app/Contents/MacOS/airk','AIRK.app/Contents/MacOS/airk-embed','AIRK.app/Contents/MacOS/airk-launcher-image','AIRK.app/Contents/MacOS/libonnxruntime.dylib']:
  ci.verify_signed_certificate(runtime/name,identity);checks.append(name)
 report['verified_signed_certificates']=checks;report['status']='passed'
except BaseException as error:report['status']='failed';report['error']={'type':type(error).__name__,'error':str(error)}
report['target_absent']=not (run/'target').exists()
ps=subprocess.run(['/bin/ps','-axo','comm='],capture_output=True,text=True,timeout=15)
report['compiler_processes']=[line.strip() for line in ps.stdout.splitlines() if Path(line.strip()).name in ['rustc','cargo','clippy-driver']]
(directory/'proof.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
