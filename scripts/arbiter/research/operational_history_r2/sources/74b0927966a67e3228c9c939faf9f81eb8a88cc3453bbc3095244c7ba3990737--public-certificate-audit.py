# SPDX-License-Identifier: Apache-2.0
"""Extract only certificates already public in the successful signed images."""
import hashlib,json,subprocess
from pathlib import Path
run=Path(__file__).resolve().parent;runtime=run/'source/artifacts/neo/signed-gui1/runtime'
assert json.loads((run/'gui-sign-result.json').read_text())['status']=='passed'
out=run/'evidence/public-certificate-identity-r2';out.mkdir(mode=0o700,exist_ok=False)
images={'app':runtime/'AIRK.app','launcher':runtime/'AIRK.app/Contents/MacOS/airk-launcher-image'}
records={}
for name,image in images.items():
 directory=out/name;directory.mkdir(mode=0o700)
 verify=subprocess.run(['/usr/bin/codesign','--verify','--strict','--verbose=2',str(image)],capture_output=True,text=True,timeout=30)
 assert verify.returncode==0,verify.stderr
 display=subprocess.run(['/usr/bin/codesign','--display','--verbose=4','--extract-certificates='+str(directory/'certificate-'),str(image)],capture_output=True,text=True,timeout=30)
 assert display.returncode==0,display.stderr
 leaf=directory/'certificate-0';data=leaf.read_bytes()
 subject=subprocess.run(['/usr/bin/openssl','x509','-inform','DER','-in',str(leaf),'-noout','-subject','-issuer','-serial','-dates'],capture_output=True,text=True,check=True,timeout=15)
 pub=subprocess.run(['/usr/bin/openssl','x509','-inform','DER','-in',str(leaf),'-pubkey','-noout'],capture_output=True,check=True,timeout=15)
 spki=subprocess.run(['/usr/bin/openssl','pkey','-pubin','-outform','DER'],input=pub.stdout,capture_output=True,check=True,timeout=15)
 (directory/'public-key-spki.der').write_bytes(spki.stdout)
 records[name]={'image':str(image),'public_leaf_der':str(leaf),'leaf_der_sha256':hashlib.sha256(data).hexdigest(),'leaf_der_sha1':hashlib.sha1(data).hexdigest(),'public_key_spki_sha256':hashlib.sha256(spki.stdout).hexdigest(),'public_key_spki_der':str(directory/'public-key-spki.der'),'certificate_details':subject.stdout,'signature_display':display.stdout+display.stderr,'strict_verification':{'exit_code':verify.returncode,'output':verify.stdout+verify.stderr}}
assert records['app']['leaf_der_sha256']==records['launcher']['leaf_der_sha256']
report={'schema':'airk.public-signing-certificate-identity/1','scope':'Only public certificates extracted from already signed images; no private keys, passwords, keychain changes, builds or signing operations.','team_id':'5PX7D99PXA','authority':'Developer ID Application: Ioka LLC (5PX7D99PXA)','same_leaf_for_app_and_copied_launcher':True,'records':records}
(out/'certificate-identity.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
