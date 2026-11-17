# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os
p=Path.home()/'airk-builds/arbiter-final-20261116-r1';e=p/'evidence';o=p/'inputs/static-ort130-aarch64'
rows=[json.loads(x) for x in (e/'prerequisite-events-r1.jsonl').read_text().splitlines()]
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'events':[{k:r[k] for k in ('event','index','pid','exit_code','elapsed_seconds') if k in r} for r in rows],'build_log_bytes':(o/'logs/build.log').stat().st_size,'build_tail':(o/'logs/build.log').read_text(errors='replace')[-1600:],'result_present':(e/'prerequisite-result-r1.json').exists(),'existing_rust_toolchain_directories':[q.name for q in (Path.home()/'.rustup/toolchains').glob('*') if q.is_dir()]},indent=2))
REMOTE
