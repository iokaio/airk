# SPDX-License-Identifier: Apache-2.0
"""Read retained evidence/static public drivers only; never contact a running editor."""
from pathlib import Path
import hashlib,json,re,zipfile,os
A=Path(__file__).parent;W=A/'p10-worktree';P=W/'artifacts/editor-qualification/native-installed-z38fCa/same'
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();read=lambda p:json.loads(p.read_text(encoding='utf-8'))
s=(W/'scripts/editor-ui.cjs').read_text(encoding='utf-8')
line=next(line for line in s.splitlines() if 'const values=${JSON.stringify(values)}' in line)
templates=re.findall(r'`([^`]+)`',line);assert len(templates)==2
request=read(P/'b-ui-request.json')
values={'Index ID from this grant':request['index'],'Project IDs from this grant (1–32, comma separated)':request['project']}
j=lambda value:json.dumps(value,ensure_ascii=False,separators=(',',':'))
expression=templates[1].replace('${JSON.stringify(button)}',j('Use Grant for this window')).replace('${JSON.stringify(values)}',j(values))
dispatch=read(P/'b-ui-dispatch-receipts.json')[-1]
assert dispatch['dispatch']==6 and hashlib.sha256(expression.encode()).hexdigest()==dispatch['expression_sha256']
vsix=Path(read(A/'p14-package-editor-owner-r2.json')['vsix']['path'])
with zipfile.ZipFile(vsix) as z:
    bundles=[name for name in z.namelist() if name.endswith('/out/explorerWebview.js')]
    assert len(bundles)==1;bundle=z.read(bundles[0]).decode('utf-8')
old='Index ID from this grant';new='Optional index subset from this grant (1–32 IDs; blank includes all permitted members)'
assert old not in bundle and new in bundle
assert new in (W/'packages/vscode/src/explorerWebview/indexing.tsx').read_text(encoding='utf-8')
receipts={};states=[]
for role in ['a','b']:
    for path in sorted(P.glob(role+'-ui-done-*.json')):
        value=read(path);receipts[path.name]=h(path)
        states.append({'file':path.name,'generation':value.get('generation'),'error':value.get('error'),
            'outcome_status':value.get('outcome',{}).get('status'),'busy':value.get('busy')})
    for suffix in ['-ui-request.json','-ui-dispatch-receipts.json','-editor.log','-metadata-ready.json']:
        path=P/(role+suffix);receipts[path.name]=h(path)
environment={name:os.environ.get(name) for name in ['VSCODE_CWD','VSCODE_CODE_CACHE_PATH','VSCODE_NODE_COMPILE_CACHE_ROOT','VSCODE_NODE_COMPILE_CACHE_KIND','VSCODE_GENERATE_NODE_COMPILE_CACHE','VSCODE_ESM_ENTRYPOINT','ELECTRON_RUN_AS_NODE']}
out={'status':'READONLY_DIAGNOSIS_NO_REPLAY_NO_LIVE_EDITS','source':{'sha256':'5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11','files':960},
 'failure':'B useGrant action sequence6 failed captured-context dispatch6; A29 dispatches completed; B5 completed then6 unknown',
 'exact_expression_sha256_verified':dispatch['expression_sha256'],'request':request,
 'concrete_driver_defect':{'path':'scripts/editor-ui.cjs','line':163,'stale_first_field_label':old,'canonical_and_packaged_label':new,
     'canonical_source':'packages/vscode/src/explorerWebview/indexing.tsx:113','vsix_sha256':h(vsix),'packaged_bundle':bundles[0],
     'scope':'UseGrant still selects exact requested single index and Project. Fix only field locator; never blank optional subset/widen scope.'},
 'dispatch_limit':'The browser return value was not retained; existing Unknown result remains Unknown. The exact expression hash and shipped label establish the stale lookup. Never replay this old action or relabel failed fixture as passed.',
 'retained_full_receipt_hashes':receipts,'receipt_states':states,
 'current_environment_observation':environment,
 'environment_scope':'Current agent environment only, not a saved exact historical child env. Actual two editor logs show the same configured normal-install node cache. Official bootstrap accepts inherited VSCODE_NODE_COMPILE_CACHE_ROOT; absent GENERATE uses readOnly cache. No origin link to label failure established.',
 'environment_boundary':'Canonical check-native-editor environment currently removes only ELECTRON_RUN_AS_NODE and VSCODE_IPC_HOOK_CLI. Inherited editor bootstrap/cache/entrypoint variables can point at normal editor; sanitize child-only VSCODE_* while preserving explicit AIRK fixture variables/private profile flags. Do not alter global environment, normal files, stock product or identity.',
 'cleanup':'Original native-stop receipt absent and original cleanup unqualified. Parent separately verified quiescence/repair/inspect in p14-editor-failed-fixture-cleanup-r2.json; lifecycleStoppedfalse, no Stop dispatch claimed.',
 'other_callers':'All fill callsites audited: setup/project/rename/membership/association/grant-draft labels match current renderer; actual A completed preceding setup/membership/grant flow. New UseGrant optional-index text is the concrete stale label. Generic fill readiness checks only form existence, missing fields can produce unknown; preflight all fields before any setter and verify exact values before submit.'}
dest=A/'p14-editor-dispatch-independent-diagnosis-r1.json';dest.write_bytes((json.dumps(out,indent=2)+'\n').encode())
print('exact B6 expression + packagedlabel verified; receipts',len(receipts),'diagnosis',h(dest))
