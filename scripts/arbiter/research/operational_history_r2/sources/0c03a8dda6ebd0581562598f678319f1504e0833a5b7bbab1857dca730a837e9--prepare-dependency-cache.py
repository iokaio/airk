# SPDX-License-Identifier: Apache-2.0
import json,shutil,subprocess,sys
from pathlib import Path
run=Path(sys.argv[1]);root=run/'source';base=root/'artifacts/dependency-policy';base.mkdir(parents=True,exist_ok=True);db=base/'cargo-audit-db';db.mkdir()
expected='965bf9ad083b58e6340d65829d85217b18be9290'
for args in [['init','-q'],['remote','add','origin','https://github.com/RustSec/advisory-db'],['fetch','--depth','1','origin',expected],['checkout','--detach','FETCH_HEAD']]:subprocess.run(['git',*args],cwd=db,check=True)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=db,text=True).strip()==expected
other=base/'advisory-dbs/advisory-db-3157b0e258782691';other.parent.mkdir();shutil.copytree(db,other)
(run/'dependency-cache.json').write_text(json.dumps({'public_database':'https://github.com/RustSec/advisory-db','commit':expected,'matches_windows_offline_control':True,'fresh_full_advisory_check':False},indent=2))
