// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { NativeProcessError, NativeWriteQueue, joinScheduledIndex, ScheduledIndexStillQueued, parseIndexProgress, prepareHelper, probeMcp, runJson, runText } from '../native';
import { LaunchSpec, launchArgs } from '../model';
import { NativeServiceFixture } from './nativeServiceFixture';
import { record } from '../serviceClient';

async function removeFixture(directory: string, prefix: string): Promise<void> {
    const resolved = await fs.realpath(directory), base = await fs.realpath(os.tmpdir());
    assert.equal(path.dirname(resolved), base); assert.ok(path.basename(resolved).startsWith(prefix));
    await fs.rm(resolved, { recursive: true, force: true });
}

test('only a structured gate outcome with an ordinary exit authorizes contention retry', async () => {
    const busy = { airk_error: 1, code: 'current_gate_busy', retryable: true, message: 'localized gate diagnostic' };
    const failure = (stderr: string, properties: object = { code: 1 }) => new NativeProcessError('airk',
        Object.assign(new Error('Command failed'), properties), stderr, 1, 10000);
    assert.equal(failure(JSON.stringify(busy)).retryableGateContention, true);
    const progress = JSON.stringify({ phase: 'scan_started', revision: null, processed_artifacts: 0,
        committed_artifacts: 0, selected_artifacts: null, completed_artifacts: null });
    assert.equal(failure(`${progress}\n${JSON.stringify(busy)}`).retryableGateContention, true,
        'Validated index progress may precede a structured native failure');
    assert.equal(failure(`usage warning\n${JSON.stringify(busy)}`).retryableGateContention, true);
    for (const stderr of [
        'Error: current index is busy applying an update\nCaused by:\n    lock acquisition failed because the operation would block',
        JSON.stringify({ ...busy, airk_error: 2 }), JSON.stringify({ ...busy, retryable: false }),
        JSON.stringify({ ...busy, code: 'operation_failed' }), JSON.stringify({ ...busy, message: null }),
        JSON.stringify(busy) + '\n' + JSON.stringify({airk_error: 1, retryable: 'invalid'}), '[]', 'null', '{'
    ]) { assert.equal(failure(stderr).retryableGateContention, false); }
    for (const properties of [{ code: 7 }, { code: 1, killed: true }, { code: 1, signal: 'SIGTERM' }]) {
        assert.equal(failure(JSON.stringify(busy), properties).retryableGateContention, false);
    }
    await assert.rejects(runText(process.execPath, ['-e',
        'if (process.env.AIRK_ERROR_FORMAT !== "json") process.exit(7); process.stderr.write(JSON.stringify({airk_error:1,code:"current_gate_busy",retryable:true,message:"gate"}), () => process.exit(1))']), error => {
        assert.ok(error instanceof NativeProcessError);
        assert.equal(error.retryableGateContention, true);
        return true;
    });
});

test('structured errors display the native message and parse before diagnostic truncation', async () => {
    // 4,096 characters is within the CLI message limit; JSON escaping exceeds
    // the old 8,192-character parser cutoff even though the message is bounded.
    const prefix = 'Retained object unavailable: ';
    const message = prefix + '\\'.repeat(4096 - prefix.length);
    assert.equal(message.length, 4096);
    const stderr = JSON.stringify({ airk_error: 1, code: 'operation_failed', retryable: false, message });
    assert.ok(stderr.length > 8192);
    const error = new NativeProcessError('airk', Object.assign(new Error('Command failed'), { code: 1 }), stderr, 9, 10000);
    assert.equal(error.nativeMessage, message);
    assert.equal(error.message, `airk failed: exit code 1; elapsed 9 ms\n${message}`);
    assert.equal(error.retryableGateContention, false);
    assert.ok(error.stderr.startsWith('{"airk_error":1'));
    assert.match(error.stderr, /diagnostic truncated/);
    assert.ok(error.stderr.length < 8300);

    const busy = JSON.stringify({ airk_error: 1, code: 'current_gate_busy', retryable: true, message });
    const busyError = new NativeProcessError('airk', Object.assign(new Error('Command failed'), { code: 1 }), busy, 9, 10000);
    assert.equal(busyError.retryableGateContention, true);
    assert.equal(busyError.nativeMessage, message);
    await assert.rejects(runText(process.execPath, ['-e',
        'process.stderr.write(JSON.stringify({airk_error:1,code:"operation_failed",retryable:false,message:"Readable native failure"}), () => process.exit(1))']), failure => {
        assert.ok(failure instanceof NativeProcessError);
        assert.ok(failure.message.endsWith('\nReadable native failure'));
        assert.ok(!failure.message.includes('airk_error'));
        assert.ok(failure.stderr.includes('"airk_error":1'));
        return true;
    });
});

