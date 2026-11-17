# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
s=Path('artifacts/v190/p9-launch-neo-20261105-r4.py').read_text().replace('-r4','-r5')
at=s.index("    {'name':'read-only-contention'")
s=s[:at]+"    {'name':'store-policy-recovery','command':['cargo','test','-p','airk-store','--lib','--all-features','--locked','--offline','policy','--','--test-threads=1']},\n"+s[at:]
Path('artifacts/v190/p9-launch-neo-20261105-r5.py').write_text(s)
