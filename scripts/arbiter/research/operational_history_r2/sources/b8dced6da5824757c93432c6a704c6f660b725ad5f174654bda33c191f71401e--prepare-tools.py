# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,tarfile,urllib.request
from pathlib import Path
run=Path(__file__).resolve().parent;root=run/'prepared-tools';root.mkdir(exist_ok=False);(root/'downloads').mkdir();(root/'bin').mkdir()
assets=[
 ('cargo-audit','0.22.2','https://github.com/rustsec/rustsec/releases/download/cargo-audit/v0.22.2/cargo-audit-aarch64-apple-darwin-v0.22.2.tgz','ec7ca4263769593df4d909be85b94a6b79efa2897be5d2bb8ebd516e823175af'),
 ('cargo-deny','0.20.2','https://github.com/EmbarkStudios/cargo-deny/releases/download/0.20.2/cargo-deny-0.20.2-aarch64-apple-darwin.tar.gz','fe67d82a10d8597a3549364cb733a3f9cc1bfff9031b7ae46384a9f2a72090c3'),
 ('cargo-vet','0.10.3','https://github.com/mozilla/cargo-vet/releases/download/v0.10.3/cargo-vet-aarch64-apple-darwin.tar.xz','a83f7d6f48a069890743ba61f7637660c8577f263efd64fee47e673632f0e423'),
 ('pwsh','7.6.6','https://github.com/PowerShell/PowerShell/releases/download/v7.6.6/powershell-7.6.6-osx-arm64.tar.gz','6df833d094ebac1c1a74340d7b3437f4aaf5e03ce640484a1c4359f3ce8b3db1')]
records=[]
for name,version,url,expected in assets:
 archive=root/'downloads'/url.rsplit('/',1)[1];request=urllib.request.Request(url,headers={'User-Agent':'AIRK-owned-qualification'})
 print('Preparing '+name+' '+version,flush=True)
 with urllib.request.urlopen(request,timeout=120) as response,archive.open('xb') as output:shutil.copyfileobj(response,output)
 actual=hashlib.sha256(archive.read_bytes()).hexdigest();assert actual==expected,(name,actual,expected)
 destination=root/name;destination.mkdir()
 with tarfile.open(archive) as packed:packed.extractall(destination,filter='data')
 binaries=[file for file in destination.rglob(name) if file.is_file() and not file.is_symlink()];assert len(binaries)==1,(name,binaries)
 binary=binaries[0];binary.chmod(0o700)
 if name!='pwsh':shutil.copy2(binary,root/'bin'/name)
 records.append({'name':name,'version':version,'url':url,'sha256':actual,'binary':str(binary)})
 (root/'provenance.json').write_text(json.dumps(records,indent=2))
environment=dict(os.environ,PATH=str(root/'bin')+':'+str(root/'pwsh')+':/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools:'+str(Path.home()/'.cargo/bin')+':'+os.environ['PATH'],POWERSHELL_TELEMETRY_OPTOUT='1',DOTNET_CLI_TELEMETRY_OPTOUT='1')
for record in records:
 name,version=record['name'],record['version'];command=[record['binary'],'-NoProfile','-Command','$PSVersionTable.PSVersion.ToString()'] if name=='pwsh' else ['cargo',name.removeprefix('cargo-'),'--version']
 result=subprocess.run(command,capture_output=True,text=True,env=environment,timeout=60);assert result.returncode==0,(name,result.stderr);assert version in result.stdout,(name,result.stdout)
 record['observed_version']=result.stdout.strip();print(record['observed_version'],flush=True)
(root/'provenance.json').write_text(json.dumps(records,indent=2));(root/'complete.json').write_text(json.dumps({'passed':True,'tools':len(records)}));print('Owned qualification tools verified',flush=True)