test('native process failures retain exit status and stderr diagnostics', async () => {
    await assert.rejects(runText(process.execPath, ['-e', 'process.stderr.write("fixture indexing failure"); process.exit(7)']), error => {
        assert.ok(error instanceof NativeProcessError);
        assert.equal(error.code, 7); assert.equal(error.killed, false); assert.equal(error.timedOut, false);
        assert.match(error.message, /exit code 7/); assert.match(error.message, /fixture indexing failure/);
        assert.equal(error.stderr, 'fixture indexing failure');
        return true;
    });
});

test('native process timeout identifies the limit and termination without stderr', async () => {
    await assert.rejects(runText(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], undefined, 100), error => {
        assert.ok(error instanceof NativeProcessError);
        assert.equal(error.killed, true); assert.equal(error.timedOut, true); assert.equal(error.timeoutMs, 100);
        assert.match(error.message, /timed out \(limit 100 ms\)/); assert.match(error.message, /terminated/);
        assert.ok(error.elapsedMs >= 90);
        return true;
    });
});

test('index progress is validated and a silent native scan is stopped', async () => {
    const event = { collection: 'fixture', phase: 'base_committed', revision: 'airk-r1:fixture', processed_artifacts: 32,
        committed_artifacts: 32, selected_artifacts: 10, completed_artifacts: 4 };
    assert.deepEqual(parseIndexProgress(JSON.stringify(event)), event);
    assert.deepEqual(parseIndexProgress(JSON.stringify({ ...event, phase: 'enrichment_prepared', revision: null })),
        { ...event, phase: 'enrichment_prepared', revision: null });
    assert.equal(parseIndexProgress(JSON.stringify({ ...event, processed_artifacts: -1 })), undefined);
    assert.equal(parseIndexProgress('{'), undefined);
    await assert.rejects(runText(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], undefined, 5000,
        () => false, 150), /stopped after 150 ms without progress/);
});

test('native process diagnostics stay bounded and retain both ends of stderr', async () => {
    await assert.rejects(runText(process.execPath, ['-e', 'process.stderr.write("first diagnostic\\n" + "x".repeat(20000) + "\\nlast diagnostic", () => process.exit(3))']), error => {
        assert.ok(error instanceof NativeProcessError);
        assert.ok(error.message.length < 9000); assert.ok(error.stderr.length < 8300);
        assert.match(error.message, /first diagnostic/); assert.match(error.message, /last diagnostic/);
        assert.match(error.message, /diagnostic truncated/);
        return true;
    });
});

test('native output cap remains distinguishable from a timeout', async () => {
    await assert.rejects(runText(process.execPath, ['-e', 'process.stdout.write("x".repeat(17 * 1024 * 1024)); setInterval(() => {}, 1000)']), error => {
        assert.ok(error instanceof NativeProcessError);
        assert.equal(error.code, 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER'); assert.equal(error.timedOut, false);
        assert.match(error.message, /maxBuffer/);
        return true;
    });
});

test('native writer queue isolates data directories and collections', async () => {
    const queue = new NativeWriteQueue();
    const spec: LaunchSpec = { command: '/fixture/airk', root: '/fixture/project', dataDir: '/fixture/state', collection: 'project', name: 'Fixture' };
    let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; });
    const calls: string[] = [];
    const first = queue.run(spec, async () => { calls.push('first'); await held; });
    const same = queue.run({ ...spec, dataDir: path.join(spec.dataDir, '.') }, async () => { calls.push('same'); });
    try {
        await Promise.all([
            queue.run({ ...spec, dataDir: '/fixture/other-storage' }, async () => { calls.push('storage'); }),
            queue.run({ ...spec, collection: 'other-project' }, async () => { calls.push('collection'); })
        ]);
        assert.deepEqual(calls, ['first', 'storage', 'collection']);
    } finally { release(); await Promise.all([first, same]); }
    assert.equal(calls.at(-1), 'same', 'The same native writer must wait even through a path spelling alias');
});

