# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -eu
RUN=/Users/tylerjensen/airk-builds/p6-full-20261104.zS15Hn
cd "$RUN"
python3 - <<'VERIFY_ARCHIVE'
import hashlib
from pathlib import Path
assert hashlib.sha256(Path('source.tar.gz').read_bytes()).hexdigest()=='7f30d56188087863e2a2214ce847c041dca28bc911148acdf627de03e26aeb9d'
VERIFY_ARCHIVE
mkdir source
tar -xzf source.tar.gz -C source
cd source
git read-tree HEAD
cd "$RUN"
nohup /bin/bash "$RUN/run.sh" > "$RUN/campaign.log" 2>&1 < /dev/null &
printf '%s\n' "$!" > "$RUN/controller-pid.txt"
cat "$RUN/controller-pid.txt"
