# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,types
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/receipts/mcp-corrections-r21';out.mkdir(exist_ok=False)
helper=root/'scripts/provision-static-onnx-runtime.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='bdbdf773716af7184da59ee17b9ec718f657874cfe15efea8462fbbae27fc59a'
owner=types.ModuleType('retained_mcp_correction_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
binary=root/'artifacts/v190/p10-target/debug/deps/airk_mcp-6566b8fea9bb6a35.exe'
with binary.open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()=='b449b9aa74ec25fa6a683552e3a2423948e0a8d7a4086784bf43f52fabddec01'
selectors=['local::tests::evidence_tools_advertise_response_controls_and_discovery_keeps_its_closed_metadata_contract','unified::tests::one_catalog_has_unique_unprefixed_names_and_honest_effect_annotations','verification_tests::only_evidence_and_parse_catalogs_offer_explicit_verification_controls','arbiter_handler_tests::canonical_answer_context_handles_one_line_eof_and_late_verified_hits','verification_tests::answer_verification_controls_reach_the_saved_source_owner_through_actual_sdk']
rows=[]
for offset,selector in enumerate(selectors):
 log=out/(str(offset)+'.log');result=owner.run_owned([str(binary),'--exact',selector,'--nocapture','--test-threads=1'],root,log,60);rows.append({'selector':selector,'owned':result});(out/'execution.json').write_text(json.dumps({'binary_sha256':'b449b9aa74ec25fa6a683552e3a2423948e0a8d7a4086784bf43f52fabddec01','rows':rows},indent=2)+'\n')
 assert owner.successful(result) and result['owned_job_forced_termination'] is False,result
 assert '1 passed; 0 failed' in log.read_text(), 'selector must run exactly one test'
 print(json.dumps({'selector':selector,'passed':1,'elapsed_seconds':result['elapsed_seconds']}),flush=True)
