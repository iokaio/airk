# SPDX-License-Identifier: Apache-2.0
"""Closed eight-path historical source epoch; no live discovery or model imports."""
import hashlib,json
from pathlib import Path

PATHS=frozenset(['Cargo.lock','crates/airk-core/src/arbiter/mod.rs',
'crates/airk-core/src/arbiter/context_action.rs','crates/airk-core/src/arbiter/context_action/calibration.rs',
'crates/airk-core/src/arbiter/focus.rs','crates/airk-retrieval/src/arbiter/compiler/focus.rs',
'crates/airk-retrieval/src/arbiter/runtime/qualification.rs','crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration.rs'])
BUILD='75aa28c484e8121957f0ee590b0f8376233bf14bedc88dfef88bf1ec0ea5df36'
GROUP='dbc89b3cca981d1c87c2340c8697ac65d7f6a844ece6f24bedad336857d3eace'
CENSUS='237d7cd102961c80a6004f7fb0b1f1caa2631208829b36c83dc7b812a3be7d33'
IMAGE='60b8d3e568054a5fa23007c616b2f6e582d1d5a2718a89484733a39941158665'
OWNED='8d2d8428674f2a0b76674c3606575e82bcb9afea4e228c31ed9971a2362caf55'
HELD='6f1cf1c880958c2f8c465495852e6f1bf82253533593081cd1b99ccff511f8f0'


def closure_sha(values):
    return hashlib.sha256(json.dumps(values,sort_keys=True,separators=(',',':')).encode()).hexdigest()


def advance(held,original,epoch,build,emission):
    if epoch['schema']!='airk.private.F1-operational-source-epoch/1':raise ValueError('unknown source epoch')
    for key,expected in [('held_failed_plan',HELD),('native_build_receipt',BUILD),('native_census',CENSUS),('native_image',IMAGE),('native_owned_receipt',OWNED)]:
        if epoch[key]['sha256']!=expected:raise ValueError('wrong authentic epoch parent: '+key)
    if epoch['native_census']!=held['native_census']or epoch['native_owned_receipt']!=held['native_owned_receipt']:raise ValueError('census lineage changed')
    if emission['native_image']['sha256']!=IMAGE:raise ValueError('actual census image changed')
    if build.get('schema')!='airk-readiness-command/1'or build.get('status')!='completed'or build.get('passed')is not True or build.get('inputs_unchanged')is not True or build.get('exit_code')!=0 or build.get('changed_groups')!=[]or build['groups']['native']['sha256']!=GROUP:raise ValueError('authentic unchanged CPU census build required')
    if held['reference_module']!=original['reference_module']or held['versions']!=original['versions']:raise ValueError('original reference math/runtime changed')
    baseline=held['input_seals'];root=Path(held['repository_root']);mapping=epoch['changes']
    if len(baseline)!=13274 or len(mapping)!=8 or {x['relative_path']for x in mapping}!=PATHS:raise ValueError('closed original13274/eight source paths required')
    original_pin=held['execution_sources'].get(epoch['original_reference_plan']['path'])
    if original_pin!=epoch['original_reference_plan']:raise ValueError('original reference plan provenance changed')
    changed={str(root/x)for x in PATHS};unchanged={p:v for p,v in baseline.items()if p not in changed}
    if len(unchanged)!=13266 or closure_sha(unchanged)!=epoch['unchanged13266_sha256']:raise ValueError('immutable13266 consumer closure changed')
    for path,value in original['input_seals'].items():
        if baseline.get(path)!=value:raise ValueError('original inherited reference seal changed')
    result=dict(baseline)
    for row in mapping:
        path=str(root/row['relative_path']);expected=build['groups']['native']['files'][row['relative_path']]
        if baseline.get(path)!=row['old_sha256']or row['new_sha256']!=expected or expected==row['old_sha256']:raise ValueError('epoch source must advance only authentic reviewed old/new pair')
        result[path]=expected
    if set(result)!=set(baseline)or sum(result[p]!=baseline[p]for p in baseline)!=8:raise ValueError('operative seal denominator changed')
    return result
