# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/v190/p9-launch-neo-20261105.py')
s=p.read_text()
for label in ['p9-source-20261105','p9-native-20261105','p9-neo-plan-20261105','p9-neo-owner-20261105','p9-neo-active-20261105']:
    s=s.replace(label+'-r1',label+'-r2')
s=s.replace('/opt/homebrew/bin/python3.13','/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13')
s=s.replace("'--locked','--offline','--','--test-threads=2'","'--locked','--offline','--no-fail-fast','--','--test-threads=2'")
Path('artifacts/v190/p9-launch-neo-20261105-r2.py').write_text(s)
