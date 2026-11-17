# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os
p=Path.home()/'airk-builds/arbiter-final-20261116-r1';e=p/'evidence';o=p/'inputs/static-ort130-aarch64'
source=e/'source-custody-r1.json';start=e/'prerequisite-start-r1.json'
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'source_custody':json.loads(source.read_text()),'prerequisite_start':json.loads(start.read_text()),'current_build_log_tail':(o/'logs/build.log').read_text(errors='replace')[-1000:]},separators=(',',':')))
REMOTE
