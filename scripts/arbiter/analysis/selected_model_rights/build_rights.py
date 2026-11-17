# SPDX-License-Identifier: Apache-2.0
"""Private evidence assembler. Copies actual native JSON objects; never constructs a Profile."""
import hashlib,json,shutil
from pathlib import Path
from native_json_projection import array_member
ROOT=Path.cwd();OUT=Path(__file__).resolve().parent
SHA=lambda raw:hashlib.sha256(raw).hexdigest()
def read_small(path):
 path=Path(path);assert 0<path.stat().st_size<=32*1024*1024,('not a small evidence file',path)
 return path.read_bytes()
def pin(path):
 path=Path(path).resolve();raw=read_small(path);return {'path':str(path),'bytes':len(raw),'sha256':SHA(raw)}
def write_new(path,raw):
 path.parent.mkdir(parents=True,exist_ok=True)
 with path.open('xb')as stream:stream.write(raw)
def encode(value):return (json.dumps(value,ensure_ascii=True,indent=2)+'\n').encode('utf-8')
def evidence(path,directory):
 path=Path(path);raw=read_small(path);name=path.name
 dst=directory/(SHA(raw)[:12]+'--'+name)
 if dst.exists():assert dst.read_bytes()==raw
 else:write_new(dst,raw)
 return pin(dst)
