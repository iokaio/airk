# SPDX-License-Identifier: Apache-2.0
"""Prepare a historical candidate roster for human review; never copy source."""
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[4]
INPUT=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z/source-non-script-review-r1.json'
FAMILIES={'optimizations':'F02-F18 native/client optimization source and base proposals','v190':'P6-P14 native/client implementation proposals and before-edit snapshots','advanced-split':'native service/host/kernel/Arbiter preparation and receipt source snapshots','qa-1.8.9':'composed native/client capability, transport and retry source proposals','local':'explicit native cross-host retirement and checkpoint drafts','branch-review-r1':'quiet watcher fixture implementation proposals'}
STANDALONE={'artifacts/v190/final-plan/exec-20261116T052942Z/ui-workbench-build-r1/pre-overflow-fix-style.css','artifacts/catalog_normalized.rs','artifacts/verified_cache.rs','artifacts/recovered-indexing-pre-edit.tsx','artifacts/recovered-indexing-pre-edit-latest.tsx'}
EXCLUDE=['/p10-target/','/msvc-configure-','/native-fixtures','/post-p14-focused-run-r1-evidence/','/project-','/workspace/','/p6f-resume-legacy-upgrade-','/sigstore-upstream-','/rust-1.98-unix-time.rs']

def main():
    input_raw=INPUT.read_bytes();data=json.loads(input_raw);selected=[];rejected=[]
    for row in data['rows']:
        p=row['path'];family=p.split('/')[1]
        if row['review_status']!='unresolved' or Path(p).suffix not in {'.rs','.ts','.tsx'} and p not in STANDALONE:continue
        if family not in FAMILIES and p not in STANDALONE:continue
        if any(s in p for s in EXCLUDE):continue
        raw=(ROOT/p).read_bytes()
        if hashlib.sha256(raw).hexdigest()!=row['source_sha256'] or len(raw)!=row['bytes']:raise ValueError('original changed '+p)
        if len(raw)<70:rejected.append(p);continue
        text=raw.decode('utf-8',errors='replace')
        if any(x in text for x in ['-----BEGIN PRIVATE KEY-----','-----BEGIN RSA PRIVATE KEY-----']):raise ValueError('secret-shaped literal '+p)
        markers=[x for x in ['airk_','AIRK','use super::','use crate::','crate::','vscode','workbench','nativeRead','serviceConsole','RootObservation','Focus','arbiter'] if x in text]
        if not markers and p not in json.loads(Path(__file__).with_name('reviewed-additions.json').read_bytes())['paths']:rejected.append(p);continue
        selected.append({'source':p,'source_sha256':row['source_sha256'],'source_bytes':row['bytes'],'family':family,'role':FAMILIES.get(family,'standalone recovered AIRK indexing/catalog/cache implementation'),'content_markers':markers,'original_first_line_spdx':raw.splitlines()[0]==b'// SPDX-License-Identifier: Apache-2.0','non_utf8':'\ufffd' in text})
    print(json.dumps({'schema':'airk.historical-non-script-selection/1','status':'awaiting independent family review before source capture','input':{'path':INPUT.relative_to(ROOT).as_posix(),'sha256':hashlib.sha256(input_raw).hexdigest()},'paths':len(selected),'unique_source_sha256':len({r['source_sha256'] for r in selected}),'files':selected,'unclassified_in_selected_families':rejected},indent=2))

if __name__=='__main__':main()
