# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re,struct
root=Path.cwd();source=root/'artifacts/advanced-split/msvc-configure-owner-context-r8';out=root/'artifacts/advanced-split/msvc-configure-owner-context-r9'
original=(source/'receipt.json').read_bytes();receipt=json.loads(original);observed={}
assert receipt['result']['exit_code']==0 and receipt['result']['owned_descendants_settled'] and not receipt['result']['owned_job_forced_termination']
for mode,expected in [('empty',''),('embedded','Embedded')]:
 build=source/'coff-witness'/mode;row=receipt['coff_comparison'][mode]
 for command in row['commands']:
  r=command['result'];assert r['exit_code']==0 and r['owned_descendants_settled'] and not r['owned_job_forced_termination']
 obj=next(build.rglob('*.obj'));raw=obj.read_bytes();assert hashlib.sha256(raw).hexdigest()==row['object_sha256'] and len(raw)==row['object_bytes']
 flags=(build/'CMakeFiles/coff_witness.dir/flags.make').read_text();actual=next(line.split('=',1)[1].strip() for line in flags.splitlines() if line.startswith('CXX_FLAGS ='))
 debug=[flag for flag in actual.split() if flag in ['/Z7','-Z7','/Zi','-Zi','/ZI','-ZI']]
 assert debug==([] if mode=='empty' else ['-Z7']),(mode,debug)
 assert '/DEBUG' not in actual.upper() and '/O2' in actual and '/DNDEBUG' in actual
 cache=(build/'CMakeCache.txt').read_text();values={line.split(':',1)[0]:line.split('=',1)[1] for line in cache.splitlines() if ':' in line and '=' in line and not line.startswith('//')}
 assert values['CMAKE_MSVC_DEBUG_INFORMATION_FORMAT']==expected
 assert values['CMAKE_BUILD_TYPE']=='Release'
 count=struct.unpack_from('<H',raw,2)[0];sections=[raw[20+i*40:28+i*40].rstrip(b'\0').decode('ascii') for i in range(count)]
 assert ('.debug$T' in sections)==(mode=='embedded')
 observed[mode]={'object_sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'actual_flags':actual,'sections':sections,'flags_sha256':hashlib.sha256((build/'CMakeFiles/coff_witness.dir/flags.make').read_bytes()).hexdigest(),'cache_sha256':hashlib.sha256((build/'CMakeCache.txt').read_bytes()).hexdigest()}
assert observed['empty']['bytes']<observed['embedded']['bytes']
assert (source/'coff-witness/witness.cc').read_bytes()==b'int airk_release_coff_witness(int x) { return x + 7; }\r\n'
assert (source/'receipt.json').read_bytes()==original
result={'read_only_artifact_inspection':True,'r8_wrapper_final_assertion':'failed literal console-Z7 check; NMake uses response files; original evidence unchanged','original_receipt_sha256':hashlib.sha256(original).hexdigest(),'source_sha256':hashlib.sha256((source/'coff-witness/witness.cc').read_bytes()).hexdigest(),'observed':observed,'passed':True}
(out/'receipt.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
