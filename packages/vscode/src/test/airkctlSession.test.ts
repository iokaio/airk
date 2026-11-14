// SPDX-License-Identifier: Apache-2.0
// Session protocol 2 client (contracts §3.7–§3.8) against the reviewed frames in
// fixtures/wire/airkctl_session_frames.json, plus client behavior over an
// injected child and clock: hello identity, invoke, busy retry, deadline
// cancel, events and the one-shot-only commands.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { PassThrough } from 'node:stream';
import { EventEmitter } from 'node:events';
import {
    AirkctlSession, decodeFrame, encodeRequest, FrameReader, MAX_IN_FLIGHT, MAX_INVOKE_ARG_BYTES, MAX_INVOKE_ARGS, MAX_INVOKE_DEADLINE_MS,
    MAX_INVOKE_STDIN_BYTES, MAX_REQUEST_FRAME_BYTES, MAX_RESPONSE_FRAME_BYTES, requestFrameValid, SESSION_PROTOCOL, SessionError, type ClientEvent, type SessionChild,
} from '../airkctl/session';
import { sessionRefuses } from '../airkctl';
import { exitClass, parseErrorLine, splitErrorLine } from '../airkctl/runner';
import { WindowAssistance } from '../airkctl/features/windowAssistance';
import { AirkctlToolSession, selectionArguments } from '../airkctl/tools';

const repo = path.resolve(__dirname, '../../../..');
type Case = { name: string; json: Record<string, any> };
const frames = JSON.parse(fs.readFileSync(path.join(repo, 'fixtures/wire/airkctl_session_frames.json'), 'utf8')) as
    { schema: string; protocol: number; budgets: Record<string, number>; requests: Case[]; invalid_requests: Case[]; responses: Case[]; events: Case[]; invalid_events: Case[] };
const cases = (key: 'requests' | 'invalid_requests' | 'responses' | 'events' | 'invalid_events') => { assert.ok(frames[key].length > 0, `${key}: zero cases is a failure`); return frames[key]; };

test('session fixture is the reviewed schema; every budget equals the client constant', () => {
    assert.equal(frames.schema, 'airk.wire-fixture.airkctl-session-frames/1');
    assert.equal(frames.protocol, SESSION_PROTOCOL);
    assert.deepEqual({
        max_request_frame_bytes: frames.budgets.max_request_frame_bytes, max_invoke_stdin_bytes: frames.budgets.max_invoke_stdin_bytes,
        max_invoke_args: frames.budgets.max_invoke_args, max_invoke_arg_bytes: frames.budgets.max_invoke_arg_bytes,
        max_response_frame_bytes: frames.budgets.max_response_frame_bytes, max_in_flight: frames.budgets.max_in_flight, max_invoke_deadline_ms: frames.budgets.max_invoke_deadline_ms,
    }, {
        max_request_frame_bytes: MAX_REQUEST_FRAME_BYTES, max_invoke_stdin_bytes: MAX_INVOKE_STDIN_BYTES, max_invoke_args: MAX_INVOKE_ARGS,
        max_invoke_arg_bytes: MAX_INVOKE_ARG_BYTES, max_response_frame_bytes: MAX_RESPONSE_FRAME_BYTES, max_in_flight: MAX_IN_FLIGHT, max_invoke_deadline_ms: MAX_INVOKE_DEADLINE_MS,
    });
});

