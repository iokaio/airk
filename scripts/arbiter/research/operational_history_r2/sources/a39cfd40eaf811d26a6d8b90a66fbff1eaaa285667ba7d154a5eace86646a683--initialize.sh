# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -eu
RUN=/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk
cd "$RUN"
python3 - <<'VERIFY_ARCHIVE'
import hashlib
from pathlib import Path
assert hashlib.sha256(Path('source.tar.gz').read_bytes()).hexdigest()=='578cdf11146a97f1c8c9139b9419d472692193052876fd98ab01cbb04488748e'
VERIFY_ARCHIVE
mkdir source
tar -xzf source.tar.gz -C source
git init -q source
git -C source -c core.autocrlf=false add .
nohup /bin/bash "$RUN/run.sh" > "$RUN/campaign.log" 2>&1 < /dev/null &
printf '%s\n' "$!" > "$RUN/controller-pid.txt"
cat "$RUN/controller-pid.txt"