def main():
 source=json.loads((OUT/'source-notice-manifest.json').read_bytes());inputs=json.loads((OUT/'rights-inputs.json').read_bytes());outputs=[]
 for item in inputs['models']:
  master=Path(item['master']);assert SHA(read_small(master))==item['master_sha256'];entry=json.loads(master.read_bytes())['profiles'][0]
  raw=read_small(entry['profile']);assert SHA(raw)==entry['profile_sha256'];profile=json.loads(raw);assert profile['model']==item['model'] and profile['manifest']==entry['manifest']
  preimage=array_member(raw,'assets');assert json.loads(preimage)==profile['assets'];assert set(entry['files'])=={a['name']for a in profile['assets']}
  directory=OUT/profile['model'];directory.mkdir(exist_ok=False);ev=directory/'evidence';ev.mkdir()
  preimage_path=directory/'assets.serde-compact.native-preimage.json';write_new(preimage_path,preimage)
  notices={};basis={}
  def add(p):
   value=pin(p);notices.setdefault(value['sha256'],value);return value['sha256']
  upstream=next(row for row in source['upstream']if row['repository']==item['upstream'])
  assert upstream['publisher_license_declaration']=='apache-2.0' and upstream['not_locally_acquired']==[]
  for row in upstream['files']:basis[row['name']]=add(row['pin']['path'])
  basis['publisher_metadata']=add(upstream['metadata_pin']['path'])
  if item['upstream']=='Qwen/Qwen3-Reranker-0.6B':
   assert upstream['declared_files']==['README.md']
   base=next(row for row in source['upstream']if row['repository']=='Qwen/Qwen3-0.6B-Base')
   for row in base['files']:basis['base_'+row['name']]=add(row['pin']['path'])
   add(base['metadata_pin']['path']);basis['LICENSE']=basis['base_LICENSE']
  basis['AIRK_LICENSE']=add(source['airk_repository_license']['path']);basis['source_manifest']=add(OUT/'source-notice-manifest.json')
  producer_pins=[]
  for p in inputs['common_source']+item['producer_files']:
   p=Path(p);ep=evidence(p,ev);notices.setdefault(ep['sha256'],ep);producer_pins.append({'original':str(p.resolve()),'retained':ep})
  small_assets=[]
  for asset in profile['assets']:
   if asset['role']in ['compiler','calibration','fixture','notice']:
    original=Path(entry['files'][asset['name']]);raw_asset=read_small(original);assert len(raw_asset)==asset['bytes']and SHA(raw_asset)==asset['sha256'];small_assets.append({'asset':asset,'original':str(original),'verified_bytes':True})
  provenance={'schema':'airk.private-selected-generated-asset-provenance/1','model':profile['model'],'native_profile':{'path':entry['profile'],'sha256':entry['profile_sha256'],'manifest':entry['manifest']},'native_master':pin(master),'upstream_repository':upstream['repository'],'upstream_revision':upstream['revision'],'producer_receipts_and_source':producer_pins,'small_metadata_assets_verified':small_assets,'large_assets_not_rehashed_or_copied':True,'actual_asset_preimage':pin(preimage_path),'preimage_method':'Exact raw assets array from the pinned Rust-written Profile, with JSON whitespace removed only outside strings. No sorting, field rewriting, string escape normalization, integer rewriting or Python Profile construction. Actual Rust final validator must independently recompute typed Profile.assets SHA before Root approval.','generated_assets_license_basis':'AIRK Apache-2.0 repository LICENSE and actual producer/source SPDX headers; publisher diagnostic prompts/tokenizer remain separately attributed under their own full upstream README/license declarations. Current source snapshots are retained for audit; original native emitter/storage/numeric receipts identify actual production.','fixture_scope':item['fixture_scope'],'raw_calibration_scope':item['calibration_scope'],'private_corpus_or_human_gold_included':False,'final_fitted_calibration':'Not included or implied. A future actual fitted Core Asset requires a separate additive rights roster and Root approval.','qualification_or_approval_granted':False}
  prov=directory/'generated-asset-provenance.json';write_new(prov,encode(provenance));basis['generated_provenance']=add(prov)
  upstream_basis=[basis['README.md'],basis['LICENSE'],basis['publisher_metadata'],basis['source_manifest']]
  own_basis=[basis['AIRK_LICENSE'],basis['generated_provenance']]+[value['retained']['sha256']for value in producer_pins]
  rows=[]
  for asset in profile['assets']:
   role=asset['role'];actual=str(Path(entry['files'][asset['name']]).resolve())
   if role in ['model','external_weights']:
    why=f"Derived ONNX graph/external tensor bytes from {upstream['repository']}@{upstream['revision']}; exact native storage/export receipts and declared asset identity retained. No library executable or dataset corpus is an asset. Original path: {actual}."
    licenses=upstream_basis+[basis['generated_provenance']]
   elif role=='tokenizer':
    selected=next(file for file in json.loads(Path(upstream['metadata_pin']['path']).read_bytes())['siblings']if file['rfilename']=='tokenizer.json');assert selected['lfs']['sha256']==asset['sha256']and selected['size']==asset['bytes']
    why=f"Unmodified publisher tokenizer from {upstream['repository']}@{upstream['revision']}; exact LFS SHA/length agrees with native Core Asset. Actual bytes were authenticated by the native profile producer; not reread during the exclusive NN window. Original path: {actual}.";licenses=upstream_basis
   elif role=='notice' and asset['name']=='LICENSE':
    why=("Complete LICENSE bytes from separately pinned Qwen/Qwen3-0.6B-Base repository, NOT a LICENSE from the reranker repository. The reranker own full README/card declares apache-2.0; repository absence of LICENSE is explicit. "if item['upstream']=='Qwen/Qwen3-Reranker-0.6B'else f"Complete same-repository publisher LICENSE from {upstream['repository']}@{upstream['revision']}. ")+f"Original path: {actual}.";licenses=upstream_basis
   elif role=='notice'and asset['name']=='README.md':why=f"Complete unmodified reranker publisher README, including its own Apache-2.0 declaration and base-model attribution; pinned Git blob and full byte hash retained. Original path: {actual}.";licenses=upstream_basis
   else:
    why=f"AIRK-authored {role} metadata, generated by the retained actual native factory/source or explicitly synthetic/publisher numerical producer receipts. {item['fixture_scope'] if role=='fixture' else item['calibration_scope'] if role=='calibration' else 'No dataset corpus or human semantic gold is embedded.'} Original path: {actual}.";licenses=own_basis+upstream_basis
   if item['upstream']=='Qwen/Qwen3-Reranker-0.6B'and role in ['model','external_weights','tokenizer']:why+=' SPDX expression follows the reranker publisher model-card declaration; the distinct base-model LICENSE is corroborating base provenance and is not relabelled as a reranker same-repository LICENSE.'
   assert len(why.encode('utf-8'))<=4096
   rows.append({'asset':asset,'provenance':why,'license_spdx':'Apache-2.0','license_basis':list(dict.fromkeys(licenses))})
  roster={'schema':'airk.private-reviewed-model-rights/1','model':profile['model'],'assets_sha256':SHA(preimage),'assets':rows,'license_and_notice_files':list(notices.values())}
  assert len(rows)==len(profile['assets'])and{a['asset']['name']for a in rows}==set(entry['files'])
  for row in rows:assert set(row['license_basis'])<=set(notices)
  dest=directory/'rights-roster.raw.json';write_new(dest,encode(roster));outputs.append({'model':profile['model'],'profile_manifest':profile['manifest'],'profile_sha256':entry['profile_sha256'],'roster':pin(dest),'assets_preimage':pin(preimage_path),'generated_provenance':pin(prov),'native_typed_digest_validation':'REQUIRED BEFORE ROOT APPROVAL','approval':'NOT APPROVED'})
 report={'schema':'airk.private-selected-model-rights-preparation/1','status':'COMPLETE_RAW_RIGHTS_ARTIFACTS_PENDING_ROOT_REVIEW_AND_NATIVE_DIGEST_GATE','profiles':outputs,'exact_total_assets':sum(len(json.loads(Path(row['roster']['path']).read_bytes())['assets'])for row in outputs),'upstream_notices':pin(OUT/'source-notice-manifest.json'),'builder':pin(Path(__file__)),'mechanical_projection':pin(OUT/'native_json_projection.py'),'no_cargo_or_NN':True,'no_full_model_or_tokenizer_read_or_copy':True,'private_corpus_or_gold_payload':False,'final_fitted_calibration':'Future additive actual asset/rights roster only; not approved by this raw record.'}
 write_new(OUT/'rights-preparation-manifest.json',encode(report));print(json.dumps(report,ensure_ascii=True))
if __name__=='__main__':main()
