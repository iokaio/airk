# SPDX-License-Identifier: Apache-2.0
"""Verify private evidence; optional actual Rust facts close the native digest gate."""
import argparse,hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
sha=lambda b:hashlib.sha256(b).hexdigest()
def checked(pin):
 p=Path(pin['path']);b=p.read_bytes();assert len(b)==pin['bytes']and sha(b)==pin['sha256'],p;return b
p=argparse.ArgumentParser();p.add_argument('--native-facts',type=Path);p.add_argument('--native-facts-sha256');p.add_argument('--out',type=Path,required=True);a=p.parse_args()
manifest_raw=(ROOT/'rights-preparation-manifest.json').read_bytes();manifest=json.loads(manifest_raw);expected={}
for row in manifest['profiles']:
 roster=json.loads(checked(row['roster']));assert roster['schema']=='airk.private-reviewed-model-rights/1'and roster['model']==row['model'];preimage=checked(row['assets_preimage']);assert sha(preimage)==roster['assets_sha256'];assets=json.loads(preimage);assert[row['asset']for row in roster['assets']]==assets
 notices={}
 for notice in roster['license_and_notice_files']:checked(notice);assert notice['sha256']not in notices;notices[notice['sha256']]=notice
 assert len(assets)==len(roster['assets'])and len({v['asset']['name']for v in roster['assets']})==len(assets)
 for asset in roster['assets']:assert asset['license_spdx']=='Apache-2.0'and asset['provenance'].strip()and set(asset['license_basis'])<=notices.keys()
 expected[row['model']]=(row,roster)
assert len(expected)==3
native=None
if a.native_facts:
 assert a.native_facts_sha256,'exact actual native facts SHA required';raw=a.native_facts.read_bytes();assert sha(raw)==a.native_facts_sha256;native=json.loads(raw);assert native['schema']=='airk.private-final-distribution-candidate-facts/1'and native['input_sha256']==sha((ROOT/'native-facts-input.json').read_bytes())and native['qualification_granted']is False
 assert len(native['profiles'])==3and len({r['model']for r in native['profiles']})==3
 for fact in native['profiles']:
  row,roster=expected[fact['model']];assert fact['declared_only']is True and fact['qualification_granted']is False;assert fact['manifest']==row['profile_manifest']and fact['profile_sha256']==row['profile_sha256'];assert fact['assets_sha256']==roster['assets_sha256']and fact['assets']==[r['asset']for r in roster['assets']]
else:assert not a.native_facts_sha256
result={'schema':'airk.private-selected-rights-verification/1','status':'NATIVE_TYPED_ASSET_DIGEST_MATCH_ROOT_APPROVAL_STILL_REQUIRED'if native else'PREPARATION_INTEGRITY_PASS_NATIVE_DIGEST_AND_ROOT_APPROVAL_REQUIRED','rights_manifest_sha256':sha(manifest_raw),'native_facts':{'path':str(a.native_facts.resolve()),'sha256':a.native_facts_sha256}if native else None,'profiles':3,'assets':sum(len(v[1]['assets'])for v in expected.values()),'rights_rosters':[{k:v[0]['roster']['sha256']}for k,v in expected.items()],'Root_approved':False,'model_assets_opened':False,'task_or_release_qualification_granted':False}
assert a.out.resolve().is_relative_to(ROOT)
with a.out.open('x',encoding='utf-8',newline='\n')as stream:json.dump(result,stream,indent=2);stream.write('\n')
print(json.dumps(result))
