# SPDX-License-Identifier: Apache-2.0
set -eu
cd "$HOME/code/airk"
"$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13" -I -B - <<'PY'
import base64,json,subprocess
from pathlib import Path
root=Path('artifacts/advanced-split/receipts/mac-cli-await-r10');files={};total=0
for p in sorted(root.rglob('*')):
 if not p.is_file() or p.is_symlink() or 'native-fixtures' in p.relative_to(root).parts:continue
 assert p.suffix in ('.json','.log','.xml','.html','.py','.txt'),p
 assert p.stat().st_size<=8*1024*1024,p
 total+=p.stat().st_size;assert total<=32*1024*1024
 files[p.relative_to(root).as_posix()]=base64.b64encode(p.read_bytes()).decode()
processes=subprocess.check_output(['/bin/ps','-axo','comm='],text=True).splitlines();active=[p.strip() for p in processes if Path(p.strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt')]
print(json.dumps({'files':files,'active_compilers':active}))
PY
