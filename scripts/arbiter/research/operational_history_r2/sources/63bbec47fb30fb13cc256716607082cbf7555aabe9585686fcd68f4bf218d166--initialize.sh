# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -eu
RUN=/Users/tylerjensen/airk-builds/p6f-focused-final-20261104.IsswrU
cd "$RUN"
python3 - <<'VERIFY_ARCHIVE'
import hashlib
from pathlib import Path
assert hashlib.sha256(Path('source.tar.gz').read_bytes()).hexdigest()=='e5647747e0c3a69a140b8a682e7b7225c6310fbb0d2e04acccf732ead75c5924'
VERIFY_ARCHIVE
mkdir source
tar -xzf source.tar.gz -C source
git init -q source
git -C source -c core.autocrlf=false add .
nohup /bin/bash "$RUN/run.sh" > "$RUN/campaign.log" 2>&1 < /dev/null &
printf '%s\n' "$!" > "$RUN/controller-pid.txt"
cat "$RUN/controller-pid.txt"