test('session requests: every reviewed request is accepted and encodes to exactly its JSON', () => {
    for (const item of cases('requests')) {
        assert.ok(requestFrameValid(item.json), item.name);
        const frame = encodeRequest(item.json.id, item.json.request);
        assert.equal(frame.readUInt32BE(0), frame.length - 4, item.name);
        assert.deepEqual(JSON.parse(frame.subarray(4).toString('utf8')), item.json, item.name);
    }
});
test('session requests: every reviewed invalid request is refused before sending', () => {
    for (const item of cases('invalid_requests')) {
        assert.equal(requestFrameValid(item.json), false, item.name);
        if (typeof item.json.id === 'string' && item.json.request) {
            if (Object.keys(item.json).length === 2) { assert.throws(() => encodeRequest(item.json.id, item.json.request), SessionError, item.name); }
        }
    }
});
test('session replies and events: reviewed frames decode; invalid events are refused', () => {
    for (const item of cases('responses')) { assert.equal(decodeFrame(item.json).kind, 'reply', item.name); }
    for (const item of cases('events')) { assert.equal(decodeFrame(item.json).kind, 'event', item.name); }
    for (const item of cases('invalid_events')) { assert.throws(() => decodeFrame(item.json), SessionError, item.name); }
    assert.throws(() => decodeFrame({ id: '01'.repeat(16), outcome: { status: 'mystery' } }), SessionError);
});
test('token-returning and pipe-only commands are one-shots, never session requests (§3.8)', () => {
    for (const argv of [['agents', 'credential', '--connection-id', 'x'], ['admin', 'url'], ['admin', 'open'], ['service', 'uninstall'], ['tools', 'shell'], ['service', 'rpc']]) {
        assert.equal(sessionRefuses(argv), true, argv.join(' '));
    }
    assert.equal(sessionRefuses(['tools', 'call', 'airk_search']), false);
    assert.equal(sessionRefuses(['agents', 'list']), false);
});
test('exit classes and error lines follow the reviewed I2 fixtures', () => {
    const codes = JSON.parse(fs.readFileSync(path.join(repo, 'fixtures/split/i2/exit_codes.json'), 'utf8')) as { codes: { code: number; class: string }[]; unassigned: number[] };
    for (const item of codes.codes) { assert.equal(exitClass(item.code), item.class); }
    for (const code of codes.unassigned) { assert.equal(exitClass(code), 'internal'); }
    const lines = JSON.parse(fs.readFileSync(path.join(repo, 'fixtures/split/i2/error_lines.json'), 'utf8')) as Record<'valid' | 'invalid' | 'rejected_after_decoding', Case[]>;
    for (const item of lines.valid) { assert.ok(parseErrorLine(item.json), item.name); }
    for (const item of [...lines.invalid, ...lines.rejected_after_decoding]) { assert.equal(parseErrorLine(item.json), undefined, item.name); }
    const line = JSON.stringify(lines.valid[1].json);
    assert.deepEqual(splitErrorLine(`warning\n${line}\n`), { stderr: 'warning\n', error: lines.valid[1].json });
    assert.deepEqual(splitErrorLine('plain failure\n'), { stderr: 'plain failure\n' });
});

// ---- Client behavior over a fake child ------------------------------------------

class FakeChild extends EventEmitter implements SessionChild {
    readonly stdin = new PassThrough(); readonly stdout = new PassThrough(); readonly stderr = new PassThrough();
    exitCode: number | null = null; signalCode: NodeJS.Signals | null = null;
    readonly requests: Record<string, any>[] = [];
    private readonly reader = new FrameReader(MAX_REQUEST_FRAME_BYTES);
    constructor(private readonly answer: (request: Record<string, any>, child: FakeChild) => void) {
        super();
        this.stdin.on('data', (bytes: Buffer) => { for (const frame of this.reader.push(bytes)) { const value = JSON.parse(frame.toString('utf8')); this.requests.push(value); this.answer(value, this); } });
        this.stdin.on('finish', () => { this.exitCode = 0; this.emit('exit'); });
    }
    send(value: unknown): void { const body = Buffer.from(JSON.stringify(value)); const head = Buffer.alloc(4); head.writeUInt32BE(body.length); this.stdout.write(Buffer.concat([head, body])); }
    kill(): boolean { this.signalCode = 'SIGTERM'; this.emit('exit'); return true; }
}
const IMAGE = { path: path.resolve('/opt/airk/bin/airkctl'), sha256: 'ab'.repeat(32), version: '1.9.0' };
const hello = (request: Record<string, any>, overrides: Record<string, unknown> = {}) => ({ id: request.id, outcome: {
    status: 'hello', protocol: 2, version: '1.9.0', i2_contract: 1, capabilities: ['session/2', 'tools/1'], executable_sha256: IMAGE.sha256,
    service_instance: '0a'.repeat(16), window_id: '0b'.repeat(16), ...overrides } });

test('hello names the client and contracts; a different identity is refused and never restarted', async () => {
    let launched = 0;
    const session = new AirkctlSession({ clientVersion: '1.9.0', launch: () => { launched++; return new FakeChild((request, child) => {
        if (request.request.hello) { child.send(hello(request, { executable_sha256: 'cd'.repeat(32) })); }
    }); } });
    session.bind(IMAGE);
    await assert.rejects(session.invoke(['projects']), (error: SessionError) => error.code === 'session_identity_mismatch');
    await assert.rejects(session.invoke(['projects']), (error: SessionError) => error.code === 'session_identity_mismatch');
    assert.equal(launched, 1, 'an image that answered another identity is not started again');
    session.dispose();
});

