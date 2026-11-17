# SPDX-License-Identifier: Apache-2.0
"""Stage the bounded catalog-clock correction without changing frozen sources."""
import difflib
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent / 'scoped-clock-staged'
changes = {}


def edit(name, before, after):
    original, current = changes.get(name, ((ROOT / name).read_text(encoding='utf-8'), None))
    current = original if current is None else current
    assert current.count(before) == 1, (name, before[:100], current.count(before))
    changes[name] = original, current.replace(before, after, 1)


edit('packages/vscode/src/readRetry.ts',
'''        try {if(!isDeepStrictEqual(validateCatalogSession(scope.selection),validateCatalogSession(expectedScope)))return;}catch{return;}''',
'''        try {
            const observed=validateCatalogSession(scope.selection),expected=validateCatalogSession(expectedScope);
            // The native resolver refreshes only the metadata clock before
            // admission. The original grant, scope and launch arguments remain
            // fixed; a backwards clock or changed authority never permits replay.
            if(observed.expected_catalog_revision<expected.expected_catalog_revision
                ||!isDeepStrictEqual({...observed,expected_catalog_revision:expected.expected_catalog_revision},expected))return;
        }catch{return;}''')

edit('packages/vscode/src/test/diagnosticsNative.test.ts',
'''    try{
        const a=await fixture.register('diagnostic-a'),b=await fixture.register('diagnostic-b');''',
'''    try{
        await fixture.requireResidentProjects(2);
        const a=await fixture.register('diagnostic-a'),b=await fixture.register('diagnostic-b');''')

edit('packages/vscode/src/test/nativeReadRetry.test.ts',
'''test('scoped preparation retry requires the original exact launch selection and an explicit known pre-query hint',async()=>{''',
'''test('scoped preparation retry keeps original authority across a monotonic metadata-clock refresh',async()=>{''')
edit('packages/vscode/src/test/nativeReadRetry.test.ts',
'''    assert.equal(readRetryDelay(timeout),undefined);
    for(const code of ['catalog_connection_busy','project_capacity_busy','project_restart_backoff','previous_workers_draining'])''',
'''    assert.equal(readRetryDelay(timeout),undefined);
    for(const initial of [timeout,scopedSetupReceipt('scoped_connection_unavailable','project_capacity_busy')]){
        const advanced=structuredClone(initial);
        advanced.structuredContent.requested_scope.selection.expected_catalog_revision=12;
        advanced.content[0].text=JSON.stringify(advanced.structuredContent);
        const before=structuredClone(advanced),original=structuredClone(preparedSelection);
        assert.equal(readRetryDelay(advanced,preparedSelection),1,'native clock 7 to 12 retains the original reviewed authority');
        assert.deepEqual(advanced,before,'retain the complete native receipt');
        assert.deepEqual(preparedSelection,original,'never rewrite the caller selection');
    }
    for(const code of ['catalog_connection_busy','project_capacity_busy','project_restart_backoff','previous_workers_draining'])''')
edit('packages/vscode/src/test/nativeReadRetry.test.ts',
'''        (body:any)=>{body.requested_scope.selection.expected_catalog_revision++;},(body:any)=>{body.hits=[];},''',
'''        (body:any)=>{body.requested_scope.selection.expected_catalog_revision=6;},(body:any)=>{body.hits=[];},
        (body:any)=>{body.requested_scope.selection.expected_grant_revision=2;},
        (body:any)=>{body.requested_scope.selection.namespace='09'.repeat(16);},
        (body:any)=>{body.requested_scope.selection.context_key='09'.repeat(32);},
        (body:any)=>{body.requested_scope.selection.index_id='idx_'+'09'.repeat(16);},
        (body:any)=>{body.requested_scope.selection.project_ids=['prj_'+'09'.repeat(16)];},
        (body:any)=>{body.error.query_replay_safe=false;},
        (body:any)=>{body.status='retry';body.code='publication_busy';body.retryable=true;},''')
edit('packages/vscode/src/test/nativeReadRetry.test.ts',
'''    const replies=[timeout,{index_state:'available'}];let calls=0;const deadlines:number[]=[];''',
'''    const advanced=structuredClone(timeout);advanced.structuredContent.requested_scope.selection.expected_catalog_revision=12;
    advanced.content[0].text=JSON.stringify(advanced.structuredContent);
    const replies=[advanced,{index_state:'available'}];let calls=0;const deadlines:number[]=[];''')

