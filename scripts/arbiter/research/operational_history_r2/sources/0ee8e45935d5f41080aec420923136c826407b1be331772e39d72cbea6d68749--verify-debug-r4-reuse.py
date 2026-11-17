# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json
from pathlib import Path
root=Path('D:/code/Github/airk')
base=root/'artifacts/local/one-airk-1.8.14'
def module(name, path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
builder=module('reuse_builder',root/'scripts/build-service-runtime.py')
package=module('reuse_package',root/'scripts/package-native.py')
runtime=base/'windows-debug-r4/runtime'
receipt=json.loads((runtime/'airk-build.json').read_text())
changed=root/'crates/airk-cli/tests/combined_reads.rs'
before=base/'combined_reads-before-benchmark-enrollment.rs'
old=builder.digest(before)
original_digest=builder.digest
current=builder.source_identity()
builder.digest=lambda path:old if path==changed else original_digest(path)
reconstructed=builder.source_identity()
builder.digest=original_digest
if reconstructed!=receipt['source']:
    raise RuntimeError('Original compiled input identity was not reconstructed exactly')
files,_=package.validate_runtime(runtime,'1.8.14','x86_64-pc-windows-msvc',receipt['source'])
report={'schema':1,'runtime':str(runtime),'reused_not_fresh_build':True,
'original_compiled_source':receipt['source'],'current_test_source_identity':current,
'reconstructed_original_identity':reconstructed,'all_other_production_input_bytes_unchanged':True,
'changed_external_test':{'path':str(changed.relative_to(root)),'before_sha256':old,'after_sha256':original_digest(changed)},
'complete_artifact_seals_verified':True,'payload_files':len(files),
'install_manifest_sha256':files['airk-install.json'],'executable_sha256':files['airk.exe']}
(base/'windows-debug-r4-reuse-proof.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