test('invoke returns the complete three channels; events reach listeners; busy is retried', async () => {
    let busy = 1;
    const children: FakeChild[] = [];
    const session = new AirkctlSession({ clientVersion: '1.9.0', launch: () => { const child = new FakeChild((request, me) => {
        if (request.request.hello) {
            assert.deepEqual(request.request.hello, { protocol: 2, client: { name: 'airk-vscode', version: '1.9.0' }, i2_contracts: [1] });
            me.send(hello(request)); return;
        }
        if (request.request.invoke) {
            if (busy-- > 0) { me.send({ id: request.id, outcome: { status: 'failed', code: 'session_busy', retryable: true } }); return; }
            me.send({ event: { kind: 'resync', reason: 'overflow' } });
            me.send({ id: request.id, outcome: { status: 'invoked', exit_code: 1, class: 'refused', stdout: '{"status":"failure"}', stderr: 'x\n',
                error: { airkctl_error: 1, class: 'refused', code: 'source_scope_changed', retryable: false, message: 'm', native: { status: 'failure' } } } });
        }
    }); children.push(child); return child; } });
    session.bind(IMAGE);
    const events: ClientEvent[] = [];
    session.onEvent(event => events.push(event));
    const result = await session.invoke(['tools', 'call', 'airk_search', '--args', '{}', '--output', 'json'], { deadlineMs: 5000 });
    assert.deepEqual(result, { exitCode: 1, class: 'refused', stdout: '{"status":"failure"}', stderr: 'x\n',
        error: { airkctl_error: 1, class: 'refused', code: 'source_scope_changed', retryable: false, message: 'm', native: { status: 'failure' } } });
    assert.deepEqual(events, [{ kind: 'resync', reason: 'overflow' }]);
    assert.equal(session.hello?.windowId, '0b'.repeat(16));
    const invokes = children[0].requests.filter(request => request.request.invoke);
    assert.equal(invokes.length, 2, 'session_busy alone is retried');
    assert.ok(invokes.every(request => request.request.invoke.deadline_ms <= 5000));
    session.dispose();
});

test('a deadline answers at once, sends cancel and keeps no reply for the caller', async () => {
    const children: FakeChild[] = [];
    const session = new AirkctlSession({ clientVersion: '1.9.0', launch: () => { const child = new FakeChild((request, me) => {
        if (request.request.hello) { me.send(hello(request)); return; }
        if (request.request.cancel) { me.send({ id: request.id, outcome: { status: 'cancelled' } }); }
    }); children.push(child); return child; } });
    session.bind(IMAGE);
    await assert.rejects(session.invoke(['search', '--query', 'q'], { deadlineMs: 20 }), (error: SessionError) => error.code === 'session_deadline_exceeded' && error.sent);
    await new Promise(resolve => setImmediate(resolve));
    const target = children[0].requests.find(request => request.request.invoke)!.id;
    assert.ok(children[0].requests.some(request => request.request.cancel?.target === target));
    session.dispose();
});

test('evidence reads become `tools call` with the reviewed selection named explicitly', async () => {
    assert.deepEqual(selectionArguments({ project: 'ab'.repeat(32) }), ['--service-project', 'ab'.repeat(32)]);
    assert.deepEqual(selectionArguments({ project: 'p', catalogSession: { kind: 'scoped' } }), ['--catalog-session', '{"kind":"scoped"}']);
    const catalogue={kind:'catalog'},combined={kind:'combined'};
    const binding=new AirkctlToolSession({closed:false} as never,{project:'p',catalogSession:catalogue,combinedSession:combined});
    assert.deepEqual(binding.readScope(),combined,'retry capture follows the same combined-before-catalog selection precedence as dispatch');
    assert.deepEqual(selectionArguments({catalogSession:catalogue,combinedSession:combined}),['--combined-session',JSON.stringify(combined)]);

    const seen: string[][] = [];
    const fakeSession = { closed: false, invoke: async (argv: string[]) => { seen.push(argv); return { exitCode: 0, class: 'success' as const, stdout: '{"content":[{"type":"text","text":"{}"}]}', stderr: '' }; } };
    const tools = new AirkctlToolSession(fakeSession as never, { project: 'ab'.repeat(32) });
    assert.deepEqual(await tools.request('tools/call', { name: 'airk_status', arguments: { detail: 'scalar' } }, 1000), { content: [{ type: 'text', text: '{}' }] });
    assert.deepEqual(seen[0], ['tools', 'call', 'airk_status', '--args', '{"detail":"scalar"}', '--service-project', 'ab'.repeat(32), '--output', 'json']);
    await assert.rejects(tools.request('tools/call', { name: 'airk_search', arguments: { query: 'x'.repeat(MAX_INVOKE_ARG_BYTES) } }, 1000), /32 KiB/);
});