# Real leaf processes assert every repeated command still carries the original
# exact selector bytes. Each receipt can refresh its clock without widening it.
name = 'packages/vscode/src/test/nativeReadRetryProcess.test.ts'
before = (ROOT / name).read_text(encoding='utf-8')
start = before.index("test('CLI setup-timeout retries retain the original exact catalog launch selection'")
assert before[start:].rstrip().endswith('});')
after = r'''test('CLI scoped retries accept forward metadata clocks without rewriting launch arguments',async()=>{
    const selection={schema:1,context_key:'06'.repeat(32),namespace:'07'.repeat(16),expected_catalog_revision:7,
        grant_id:'grt_'+'03'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'01'.repeat(16),project_ids:['prj_'+'02'.repeat(16)]};
    const confirmed={content:[{type:'text',text:'{"files":0}'}]};
    for(const code of ['scoped_status_prepare_timeout','scoped_connection_unavailable']){
        const body={error:{code,query_submitted:false,retryable:true,retry_after_ms:1,fresh_brokerage_required:true,
            ...(code==='scoped_connection_unavailable'?{native_control_response:{status:'failure',code:'project_capacity_busy'}}:{})},
            requested_scope:{confirmed:false,selection:{...selection,expected_catalog_revision:12}}};
        const setup={isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body};
        await fixture(async(directory,counter)=>{
            const original=JSON.stringify(selection);
            const source=program(counter,`if(process.argv[process.argv.indexOf('--catalog-session')+1]!==${JSON.stringify(original)})throw new Error('original launch selector changed');`+
                `console.log(JSON.stringify(calls===1?${JSON.stringify(setup)}:${JSON.stringify(confirmed)}));process.exitCode=calls===1?1:0;`);
            const args=['-e',source,'--','--catalog-session',original];
            assert.deepEqual(await readNative(process.execPath,args,directory),confirmed);
            assert.equal(await fs.readFile(counter,'utf8'),'2');
        });
        for(const selected of [{...selection,expected_grant_revision:2},{...selection,expected_catalog_revision:13}]){
            await fixture(async(directory,counter)=>{
                const source=program(counter,`console.log(${JSON.stringify(JSON.stringify(setup))});process.exitCode=1;`);
                const args=['-e',source,'--','--catalog-session',JSON.stringify(selected)];
                await assert.rejects(readNative(process.execPath,args,directory),error=>error instanceof NativeProcessError&&error.code===1);
                assert.equal(await fs.readFile(counter,'utf8'),'1');
            });
        }
    }
});
'''
changes[name] = before, before[:start] + after

edit('crates/airk-cli/tests/common/read_retry.rs',
'''    let mut expected = expected.clone();
    expected.project_ids.sort();
    observed.project_ids.sort();''',
'''    let mut expected = expected.clone();
    // Native resolve_scoped may advance this metadata clock; every authority
    // field and the caller's original command remain unchanged.
    if observed.expected_catalog_revision < expected.expected_catalog_revision {
        return None;
    }
    observed.expected_catalog_revision = expected.expected_catalog_revision;
    expected.project_ids.sort();
    observed.project_ids.sort();''')

edit('crates/airk-cli/tests/native_read_retry.rs',
'''        "expected_catalog_revision":1,"grant_id":format!("grt_{}","03".repeat(16)),"expected_grant_revision":1,''',
'''        "expected_catalog_revision":7,"grant_id":format!("grt_{}","03".repeat(16)),"expected_grant_revision":1,''')
edit('crates/airk-cli/tests/native_read_retry.rs',
'''    assert!(read_retry::classify_for_scope(&refused, &expected).is_some());''',
'''    assert!(read_retry::classify_for_scope(&refused, &expected).is_some());
    let mut advanced = hint.clone();
    advanced["requested_scope"]["selection"]["expected_catalog_revision"] = json!(12);
    let advanced = scoped_setup_envelope(advanced);
    let original_receipt = advanced.clone();
    let original_selection = expected.clone();
    assert!(read_retry::classify_for_scope(&advanced, &expected).is_some());
    assert_eq!(advanced, original_receipt, "retain the exact native receipt");
    assert_eq!(expected, original_selection, "never rewrite the caller selection");''')
