# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
s=Path('artifacts/v190/p9-launch-neo-20261105-r3.py').read_text()
for label in ['p9-source-20261105','p9-native-20261105','p9-neo-plan-20261105','p9-neo-owner-20261105','p9-neo-active-20261105']:
    s=s.replace(label+'-r3',label+'-r4')
a=s.index("    {'name':'kernel-policy-unit'");b=s.index("    {'name':'store-catalog'",a)
s=s[:a]+s[b:]
at=s.index("    {'name':'native-permanent-trim'")
s=s[:at]+"    {'name':'read-only-contention','command':['cargo','test','-p','airk-cli','--lib','--all-features','--locked','--offline','registry_read_contention_preserves_receipts_and_other_failures','--','--test-threads=1']},\n"+s[at:]
Path('artifacts/v190/p9-launch-neo-20261105-r4.py').write_text(s)