test('native semantic work stays independent of helper stop and resumes through durable pause-aware jobs', {skip:!process.env.AIRK_TEST_BINARY,timeout:360000}, async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    try{
        const spec=await fixture.register('semantic'),model=path.join(fixture.owner,'model');await fs.mkdir(model);
        const vocabulary=JSON.stringify({authenticate:[1,0],login:[1,0]});
        await fs.writeFile(path.join(model,'vocabulary.json'),vocabulary);
        await fs.writeFile(path.join(model,'model.json'),JSON.stringify({schema:1,name:'original-native-semantic-fixture',license:'Apache-2.0',dimensions:2,
            sha256:createHash('sha256').update(vocabulary).digest('hex')}));
        await fixture.configure(spec,{operation:'install_model',directory:model});
        const embedding=await fixture.ensureEmbedding();
        assert.equal(embedding.available,true);
        await fixture.client!.rpc({stop_embedding:{worker:embedding.worker,force:false}},'embedding_stopped');
        for(let file=0;file<65;file++)await fs.writeFile(path.join(spec.root,`file_${String(file).padStart(2,'0')}.txt`),'authenticate\n');
        await fixture.job(spec);
        const lexical=await fixture.tool(spec,'airk_search',{query:'authenticate',mode:'lexical'});
        assert.ok(Array.isArray(lexical.hits)&&lexical.hits.length>0);
        const paused=await fixture.client!.rpc({pause:{project:spec.service!.project,paused:true}});assert.ok(paused);
        await fixture.ensureEmbedding();
        const accepted=await fixture.client!.submitJob(await fixture.client!.prepareJob(spec.service!.project,'vectors'));
        const queued=record(accepted.job);assert.equal(queued.state,'pending');
        await fixture.client!.rpc({pause:{project:spec.service!.project,paused:false}});
        assert.equal((await fixture.waitJob(String(queued.id))).state,'succeeded');
        const ready=await fixture.ready(spec,65);assert.equal(ready.indexed_vectors,1);
        for(let file=0;file<65;file++)assert.equal(await fs.readFile(path.join(spec.root,`file_${String(file).padStart(2,'0')}.txt`),'utf8'),'authenticate\n');
        await assert.rejects(runJson(spec,'schedule',['run','--watch']));
    }finally{await fixture.dispose();}
});

// Both inputs must be complete, independently identified native runtime pairs.
// The old standalone helper/independent-namespace scenario predates the service.
const previousHelperBinary = process.env.AIRK_TEST_PREVIOUS_BINARY || process.env.AIRK_TEST_BINARY;
const upgradedHelperBinary = process.env.AIRK_TEST_PREVIOUS_BINARY ? process.env.AIRK_TEST_BINARY : process.env.AIRK_TEST_UPGRADE_BINARY;
async function payloadDigest(filename:string):Promise<string>{
    const digest=createHash('sha256');
    for await(const bytes of createReadStream(filename))digest.update(bytes);
    return digest.digest('hex');
}

