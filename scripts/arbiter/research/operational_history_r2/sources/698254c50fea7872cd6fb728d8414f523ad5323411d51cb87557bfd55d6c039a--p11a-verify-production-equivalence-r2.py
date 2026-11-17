# SPDX-License-Identifier: Apache-2.0
import hashlib, json, subprocess
from pathlib import Path
base=Path(__file__).resolve().parent
work=base/'p10-worktree'
snapshot=json.loads((base/'p11-native-20261106-r1-snapshot.json').read_text())
args=['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates',
    'scripts/build-service-runtime.py','scripts/provision-onnx-runtime.py','scripts/provision-local-model.py',
    'scripts/int8-policy.py','scripts/macho-identity.py','scripts/license-notices.py',
    'LICENSE','NOTICE','THIRD_PARTY_NOTICES.md','third_party']
names=subprocess.check_output(['git','-c','safe.directory='+work.as_posix(),'ls-files','--cached','--others','--exclude-standard','-z','--',*args],cwd=work).decode().strip('\0').split('\0')
differences=[name for name in sorted(set(names)) if hashlib.sha256((work/name).read_bytes()).hexdigest()!=snapshot['files'][name]]
expected=['crates/airk-cli/tests/combined_reads/profile_families.rs','crates/airk-mcp/src/response_identity_tests.rs']
assert differences==expected and len(set(names))==948
source=(work/'crates/airk-mcp/src/response.rs').read_text()
assert '#[cfg(test)]\n#[path = "response_identity_tests.rs"]\nmod identity_tests;' in source
output=base/'p11a-production-equivalence-r2.json'
assert not output.exists()
output.write_text(json.dumps({'source_r1':snapshot['native_source'],
    'source_r2':{'sha256':'d43f2cacac018fe2277f3ae8e0979a4af00528c4dee27071874a0298f1516ddf','files':948},
    'only_changed_native_inputs':differences,'unchanged_other_native_inputs':946,
    'production_module_bytes_unchanged':True,'mcp_external_test_cfg_verified':True,
    'limits':'New test code still needs execution; no relabeling of R1/Mac evidence as the R2 source. Fresh source-bound runtime receipt and actual binary comparison remain pending.'},indent=2)+'\n')
print('Only two external test inputs differ; 946 remaining native inputs identical to frozen R1')
