# SPDX-License-Identifier: Apache-2.0
"""Keep compiled provenance intact while qualifying final external test corrections."""
import ast, hashlib, importlib.util, json, subprocess
from pathlib import Path
base=Path(__file__).resolve().parent
work=base/'p10-worktree'
expected={'sha256':'979568465e27b37bc82538ca91e6f5ef843f20886270679ba7da1f42e40e53fc','files':948}
compiled={'sha256':'33e6df135da90839f409dfbba6dd2ce338af31675875e99f20d0b94af212f3f7','files':948}
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
assert builder.source_identity()==expected
snapshot=json.loads((base/'p11-native-20261106-r1-snapshot.json').read_text())
assert snapshot['native_source']==compiled
exact={'Cargo.toml','Cargo.lock','rust-toolchain.toml','scripts/build-service-runtime.py',
 'scripts/provision-onnx-runtime.py','scripts/provision-local-model.py','scripts/int8-policy.py',
 'scripts/macho-identity.py','scripts/license-notices.py','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md'}
def selected(p):return p in exact or p.startswith(('.cargo/','crates/','third_party/'))
old={p:h for p,h in snapshot['files'].items() if selected(p)}
names=subprocess.check_output(['git','-c','safe.directory='+work.as_posix(),'ls-files','--cached','--others','--exclude-standard','-z'],cwd=work).decode().strip('\0').split('\0')
current={p:hashlib.sha256((work/p).read_bytes()).hexdigest() for p in names if selected(p)}
assert set(current)==set(old) and len(current)==948
changed=sorted(p for p in current if current[p]!=old[p])
assert changed==['crates/airk-cli/tests/combined_reads/profile_families.rs','crates/airk-mcp/src/response_identity_tests.rs']
assert '#[cfg(test)]\n#[path = "response_identity_tests.rs"]\nmod identity_tests;' in (work/'crates/airk-mcp/src/response.rs').read_text()
proof={'compiled_runtime_source':compiled,'current_test_source':expected,'unchanged_other_native_inputs':946,
 'only_changed_inputs':changed,'runtime_metadata_unchanged':True,
 'scope':'Reuse the exact R1 matched pair with original build receipt. Test-only source revisions are separate; no rebuilt or relabeled binary claim.'}
proof_path=base/'p11a-runtime-adoption-r2.json';assert not proof_path.exists()
proof_path.write_text(json.dumps(proof,indent=2)+'\n')
plan=json.loads((base/'p11a-final-qualification-command-map-r1.json').read_text())
def rename(v):
 if isinstance(v,str):return v.replace('p11a-final-', 'p11a-final-').replace('-r1.log','-r2.log').replace('-r1.json','-r2.json')
 if isinstance(v,list):return [rename(x) for x in v]
 if isinstance(v,dict):return {k:rename(x) for k,x in v.items()}
 return v
# Only command outputs/expected reports receive new labels; keep inputs and runtime untouched.
for row in plan['commands']:
 row['log']=row['log'].replace('-r1.log','-r2.log')
 row['command']=[a.replace('p11a-final-', 'p11a-final-').replace('-r1.log','-r2.log').replace('-r1.json','-r2.json') if isinstance(a,str) and 'p11a-final-' in a else a for a in row['command']]
 for i,a in enumerate(row['command']):
  if isinstance(a,str) and 'p11a-final-' in a and '-r1' in a:row['command'][i]=a.replace('-r1','-r2')
 if 'expected_report' in row:row['expected_report']=rename(row['expected_report'])
plan['source_expected']=expected
plan['compiled_runtime_source']=compiled
plan['production_equivalence_proof']=str(proof_path)
mapping=base/'p11a-final-qualification-command-map-r2.json';assert not mapping.exists()
mapping.write_text(json.dumps(plan,indent=2)+'\n')
runner=(base/'p11a-final-queue-r1.py').read_text()
runner=runner.replace('p11a-final-qualification-command-map-r1.json',mapping.name).replace('p11a-final-qualification-r1.json','p11a-final-qualification-r2.json').replace('p11a-final-qualification-owner.lock','p11a-final-qualification-owner-r2.lock')
runner=runner.replace("    assert release['source_before']==release['source_after']==plan['source_expected'] and release['source_unchanged']", "    assert release['source_before']==release['source_after']==plan['compiled_runtime_source'] and release['source_unchanged']")
runner=runner.replace("    record['source']=plan['source_expected']", "    record['source']=plan['source_expected']\n    record['compiled_runtime_source']=plan['compiled_runtime_source']\n    record['runtime_adoption_proof']=plan['production_equivalence_proof']")
runner=runner.replace("sealer.seal_runtime(runtime,record['source'])","sealer.seal_runtime(runtime,plan['compiled_runtime_source'])").replace("sealer.seal_runtime(runtime, record['source'])","sealer.seal_runtime(runtime, plan['compiled_runtime_source'])")
runner=runner.replace("with (base/'p11a-runtime-final-r1-seal.json').open('x',encoding='utf-8') as handle:","with (base/'p11a-runtime-adoption-r2-seal.json').open('x',encoding='utf-8') as handle:")
ast.parse(runner)
output=base/'p11a-final-queue-r2.py';assert not output.exists()
output.write_text(runner,encoding='utf-8')
print(json.dumps({'prepared_only':True,'test_source':expected,'compiled_runtime_source':compiled,'unchanged_production_inputs':946,'stages':len(plan['commands'])}))
