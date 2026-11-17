# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,re,hashlib,time
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
A=B/'apt';env={**os.environ,'APT_CONFIG':str(A/'apt.conf'),'TMPDIR':str(B/'tmp')}
def run(args):
 p=subprocess.run(args,env=env,cwd=A,capture_output=True,text=True,timeout=45)
 if p.returncode:raise RuntimeError(json.dumps({'argv':args,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}))
 return p.stdout
G=A/'gpgv-home';G.mkdir(mode=0o700)
releases=[]
for p in sorted((A/'lists').glob('*_InRelease')):
 output=run(['/usr/bin/gpgv','--homedir',str(G),'--status-fd','1','--keyring','/usr/share/keyrings/ubuntu-archive-keyring.gpg',str(p)])
 assert '[GNUPG:] VALIDSIG ' in output
 releases.append({'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'signature_status':output})
assert len(releases)==3
roots=['gcc-13','g++-13','libc6-dev','linux-libc-dev','make','pkgconf','libc6','libgcc-s1','libstdc++6','libgmp10','libmpfr6','libzstd1','zlib1g']
simulation=run(['/usr/bin/apt-get','-s','--no-install-recommends','install',*roots])
packages=sorted(set(roots)|set(re.findall(r'^Inst (\S+)',simulation,re.M)))
rows=[]
for name in packages:
 policy=run(['/usr/bin/apt-cache','policy',name]);version=re.search(r'^  Candidate: (\S+)',policy,re.M).group(1);assert version!='(none)'
 text=run(['/usr/bin/apt-cache','show',name+'='+version]);paragraphs=text.strip().split('\n\n')
 found=[]
 for paragraph in paragraphs:
  fields={};key=None
  for line in paragraph.splitlines():
   if line.startswith((' ','\t')):fields[key]+='\n'+line
   elif ': ' in line:key,value=line.split(': ',1);fields[key]=value
  if fields.get('Package')==name.split(':')[0] and fields.get('Version')==version and 'Filename' in fields:found.append(fields)
 assert found,name
 fields=found[0];assert all(all(x[k]==fields[k] for k in ['Filename','Size','SHA256']) for x in found)
 assert fields['Filename'].startswith('pool/') and re.fullmatch('[0-9a-f]{64}',fields['SHA256'])
 rows.append({k:fields[k] for k in ['Package','Version','Architecture','Filename','Size','SHA256']}|{'Depends':fields.get('Depends'),'Pre-Depends':fields.get('Pre-Depends')})
record={'schema':'airk.private.signed-linux-package-roster/1','uid':os.getuid(),'root':str(B),'roots':roots,'releases':releases,'keyring':{'path':'/usr/share/keyrings/ubuntu-archive-keyring.gpg','sha256':hashlib.sha256(pathlib.Path('/usr/share/keyrings/ubuntu-archive-keyring.gpg').read_bytes()).hexdigest()},'simulation':simulation,'packages':rows,'total_package_bytes':sum(int(r['Size']) for r in rows),'packages_downloaded':False,'no_libssl_or_libclang_added':True}
path=B/'evidence/signed-package-roster-r1.json';path.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record));print('ROSTER_SHA256',hashlib.sha256(path.read_bytes()).hexdigest())
