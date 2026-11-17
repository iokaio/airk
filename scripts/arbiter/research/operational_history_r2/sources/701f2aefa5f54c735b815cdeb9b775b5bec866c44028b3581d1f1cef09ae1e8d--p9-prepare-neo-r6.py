# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
s=Path('artifacts/v190/p9-launch-neo-20261105-r5.py').read_text().replace('-r5','-r6')
at=s.index("    {'name':'native-permanent-trim'")
s=s[:at]+"    {'name':'kernel-policy','command':['cargo','test','-p','airk-kernel','--test','source_policy','--all-features','--locked','--offline','--','--test-threads=1']},\n    {'name':'store-retirement','command':['cargo','test','-p','airk-store','--lib','--all-features','--locked','--offline','retirement_','--','--test-threads=1']},\n"+s[at:]
at=s.index("    {'name':'clippy'")
s=s[:at]+"    {'name':'pre-policy-build','command':['python3','{run}/pre-policy-prepare.py']},\n    {'name':'pre-policy-native','command':['python3','{run}/pre-policy-run.py']},\n"+s[at:]
start=s.index('for local,name in [')
stop=s.index(']:\n    subprocess.run([*scp',start)
s=s[:stop]+",(base/'p9-pre-policy-probe-20261105-r1/source.tar','pre-policy-source.tar'),(base/'p9-pre-policy-probe-20261105-r1/receipt.json','pre-policy-inputs.json'),(base/'p9-pre-policy-owner-probe.rs','pre-policy-driver.rs'),(base/'p9-neo-prepare-pre-policy-probe.py','pre-policy-prepare.py'),(base/'p9-neo-run-pre-policy-qualification.py','pre-policy-run.py')"+s[stop:]
Path('artifacts/v190/p9-launch-neo-20261105-r6.py').write_text(s)
