# SPDX-License-Identifier: Apache-2.0
import ast,hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parent
mapping=base/'p11a-final-qualification-command-map-r2.json'
plan=json.loads(mapping.read_text())
assert not (base/'p11a-final-qualification-r2.json').exists()
plan['qualification_change']='P11a R2: final external test corrections only; production and original matched R1 runtime remain unchanged. Preserve R1 failed fixture runs, R3/R4 successful complete native parity, exact source identities and all existing limits. No rebuilt or relabeled binary claim.'
plan['source_freeze']='Current test/source identity 97956846/948 is checked before/after every command. Compiled runtime source remains 33e6df13/948. Exactly two external test inputs differ; the other946 declared native inputs match the R1 snapshot. Keep these identities separate.'
plan['runtime_seal_gate'].update(call='Seal against compiled_runtime_source, compare exactly with original saved R1 seal, then compare before/after every stage; retain original runtime build metadata.',initial_source_expected='compiled_runtime_source from actual original successful R1 build receipt, never current test revision')
proof_path=Path(plan['production_equivalence_proof'])
proof=json.loads(proof_path.read_text())
proof['runtime_metadata_unchanged']=None
proof['requires_exact_original_seal_comparison']=True
proof_path.write_text(json.dumps(proof,indent=2)+'\n')
plan['production_equivalence_proof_sha256']=hashlib.sha256(proof_path.read_bytes()).hexdigest()
plan['adopted_checks']=[]
kept=[]
for row in plan['commands']:
 if row['name'].startswith('profile-parity-'):
  mode=row['name'].removeprefix('profile-parity-')
  receipt=base/f'p11a-flat-parity-preflight-{mode}-r4.receipt.json'
  plan['adopted_checks'].append({'name':row['name'],'receipt':str(receipt),'expected_command':row['command'][3:]})
 else:kept.append(row)
assert len(plan['adopted_checks'])==2
plan['commands']=kept
mapping.write_text(json.dumps(plan,indent=2)+'\n')
path=base/'p11a-final-queue-r2.py'
s=path.read_text()
anchor="    seal=sealer.seal_runtime(runtime,plan['compiled_runtime_source'])"
assert s.count(anchor)==1
replacement="""    adoption_path=Path(plan['production_equivalence_proof'])
    assert hashlib.sha256(adoption_path.read_bytes()).hexdigest()==plan['production_equivalence_proof_sha256']
    adoption=json.loads(adoption_path.read_text())
    assert adoption['current_test_source']==plan['source_expected'] and adoption['compiled_runtime_source']==plan['compiled_runtime_source']
    seal=sealer.seal_runtime(runtime,plan['compiled_runtime_source'])
    original_seal_path=base/'p11a-runtime-final-r1-seal.json'
    assert seal==json.loads(original_seal_path.read_text()), 'Adopted runtime differs from original saved R1 pair'
    record['original_runtime_seal_sha256']=hashlib.sha256(original_seal_path.read_bytes()).hexdigest()
    record['production_equivalence_proof_sha256']=plan['production_equivalence_proof_sha256']
    preflight=json.loads((base/'p11a-transport-preflight-r4.json').read_text())
    assert preflight['status']=='passed' and preflight['test_source']==plan['source_expected'] and preflight['runtime_source']==plan['compiled_runtime_source']['sha256']
    record['adopted_checks']=[]
    for adopted in plan['adopted_checks']:
        receipt_path=Path(adopted['receipt'])
        observed=json.loads(receipt_path.read_text())
        log_path=receipt_path.with_name(receipt_path.name.replace('.receipt.json','.log'))
        assert observed['command']==adopted['expected_command']
        assert observed['exit_code']==observed['actual_exit_code']==0 and observed['passing_tests']==1
        assert observed['source_before']==observed['source_after']==plan['source_expected'] and observed['source_unchanged']
        assert hashlib.sha256(log_path.read_bytes()).hexdigest()==observed['log_sha256']
        record['adopted_checks'].append({'name':adopted['name'],'receipt':str(receipt_path),'log_sha256':observed['log_sha256'],'passing_tests':1,'executed_in_this_queue':False})"""
s=s.replace(anchor,replacement)
ast.parse(s)
path.write_text(s,encoding='utf-8')
print('R2 reviewed: exact original runtime seal required;18 execution stages plus2 explicitly adopted passing R4 transport checks')
