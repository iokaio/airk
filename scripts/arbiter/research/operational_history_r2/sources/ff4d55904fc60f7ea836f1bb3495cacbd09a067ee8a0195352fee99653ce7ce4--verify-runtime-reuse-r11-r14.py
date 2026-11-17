# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, sys
from pathlib import Path
sys.dont_write_bytecode=True
spec=importlib.util.spec_from_file_location("reuse_package", "scripts/package-native.py")
package=importlib.util.module_from_spec(spec)
sys.modules[spec.name]=package
spec.loader.exec_module(package)
runtime=Path("artifacts/local/one-airk-1.8.14/windows-debug-r11/runtime").resolve()
source=package.runtime_builder().source_identity()
assert source=={"sha256":"7d4956f0cafc7307c3dc07146779e8bb247031caa18649d3a029d00203262f84","files":1104}
files,_=package.validate_runtime(runtime,"1.8.14","x86_64-pc-windows-msvc",source)
assert package.inventory(runtime)==files
harness=Path("scripts/check-standalone-adoption.py")
assert package.sha256(runtime/"airk-install.json")=="4be80317bdb25c621ebcc0b65c02543b6e51c2a344a0c03af553d5075412def4"
assert package.sha256(runtime/"airk.exe")=="874d26037b5d66b2ae6cfb14e51600ae0c19d17e77582e84c22beb7d348974c7"
report={"reused_runtime":True,"fresh_runtime_build":False,"compiled_source":source,"sealed_files":len(files),"runtime":str(runtime),"harness_sha256":package.sha256(harness),"scope":"Validated per-request verification-clock comparison changed; all compiler production inputs and full sealed payload unchanged"}
with Path("artifacts/local/one-airk-1.8.14/windows-debug-r11-reuse-for-adoption-r14.json").open("x") as out:json.dump(report,out,indent=2)
print(json.dumps(report))