test('native pre-cutover upgrade requires fresh registration and indexing while preserving sources',
    { skip: !previousHelperBinary || !upgradedHelperBinary }, async () => {
    const oldBinary=path.resolve(previousHelperBinary!),newBinary=path.resolve(upgradedHelperBinary!);
    assert.match((await runText(oldBinary,['--version'])).trim(),/^airk 1\.4\.4$/,
        'Supply the complete qualified 1.4.4 native pair as the pre-cutover fixture');
    const oldDigest=await payloadDigest(oldBinary),newDigest=await payloadDigest(newBinary);
    assert.notEqual(oldDigest,newDigest);
    const fixture=await NativeServiceFixture.create(oldBinary,'1.4.4');
    try{
        const oldSpec=await fixture.register('cutover');
        const quota=path.join(fixture.owner,'cutover-quota.json');
        await fs.writeFile(quota,JSON.stringify({root_bytes:512*1024**2,default_collection_bytes:128*1024**2,collections:{}}));
        await fixture.configure(oldSpec,{operation:'quota_policy',path:quota});
        await fixture.configure(oldSpec,{operation:'vectors',enabled:false});
        await fixture.configure(oldSpec,{operation:'usage',mode:'disabled'});
        const source=path.join(oldSpec.root,'fixture.rs'),text='pub fn upgrade_fixture_marker() {}\n';
        await fs.writeFile(source,text);
        const unrelated=path.join(oldSpec.dataDir,'unrelated-note.txt');
        await fs.writeFile(unrelated,'Preserve unrelated fixture bytes.');
        await fixture.job(oldSpec);
        const oldStatus=await fixture.publishedStatus(oldSpec),oldToken=record(oldStatus.publication).token;
        assert.equal(oldStatus.files,1);
        const stale=await fixture.client!.prepareJob(oldSpec.service!.project,'index');
        const oldControl=fixture.control;
        await fixture.upgrade(newBinary);
        assert.equal(fixture.control,oldControl,'The private installation/control location stays stable');
        const epoch=await fixture.client!.rpc('request_epoch','request_epoch');
        assert.notEqual(epoch.namespace,stale.key.namespace,'The request namespace must be fresh');
        const spec=await fixture.register('cutover',oldSpec.root);
        assert.equal(spec.dataDir,oldSpec.dataDir,'Preserve the selected data location');
        const refused=await fixture.client!.transport.request({enqueue:stale});
        assert.equal(refused.status,'failure');
        assert.equal(refused.code,'KeyExpired','Reject the old request key explicitly');
        await fixture.configure(spec,{operation:'vectors',enabled:false});
        await fixture.job(spec);
        const status=await fixture.publishedStatus(spec);
        assert.equal(status.files,1);assert.notEqual(record(status.publication).token,oldToken);
        assert.equal(record(status.storage_sizes).current_index_file,'current.airk');
        const found=await fixture.tool(spec,'airk_search',{query:'upgrade_fixture_marker',mode:'lexical'});
        assert.ok(Array.isArray(found.hits)&&found.hits.some(value=>record(record(value).citation).path==='fixture.rs'));
        async function refuseOldFiles(directory:string):Promise<void>{
            for(const entry of await fs.readdir(directory,{withFileTypes:true})){
                assert.ok(!entry.isSymbolicLink(),'Unexpected alias in owned fixture data');
                if(entry.isDirectory())await refuseOldFiles(path.join(directory,entry.name));
                else assert.ok(!/\.sqlite3(?:-(?:wal|shm|journal))?$/.test(entry.name),'Old database file survived fresh setup');
            }
        }
        await refuseOldFiles(spec.dataDir);
        await assert.rejects(fs.stat(path.join(fixture.control,'service-1.4.sqlite3')),{code:'ENOENT'});
        assert.equal(await fs.readFile(source,'utf8'),text);
        assert.equal(await fs.readFile(unrelated,'utf8'),'Preserve unrelated fixture bytes.');
        assert.equal(await payloadDigest(oldBinary),oldDigest,'The supplied historical runtime remains unchanged');
        assert.equal(await payloadDigest(newBinary),newDigest);
    }finally{await fixture.dispose();}
});

test('bundled helper rejects mismatched platforms and tampered bytes before execution', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-helper-test-'));
    try {
        const native = path.join(directory, 'native'); await fs.mkdir(native);
        const executable = process.platform === 'win32' ? 'airk.exe' : 'airk';
        await fs.writeFile(path.join(native, executable), 'fictional fixture');
        const manifest = { version: '0.1.0', vsix_target: 'wrong-platform', payload: { [executable]: '0'.repeat(64) } };
        await fs.writeFile(path.join(native, 'manifest.json'), JSON.stringify(manifest));
        await assert.rejects(prepareHelper(directory, path.join(directory, 'cache'), '0.1.0'), /does not match/);
        manifest.vsix_target = `${process.platform}-${process.arch}`;
        await fs.writeFile(path.join(native, 'manifest.json'), JSON.stringify(manifest));
        await assert.rejects(prepareHelper(directory, path.join(directory, 'cache'), '0.1.0'), /digest mismatch/);
    } finally { await removeFixture(directory, 'airk-helper-test-'); }
});