test('retiring a verified session child resets event subscription and discards its later events', async () => {
    const children:FakeChild[]=[],seen:ClientEvent[]=[];
    let image=IMAGE;
    const session=new AirkctlSession({clientVersion:'1.9.0',launch:()=>{
        const expected=image;
        const child=new FakeChild((request,me)=>{
            if(request.request.hello){me.send(hello(request,{executable_sha256:expected.sha256,capabilities:['session/2','assistance/1']}));}
            else if(request.request.subscribe){me.send({id:request.id,outcome:{status:'subscribed',cursor:{}}});}
            else if(request.request.invoke){me.send({id:request.id,outcome:{status:'invoked',exit_code:0,class:'success',stdout:'{}',stderr:''}});}
        });children.push(child);return child;
    }});
    const assistance=new WindowAssistance(session,()=>session.hello?.capabilities,{submit:()=>({ok:false,code:'fixture',message:'fixture'})} as never);
    session.onEvent(event=>{seen.push(event);void assistance.event(event);});
    try{
        session.bind(image);await session.invoke(['status']);await assistance.start();
        assert.equal(children[0].requests.filter(request=>request.request.subscribe).length,1);
        image={...IMAGE,sha256:'cd'.repeat(32)};session.bind(image);
        assert.deepEqual(seen,[{kind:'session_lost',code:'session_image_changed'}]);
        await session.invoke(['status']);await assistance.start();
        assert.equal(children[1].requests.filter(request=>request.request.subscribe).length,1,'replacement must receive its own subscription');
        children[0].send({event:{kind:'resync',reason:'overflow'}});
        assert.equal(seen.length,1,'detached stdout cannot publish stale events');
        session.unbind();assert.equal(seen.length,2,'unbind retires the generation once');
    }finally{session.dispose();}
});


test('queued Invoke and Admin deadlines floor fractional time without a minimum millisecond or original send after expiry', async () => {
    for (const kind of ['invoke','admin'] as const) for (const [observed,expected] of [[8.4,1],[9.6,0],[10,0]] as const) {
        let now=0;let child!:FakeChild;
        const held:Record<string,any>[]=[];
        const invoked=(request:Record<string,any>)=>({id:request.id,outcome:{status:'invoked',exit_code:0,class:'success',stdout:'{}',stderr:''}});
        const session=new AirkctlSession({clientVersion:'1.9.0',timers:{now:()=>now,set:callback=>callback,clear:()=>{}},
            launch:()=>child=new FakeChild((request,me)=>{
                if(request.request.hello){me.send(hello(request,{capabilities:['session/2','tools/1','admin-management/1']}));return;}
                if(request.request.invoke?.argv[0]==='hold'){held.push(request);return;}
                if(request.request.invoke){me.send(invoked(request));return;}
                if(request.request.admin_call){me.send({id:request.id,outcome:{status:'admin_chunk',chunk:{http_status:200,total_bytes:2,offset:0,data:[123,125],done:true}}});}
            })});
        const blockers:Promise<unknown>[]=[];
        try {
            session.bind(IMAGE);await session.invoke(['version']);
            for(let n=0;n<MAX_IN_FLIGHT;n++){const blocked=session.invoke(['hold'],{deadlineMs:1000});blockers.push(blocked);void blocked.catch(()=>{});}
            assert.equal(held.length,MAX_IN_FLIGHT);
            const pending=kind==='invoke'?session.invoke(['candidate'],{deadlineMs:10}):session.adminCall(session.captureAdmin(),'usage.document',{}, {deadlineMs:10});
            void pending.catch(()=>{});
            now=observed;child.send(invoked(held[0]));
            if(expected===0){await assert.rejects(pending,(error:SessionError)=>error.code==='session_deadline_exceeded'&&!error.sent);}
            else {await pending;}
            const actual=child.requests.filter(request=>kind==='invoke'?request.request.invoke?.argv[0]==='candidate':!!request.request.admin_call);
            assert.equal(actual.length,expected===0?0:1,kind+' at '+observed);
            if(expected>0){assert.equal((kind==='invoke'?actual[0].request.invoke:actual[0].request.admin_call).deadline_ms,expected);}
        } finally {session.dispose();await Promise.allSettled(blockers);}
    }
});