edit('crates/airk-cli/tests/native_read_retry.rs',
'''            "/requested_scope/selection/expected_catalog_revision",
            json!(2),''',
'''            "/requested_scope/selection/expected_catalog_revision",
            json!(6),''')
edit('crates/airk-cli/tests/native_read_retry.rs',
'''        ("/requested_scope/confirmed", json!(true)),''',
'''        ("/requested_scope/confirmed", json!(true)),
        ("/error/query_submitted", json!(true)),
        ("/error/query_replay_safe", json!(false)),
        ("/requested_scope/selection/namespace", json!("09".repeat(16))),
        ("/requested_scope/selection/context_key", json!("09".repeat(32))),
        ("/requested_scope/selection/index_id", json!(format!("idx_{}", "09".repeat(16)))) ,''')
# Inserting optional fields needs object indexing, not pointer_mut on absent keys.
edit('crates/airk-cli/tests/native_read_retry.rs',
'''        ("/error/query_replay_safe", json!(false)),
''','')
edit('crates/airk-cli/tests/native_read_retry.rs',
'''    let mut native = hint.clone();
    native["error"]["code"] = json!("scoped_connection_unavailable");''',
'''    for (field, value) in [("query_replay_safe", json!(false)), ("unexpected", json!(true))] {
        let mut invalid = hint.clone();
        invalid["error"][field] = value;
        assert!(read_retry::classify_for_scope(&scoped_setup_envelope(invalid), &expected).is_none());
    }
    let mut mixed = hint.clone(); mixed["hits"] = json!([]);
    assert!(read_retry::classify_for_scope(&scoped_setup_envelope(mixed), &expected).is_none());
    let mut native = hint.clone();
    native["requested_scope"]["selection"]["expected_catalog_revision"] = json!(12);
    native["error"]["code"] = json!("scoped_connection_unavailable");''')
edit('crates/airk-cli/tests/native_read_retry.rs',
'''        let mut refusal = scoped_setup_hint(&expected);
        if crossed {''',
'''        let mut refusal = scoped_setup_hint(&expected);
        refusal["requested_scope"]["selection"]["expected_catalog_revision"] = json!(12);
        if crossed {''')

edit('docs/read-interface-parity.md',
'''remaining result, including error receipts, scope, provenance and retry details.
''',
'''remaining result, including error receipts, scope, provenance and retry details.

An explicit, unsubmitted scoped-read retry may report a catalog metadata clock
newer than the caller's original selector: the native resolver refreshes that
clock before admission. Clients accept only a monotonic clock with the same
validated context, namespace, index, Project selection, grant and grant revision.
They repeat the original arguments within the original deadline and retain the
complete receipt. Backward clocks, changed authority, unknown errors and
submitted or uncertain work do not authorize a retry.
''')

OUT.mkdir(parents=True, exist_ok=True)
for name, (_original, value) in changes.items():
    destination = OUT / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(value, encoding='utf-8', newline='\n')
if '--format' in sys.argv:
    formatter = 'C:/Users/tdotn/.rustup/toolchains/1.98.0-x86_64-pc-windows-msvc/bin/rustfmt.exe'
    subprocess.run([formatter, '--edition', '2024', '--config', 'skip_children=true',
                    *(str(OUT / name) for name in changes if name.endswith('.rs'))], check=True)
patch = []
manifest = []
for name, (original, _value) in changes.items():
    destination = OUT / name
    value = destination.read_text(encoding='utf-8')
    patch.extend(difflib.unified_diff(original.splitlines(True), value.splitlines(True),
                                    fromfile='a/' + name, tofile='b/' + name))
    manifest.append({'path': name, 'original_sha256': hashlib.sha256((ROOT / name).read_bytes()).hexdigest(),
                     'staged_sha256': hashlib.sha256(destination.read_bytes()).hexdigest()})
(OUT.parent / 'scoped-clock.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
(OUT.parent / 'scoped-clock-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'files': len(changes), 'patch': str(OUT.parent / 'scoped-clock.patch')}))