// CI/local build script supplies an absolute native binary; ordinary unit runs skip this boundary.
test('extension native clients index, inspect, approve rebuilds, enforce limits and preserve isolated sources', {skip:!process.env.AIRK_TEST_BINARY,timeout:480000}, async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    try{
        const a=await fixture.register('project-a'),b=await fixture.register('project-b');
        await fs.writeFile(path.join(a.root,'a.py'),'def authenticate_fixture():\n    return True\n');
        await fs.writeFile(path.join(b.root,'b.py'),'def second_fixture():\n    return False\n');
        assert.equal((await fixture.settings(a,'extraction_policy')).configured_detail,'agent_lean');
        await fixture.configure(a,{operation:'vectors',enabled:false});
        await fixture.configure(b,{operation:'vectors',enabled:false});
        await fixture.job(a);await fixture.job(b);
        const initial=await fixture.publishedStatus(a);assert.equal(initial.files,1);
        await fixture.job(a);
        assert.deepEqual((await fixture.publishedStatus(a)).publication,initial.publication);
        const published=await fixture.settings(a,'extraction_policy');
        assert.equal(published.configured_detail,'agent_lean');assert.equal(published.published_detail,'agent_lean');
        const limits=await fixture.settings(a,'scan_limits'),effective=record(limits.effective);
        assert.equal(limits.mode,'automatic');assert.ok(Number(effective.corpus_bytes)<=4294967296);
        const selected=path.join(fixture.owner,'limits.json');await fs.writeFile(selected,JSON.stringify({...effective,corpus_bytes:1}));
        await fixture.configure(a,{operation:'scan_limits',path:selected});
        // Failed scans publish readiness metadata while preserving retained
        // artifacts. Check source changes and evidence independently of that marker.
        const limited=await fixture.publishedStatus(a);
        const retainedBefore=await fixture.tool(a,'airk_read_file',{path:'a.py',start_line:1,end_line:2});
        const accepted=await fixture.client!.submitJob(await fixture.client!.prepareJob(a.service!.project,'index'));
        assert.equal((await fixture.waitJob(String(record(accepted.job).id))).state,'failed');
        const failed=await fixture.publishedStatus(a);
        assert.equal(record(failed.publication).incarnation,record(limited.publication).incarnation);
        assert.equal(record(failed.discovery_readiness).state,'failed');
        const unchanged=await fixture.tool(a,'airk_changes',{since:record(limited.publication).token,limit:100,max_bytes:65536});
        assert.equal(unchanged.baseline_status,'verified');assert.deepEqual(unchanged.changes,[]);assert.equal(unchanged.truncated,false);
        assert.deepEqual((await fixture.tool(a,'airk_read_file',{path:'a.py',start_line:1,end_line:2})).hits,retainedBefore.hits);
        const hits=(await fixture.tool(a,'airk_search',{query:'authenticate',mode:'lexical'})).hits;
        assert.ok(Array.isArray(hits)&&hits.length>0);assert.equal(record(record(hits[0]).citation).path,'a.py');
        await fs.writeFile(selected,JSON.stringify(effective));
        await fixture.configure(a,{operation:'scan_limits',path:selected});await fixture.job(a);
        const before=await fixture.publishedStatus(a);await fixture.repair(a,'verify');
        assert.deepEqual((await fixture.publishedStatus(a)).publication,before.publication);
        await fixture.repair(a,'rebuild');
        const rebuilt=await fixture.publishedStatus(a);
        assert.notEqual(record(rebuilt.publication).incarnation,record(before.publication).incarnation);
        const probe=await probeMcp(a);assert.equal(probe.tools.length,16);assert.ok(probe.tools.includes('airk_changes'));
        const foreign=await fixture.tool(b,'airk_code_facts',{name:'authenticate_fixture',compact:false});
        assert.deepEqual(foreign.items,[]);
        await assert.rejects(runJson(a,'collections',['remove',b.collection]));
        assert.equal((await fixture.publishedStatus(b)).files,1);
        assert.equal(await fs.readFile(path.join(a.root,'a.py'),'utf8'),'def authenticate_fixture():\n    return True\n');
        assert.equal(await fs.readFile(path.join(b.root,'b.py'),'utf8'),'def second_fixture():\n    return False\n');
        const usage=record(JSON.parse(await runText(a.command,[...launchArgs(a,'usage'),'report'],path.dirname(a.command),120000)));
        assert.ok(Number(record(usage.report).calls)>0);
        assert.equal(record((await fixture.run(['runtime','inspect','--install-dir',fixture.install])).managed_policy).lifecycle,'user_on_demand');
    }finally{await fixture.dispose();}
});

