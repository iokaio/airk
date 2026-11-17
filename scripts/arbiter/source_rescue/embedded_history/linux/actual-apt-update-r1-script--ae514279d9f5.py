# SPDX-License-Identifier: Apache-2.0
import os,pwd,stat,pathlib,json,subprocess,time,hashlib
assert os.getuid()==os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler'
os.umask(0o077)
H=pathlib.Path('/home/tyler');assert H.resolve(strict=True)==H and H.stat().st_uid==1000
P=H/'airk-builds'
if not P.exists():P.mkdir(mode=0o700)
assert P.resolve(strict=True)==P and P.stat().st_uid==1000 and stat.S_IMODE(P.stat().st_mode)&0o077==0
B=P/'arbiter-final-linux-r1';B.mkdir(mode=0o700)
for p in ['source-transfer','inputs','downloads','toolchain/sysroot','toolchain/bin','apt/lists/partial','apt/cache/archives/partial','apt/downloads','apt/log','apt/empty-conf','cargo','rustup','node','nextest','tmp','private-census','evidence']:(B/p).mkdir(mode=0o700,parents=True,exist_ok=False)
apt=B/'apt';(apt/'empty-main').write_text('')
cfg=f'''Dir::Etc::parts "{apt}/empty-conf";
Dir::Etc::main "{apt}/empty-main";
Dir::Etc::sourcelist "{apt}/ubuntu.sources";
Dir::Etc::sourceparts "-";
Dir::State::lists "{apt}/lists";
Dir::State::status "/var/lib/dpkg/status";
Dir::Cache "{apt}/cache";
Dir::Cache::archives "{apt}/cache/archives";
Dir::Cache::pkgcache "";
Dir::Cache::srcpkgcache "";
Dir::Log "{apt}/log";
APT::Sandbox::User "tyler";
APT::Architecture "amd64";
Acquire::Languages "none";
Acquire::AllowInsecureRepositories "false";
APT::Get::AllowUnauthenticated "false";
Acquire::Retries "0";
'''
(apt/'apt.conf').write_text(cfg)
(apt/'ubuntu.sources').write_text('Types: deb\nURIs: https://archive.ubuntu.com/ubuntu/\nSuites: noble noble-updates\nComponents: main\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n\nTypes: deb\nURIs: https://security.ubuntu.com/ubuntu/\nSuites: noble-security\nComponents: main\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n')
env={**os.environ,'APT_CONFIG':str(apt/'apt.conf'),'TMPDIR':str(B/'tmp'),'PYTHONDONTWRITEBYTECODE':'1'}
started=time.monotonic();p=subprocess.run(['/usr/bin/apt-get','update'],env=env,cwd=apt,capture_output=True,text=True,timeout=180)
r={'stage':'signed-owned-apt-update','uid':os.getuid(),'root':str(B),'argv':['/usr/bin/apt-get','update'],'exit_code':p.returncode,'seconds':time.monotonic()-started,'stdout':p.stdout,'stderr':p.stderr,'apt_config_sha256':hashlib.sha256(cfg.encode()).hexdigest(),'new_root_owner':B.stat().st_uid,'new_root_mode':oct(stat.S_IMODE(B.stat().st_mode)),'no_apt_install':True,'no_global_config':True}
(B/'evidence/apt-update-r1.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));raise SystemExit(p.returncode)
