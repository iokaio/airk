# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
soft=$(ulimit -Sn); hard=$(ulimit -Hn)
export AIRK_PREP_DESCRIPTOR_SOFT="$soft" AIRK_PREP_DESCRIPTOR_HARD="$hard"
/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13 -I -B - <<'REMOTE'
from pathlib import Path
import json,os,subprocess,hashlib,stat
home=Path.home();b=home/'airk-builds/arbiter-final-20261116-r1';s=b/'source';e=b/'evidence'
def sha(q):
 with q.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert os.getuid()==501==os.stat('/dev/console').st_uid
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=s,text=True).strip()=='dda749cbdf77b3e2106003ff10a683ec3aec3202'
assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=all'],cwd=s)
rust=home/'.rustup/toolchains/1.98.0-aarch64-apple-darwin/bin/rustc';assert rust.stat().st_uid==501
nextest=home/'.cargo/bin/cargo-nextest'
install=home/'.cargo/.crates2.json';installed=[]
if install.exists():
 d=json.loads(install.read_text())
 installed=[{'package':k.split()[0],'version':k.split()[1]} for k in d.get('installs',{}) if k.startswith('cargo-nextest ')]
rows=[]
for path in [s,b,b/'inputs',e,rust,nextest]:
 if path.exists():
  m=path.stat();rows.append({'path':str(path),'uid':m.st_uid,'mode':oct(stat.S_IMODE(m.st_mode)),'is_file':path.is_file(),'sha256':sha(path) if path.is_file() else None})
active=[];images={'airk','airkctl','airk-embed','airk-launcher','airk-launcher-image','airk-service','airk-index','airk-source'}
for line in subprocess.check_output(['/bin/ps','-ww','-axo','pid=,ppid=,uid=,comm='],text=True).splitlines():
 v=line.strip().split(None,3)
 if len(v)==4 and v[2]=='501' and Path(v[3]).name.lower() in images:active.append({'pid':int(v[0]),'parent':int(v[1]),'uid':501,'image':v[3]})
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'hostname':subprocess.check_output(['hostname'],text=True).strip(),'os':subprocess.check_output(['/usr/bin/sw_vers'],text=True),'hardware':{k:subprocess.check_output(['/usr/sbin/sysctl','-n',k],text=True).strip() for k in ('hw.memsize','hw.logicalcpu','hw.physicalcpu')},'descriptor_soft':os.environ['AIRK_PREP_DESCRIPTOR_SOFT'],'descriptor_hard':os.environ['AIRK_PREP_DESCRIPTOR_HARD'],'disk':subprocess.check_output(['df','-h',str(home)],text=True),'rustc':subprocess.check_output([str(rust),'--version','--verbose'],text=True),'nextest_binary_present':nextest.is_file(),'nextest_install_metadata':installed,'tool_and_output_owners':rows,'source_commit':'dda749cbdf77b3e2106003ff10a683ec3aec3202','source_clean':True,'target_absent':not (s/'target').exists(),'future_batch_root_absent':not (e/'platform-integrated-dda749cb-r1').exists(),'active_AIRK_images_basename_only':active,'static_environment_sha256':sha(e/'static-environment-r1.json'),'source_bridge_proof_sha256':sha(e/'source-bridge-dda749cb-r1.json')},indent=2))
REMOTE