test('native queue waits resume the same request and reject untyped or invalid continuations', async () => {
    const pending = { airk_error: 1, code: 'scheduled_index_pending', retryable: true, requested_revision: 7, message: 'still queued' };
    const failure = (value: unknown, properties: object = { code: 1 }) => new NativeProcessError('airk',
        Object.assign(new Error('Command failed'), properties), JSON.stringify(value), 240000, 3600000);
    const calls: string[][] = []; let waits = 0;
    const result = await joinScheduledIndex(['--scheduled', '--progress'], async (command, args) => {
        calls.push([command, ...args]);
        if (calls.length < 3) { throw failure(pending); }
        return { generation: 9 };
    }, () => { waits++; });
    assert.equal(result.generation, 9); assert.equal(waits, 2);
    assert.deepEqual(calls, [['index', '--scheduled', '--progress'], ['schedule', 'wait', '7', '--progress'], ['schedule', 'wait', '7', '--progress']]);
    for (const value of [{ ...pending, requested_revision: -1 }, { ...pending, requested_revision: Number.MAX_SAFE_INTEGER + 1 },
        { ...pending, retryable: false }, { ...pending, code: 'operation_failed' }]) {
        assert.equal(failure(value).pendingIndexRevision, undefined);
    }
    assert.equal(failure(pending, { code: 1, killed: true }).pendingIndexRevision, undefined);
    assert.equal(failure(pending, { code: 1, signal: 'SIGTERM' }).pendingIndexRevision, undefined);
    await assert.rejects(joinScheduledIndex([], async () => { throw failure(pending); }, () => { throw new Error('Workspace closed'); }), /Workspace closed/);
    let attempt = 0;
    await assert.rejects(joinScheduledIndex([], async () => { throw failure({ ...pending, requested_revision: ++attempt }); }, () => {}), /different request revision/);
    await assert.rejects(joinScheduledIndex([], async () => { throw new Error('permission denied'); }, () => { assert.fail(); }), /permission denied/);
});


test('queue joins yield after a bounded number and Pause prevents another join', async () => {
    const pending = new NativeProcessError('airk', Object.assign(new Error('pending'), { code: 1 }),
        'usage warning\n' + JSON.stringify({ airk_error: 1, code: 'scheduled_index_pending', retryable: true, requested_revision: 7, message: 'queued' }), 240000, 3600000);
    let calls = 0;
    await assert.rejects(joinScheduledIndex([], async () => { calls++; throw pending; }, () => {}), error => {
        assert.ok(error instanceof ScheduledIndexStillQueued); assert.equal(error.revision, 7); return true;
    });
    assert.equal(calls, 3); calls = 0;
    await assert.rejects(joinScheduledIndex([], async (command, args) => {
        calls++; assert.equal(command, 'schedule'); assert.equal(args[1], '7'); throw pending;
    }, () => false, 7), error => {
        assert.ok(error instanceof ScheduledIndexStillQueued); assert.equal(error.paused, true); return true;
    });
    assert.equal(calls, 1);
});


test('bounded native control stdin is separate from process arguments and closes at EOF',async()=>{
    const input=JSON.stringify({fixture:'copper beacon'});
    const output=await runText(process.execPath,['-e','let text="";process.stdin.on("data",bytes=>text+=bytes);process.stdin.on("end",()=>process.stdout.write(text));'],undefined,5000,undefined,0,input);
    assert.equal(output,input);
    await assert.rejects(runText(process.execPath,['-e','process.exit(7)'],undefined,5000,undefined,0,'é'.repeat(32769)),/64 KiB/);
});
