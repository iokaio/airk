// SPDX-License-Identifier: Apache-2.0
// Lifetime and routing of the one per-window `airk service session` child,
// against an in-memory child (no native binary) and an injected clock.
import { strict as assert } from 'node:assert';
import { test, type TestContext } from 'node:test';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { NativeSession, NativeSessionError, SessionControlTransport, SessionFrameReader, closeWindowSession, nativeText, openWindowSession,
    windowNativeSession, type SessionChild, type SessionImage, type SessionOutcome, type SessionTimers } from '../nativeSession';
import * as native from '../native';
import { NativeProcessError } from '../native';
import { RuntimeHost, type RuntimeBinding } from '../serviceBootstrap';
import { ServiceError, type ControlTransport, type RecordValue } from '../serviceClient';

const IMAGE: SessionImage = { path: path.resolve('fixture-bundle', 'airk'), sha256: '11'.repeat(32), version: '1.4.0' };
type Request = { id: string; request: Record<string, any> };
type Handler = (request: Request, child: FakeChild) => SessionOutcome | undefined;

class ManualTimers implements SessionTimers {
    time = 0;
    private next = 1;
    private readonly pending = new Map<number, { at: number; callback: () => void }>();
    private readonly watchers: { ms: number; resolve(): void }[] = [];
    set(callback: () => void, ms: number): unknown {
        const id = this.next++; this.pending.set(id, { at: this.time + ms, callback });
        for (const watcher of this.watchers.filter(item => item.ms === ms)) { this.watchers.splice(this.watchers.indexOf(watcher), 1); watcher.resolve(); }
        return id;
    }
    clear(handle: unknown): void { this.pending.delete(handle as number); }
    now(): number { return this.time; }
    delays(): number[] { return [...this.pending.values()].map(timer => timer.at - this.time).sort((a, b) => a - b); }
    /** Resolves when a timer with exactly this delay is scheduled (an observed event, not a sleep). */
    scheduled(ms: number): Promise<void> {
        if ([...this.pending.values()].some(timer => timer.at - this.time === ms)) { return Promise.resolve(); }
        return new Promise(resolve => this.watchers.push({ ms, resolve }));
    }
    advance(ms: number): void {
        const until = this.time + ms;
        for (;;) {
            const due = [...this.pending.entries()].filter(([, timer]) => timer.at <= until).sort((a, b) => a[1].at - b[1].at)[0];
            if (!due) { break; }
            this.pending.delete(due[0]); this.time = due[1].at; due[1].callback();
        }
        this.time = until;
    }
}

class FakeChild extends EventEmitter implements SessionChild {
    readonly stdin = new PassThrough();
    readonly stdout = new PassThrough();
    readonly stderr = new PassThrough();
    exitCode: number | null = null;
    signalCode: NodeJS.Signals | null = null;
    kills = 0;
    eof = false;
    readonly requests: Request[] = [];
    private readonly reader = new SessionFrameReader(192 * 1024);
    private readonly waiters: { count: number; resolve(): void }[] = [];
    constructor(private readonly handler: Handler, private readonly exitOnEof: boolean) {
        super();
        this.stdin.on('data', (bytes: Buffer) => {
            for (const frame of this.reader.push(bytes)) {
                const request = JSON.parse(frame.toString('utf8')) as Request;
                this.requests.push(request);
                const outcome = this.handler(request, this);
                if (outcome) { this.reply(request.id, outcome); }
                for (const waiter of this.waiters.filter(item => this.requests.length >= item.count)) { this.waiters.splice(this.waiters.indexOf(waiter), 1); waiter.resolve(); }
            }
        });
        this.stdin.on('finish', () => { this.eof = true; this.emit('eof'); if (this.exitOnEof) { this.exit(0); } });
    }
    received(count: number): Promise<void> {
        if (this.requests.length >= count) { return Promise.resolve(); }
        return new Promise(resolve => this.waiters.push({ count, resolve }));
    }
    reply(id: string, outcome: SessionOutcome): void {
        const body = Buffer.from(JSON.stringify({ id, outcome }), 'utf8'), prefix = Buffer.alloc(4); prefix.writeUInt32BE(body.length);
        // Split across chunks: identity must survive partial headers and bodies.
        this.stdout.write(prefix.subarray(0, 2)); this.stdout.write(Buffer.concat([prefix.subarray(2), body]));
    }
    exit(code: number | null, signal: NodeJS.Signals | null = null): void {
        if (this.exitCode !== null || this.signalCode !== null) { return; }
        this.exitCode = code; this.signalCode = signal; setImmediate(() => this.emit('exit', code, signal));
    }
    kill(): boolean { this.kills++; this.exit(null, 'SIGTERM'); return true; }
    invokes(): Request[] { return this.requests.filter(item => 'invoke' in item.request); }
}

const hello = (image = IMAGE): SessionOutcome => ({ status: 'hello', protocol: 1, version: image.version, executable_sha256: image.sha256 });
function harness(handler: Handler = () => undefined, options: { exitOnEof?: boolean; image?: SessionImage; identity?: SessionImage } = {}) {
    const timers = new ManualTimers(), children: FakeChild[] = [];
    const session = new NativeSession({ timers, launch: image => {
        assert.deepEqual(image, options.image ?? IMAGE, 'only the verified bundle image is launched');
        const child = new FakeChild((request, current) => 'hello' in request.request ? hello(options.identity ?? image) : handler(request, current), options.exitOnEof ?? true);
        children.push(child); return child;
    } });
    session.offer(options.image ?? IMAGE);
    return { timers, children, session };
}
const invoked = (stdout: unknown): SessionOutcome => ({ status: 'invoked', success: true, stdout: JSON.stringify(stdout), stderr: '' });
const turn = () => new Promise(resolve => setImmediate(resolve));

test('one session child serves many concurrent commands with at most eight in flight', async () => {
    let answering = false;
    const held: string[] = [];
    const { children, session } = harness(request => { if (!answering) { held.push(request.id); return undefined; } return invoked(request.request.invoke.argv); });
    try {
        const work = Array.from({ length: 12 }, (_, index) => session.text(IMAGE.path, ['runtime', 'inspect', String(index)], 60000));
        await children[0].received(1 + 8);
        await turn();
        assert.equal(children[0].invokes().length, 8, 'the ninth request waits for a free slot');
        answering = true;
        for (const id of held) { children[0].reply(id, invoked(children[0].requests.find(item => item.id === id)!.request.invoke.argv)); }
        const results = (await Promise.all(work)).map(text => JSON.parse(text));
        assert.deepEqual(results, Array.from({ length: 12 }, (_, index) => ['runtime', 'inspect', String(index)]));
        assert.equal(session.launches, 1, 'N concurrent commands share one child');
        assert.equal(new Set(children[0].invokes().map(item => item.id)).size, 12, 'every request has its own id');
        assert.equal(children[0].requests[0].request.hello.protocol, 1, 'hello is the first request');
    } finally { session.dispose(); }
});

test('dispose closes stdin, and a child that exits on EOF is never killed', async () => {
    const { timers, children, session } = harness(request => invoked(request.request.invoke.argv));
    await session.text(IMAGE.path, ['runtime', 'location'], 60000);
    const eof = new Promise(resolve => children[0].once('eof', resolve));
    session.dispose();
    await eof; await turn();
    assert.equal(children[0].eof, true); assert.equal(children[0].exitCode, 0);
    timers.advance(60000);
    assert.equal(children[0].kills, 0);
    await assert.rejects(session.text(IMAGE.path, ['runtime', 'location'], 60000), (error: unknown) => error instanceof NativeSessionError && error.code === 'native_session_disposed' && !error.sent);
    assert.equal(session.launches, 1, 'a disposed session never starts another child');
});

test('a child that ignores EOF is killed after the bounded grace', async () => {
    const { timers, children, session } = harness(request => invoked(request.request.invoke.argv), { exitOnEof: false });
    await session.text(IMAGE.path, ['runtime', 'location'], 60000);
    const eof = new Promise(resolve => children[0].once('eof', resolve));
    session.dispose(); await eof;
    timers.advance(4999); assert.equal(children[0].kills, 0);
    timers.advance(1); assert.equal(children[0].kills, 1);
});

test('an unexpected exit fails in-flight work as outcome-unknown and restarts once on next use with backoff', async () => {
    const { timers, children, session } = harness(request => request.request.invoke.argv[0] === 'hold' ? undefined : invoked(request.request.invoke.argv));
    try {
        const held = session.text(IMAGE.path, ['hold'], 60000);
        await children[0].received(2);
        children[0].exit(3);
        await assert.rejects(held, (error: unknown) => error instanceof NativeSessionError && error.code === 'native_session_closed' && error.sent && error.outcomeUnknown);
        assert.equal(session.launches, 1, 'no restart while nothing is waiting');
        const first = session.text(IMAGE.path, ['runtime', 'location'], 60000), second = session.text(IMAGE.path, ['runtime', 'inspect'], 60000);
        await timers.scheduled(500);
        assert.equal(session.launches, 1, 'the restart waits for its backoff');
        timers.advance(500);
        assert.deepEqual((await Promise.all([first, second])).map(text => JSON.parse(text)), [['runtime', 'location'], ['runtime', 'inspect']]);
        assert.equal(session.launches, 2, 'one restart serves all waiting work');
        const again = session.text(IMAGE.path, ['hold'], 60000);
        await children[1].received(4);
        children[1].exit(3);
        await assert.rejects(again, /native_session_closed/);
        const later = session.text(IMAGE.path, ['runtime', 'location'], 60000);
        await timers.scheduled(1000);
        assert.ok(!timers.delays().includes(500), 'a second crash inside the healthy window doubles the backoff');
        timers.advance(1000);
        assert.deepEqual(JSON.parse(await later), ['runtime', 'location']);
        assert.equal(session.launches, 3);
    } finally { session.dispose(); }
});

test('only session_busy is retried, with backoff and a new request id', async () => {
    let attempts = 0;
    const { timers, children, session } = harness(request => {
        if (request.request.invoke.argv[0] === 'refused') { return { status: 'failed', code: 'session_command_refused', retryable: false }; }
        return ++attempts === 1 ? { status: 'failed', code: 'session_busy', retryable: true } : invoked(request.request.invoke.argv);
    });
    try {
        const work = session.text(IMAGE.path, ['runtime', 'inspect'], 60000);
        await timers.scheduled(25);
        timers.advance(25);
        assert.deepEqual(JSON.parse(await work), ['runtime', 'inspect']);
        const invokes = children[0].invokes();
        assert.equal(invokes.length, 2); assert.notEqual(invokes[0].id, invokes[1].id);
        assert.deepEqual(invokes[0].request.invoke.argv, invokes[1].request.invoke.argv);
        await assert.rejects(session.text(IMAGE.path, ['refused'], 60000), (error: unknown) => error instanceof NativeSessionError && error.code === 'session_command_refused' && !error.outcomeUnknown);
        assert.equal(children[0].invokes().length, 3, 'a refusal is never retried');
        assert.equal(session.launches, 1);
    } finally { session.dispose(); }
});

test('a deadline cancels the sent request; its slot is held until the child answers or the grace replaces the child', async () => {
    let answerCancel = true;
    const { timers, children, session } = harness((request, child) => {
        if ('cancel' in request.request) { if (answerCancel) { child.reply(request.request.cancel.target, { status: 'cancelled' }); } return undefined; }
        return request.request.invoke.argv[0] === 'hold' ? undefined : invoked(request.request.invoke.argv);
    });
    try {
        const held = session.text(IMAGE.path, ['hold'], 1000);
        await children[0].received(2);
        assert.equal(children[0].invokes()[0].request.invoke.deadline_ms, 1000, 'the native deadline is the request deadline');
        timers.advance(1000);
        await assert.rejects(held, (error: unknown) => error instanceof NativeProcessError && error.timedOut && error.origin === 'session');
        await children[0].received(3);
        const cancel = children[0].requests[2];
        assert.deepEqual(cancel.request, { cancel: { target: children[0].invokes()[0].id } });
        assert.deepEqual(JSON.parse(await session.text(IMAGE.path, ['runtime', 'location'], 60000)), ['runtime', 'location']);
        answerCancel = false;
        const unanswered = session.text(IMAGE.path, ['hold'], 1000);
        await children[0].received(5);
        timers.advance(1000);
        await assert.rejects(unanswered, /timed out|cancelled/);
        await children[0].received(6);
        assert.ok('cancel' in children[0].requests[5].request);
        await timers.scheduled(30000);
        timers.advance(30000);
        assert.equal(children[0].kills, 1, 'an unanswered cancellation replaces the child');
        const next = session.text(IMAGE.path, ['runtime', 'location'], 60000);
        await timers.scheduled(500); timers.advance(500);
        assert.deepEqual(JSON.parse(await next), ['runtime', 'location']);
        assert.equal(session.launches, 2);
    } finally { session.dispose(); }
});

for (const order of ['target first', 'cancel first'] as const) test(`cancel answers ${order} never end the session or its other work`, async () => {
    const { timers, children, session } = harness((request, child) => {
        if ('cancel' in request.request) {
            const target = request.request.cancel.target;
            if (order === 'target first') { child.reply(target, { status: 'cancelled' }); child.reply(request.id, { status: 'failed', code: 'session_invalid_request', retryable: false }); }
            else { child.reply(request.id, { status: 'cancelled' }); child.reply(target, { status: 'cancelled' }); }
            return undefined;
        }
        return request.request.invoke.argv[0] === 'hold' ? undefined : invoked(request.request.invoke.argv);
    });
    try {
        const installer = session.text(IMAGE.path, ['hold'], 300000), held = session.text(IMAGE.path, ['hold'], 1000);
        await children[0].received(3);
        timers.advance(1000);
        await assert.rejects(held, (error: unknown) => error instanceof NativeProcessError && error.timedOut);
        await children[0].received(4);
        assert.deepEqual(JSON.parse(await session.text(IMAGE.path, ['runtime', 'location'], 60000)), ['runtime', 'location']);
        assert.equal(children[0].kills, 0, 'answers to a known cancel are benign');
        assert.equal(session.launches, 1);
        const installerId = children[0].invokes()[0].id;
        children[0].reply(installerId, invoked(['installer', 'finished']));
        assert.deepEqual(JSON.parse(await installer), ['installer', 'finished'], 'long-running work on the same child is unaffected');
    } finally { session.dispose(); }
});

test('dispose and a new bundle image let started work drain within its own deadline before any kill', async () => {
    const replacement: SessionImage = { ...IMAGE, sha256: '44'.repeat(32) };
    const timers = new ManualTimers(), children: FakeChild[] = [];
    const session = new NativeSession({ timers, launch: image => {
        const child = new FakeChild(request => 'hello' in request.request ? hello(image) : request.request.invoke.argv[0] === 'hold' ? undefined : invoked(request.request.invoke.argv), false);
        children.push(child); return child;
    } });
    session.offer(IMAGE);
    const unanswered = session.text(IMAGE.path, ['hold'], 60000);
    await children[0].received(2);
    const eof = new Promise(resolve => children[0].once('eof', resolve));
    session.offer(replacement);
    await assert.rejects(unanswered, (error: unknown) => error instanceof NativeSessionError && error.code === 'native_session_closed' && error.outcomeUnknown);
    await eof;
    assert.deepEqual(JSON.parse(await session.text(IMAGE.path, ['runtime', 'location'], 60000)), ['runtime', 'location']);
    assert.equal(session.launches, 2, 'the new image serves new work while the old child drains');
    timers.advance(5000);
    assert.equal(children[0].kills, 0, 'started work keeps its own deadline after EOF');
    timers.advance(59999);
    assert.equal(children[0].kills, 0);
    timers.advance(1);
    assert.equal(children[0].kills, 1, 'killed only after its deadline and the bounded grace');

    const held = session.text(IMAGE.path, ['hold'], 60000);
    await children[1].received(3);
    const heldId = children[1].invokes().at(-1)!.id, closed = new Promise(resolve => children[1].once('eof', resolve));
    session.dispose();
    await assert.rejects(held, (error: unknown) => error instanceof NativeSessionError && error.code === 'native_session_disposed' && error.sent);
    await closed;
    timers.advance(1000);
    assert.equal(children[1].kills, 0);
    children[1].reply(heldId, invoked(['drained']));
    await timers.scheduled(5000);
    timers.advance(4999); assert.equal(children[1].kills, 0);
    timers.advance(1); assert.equal(children[1].kills, 1, 'once started work answered, the bounded grace applies');
});

test('hello identity must match the verified bundle image; a mismatched image is never started again', async () => {
    const { timers, children, session } = harness(request => invoked(request.request.invoke.argv), { identity: { ...IMAGE, sha256: '22'.repeat(32) } });
    try {
        await assert.rejects(session.text(IMAGE.path, ['runtime', 'location'], 60000), (error: unknown) => error instanceof NativeSessionError && error.code === 'native_session_identity_mismatch');
        assert.equal(children[0].kills, 1);
        timers.advance(60000);
        await assert.rejects(session.text(IMAGE.path, ['runtime', 'location'], 60000), /native_session_identity_mismatch/);
        assert.equal(session.launches, 1, 'polling a mismatched image creates no further process');
        assert.equal(children[0].invokes().length, 0, 'nothing runs before hello is accepted');
    } finally { session.dispose(); }
});

test('a failed in-session command keeps the native error line semantics', async () => {
    const line = (code: string, retryable: boolean) => JSON.stringify({ airk_error: 1, code, retryable, message: 'fixture message' });
    const { children, session } = harness(request => ({ status: 'invoked', success: false, stdout: '', stderr: `progress\n${line(request.request.invoke.argv[0], true)}\n` }));
    try {
        await assert.rejects(session.text(IMAGE.path, ['operation_failed'], 60000), (error: unknown) => {
            assert.ok(error instanceof NativeProcessError);
            assert.equal(error.nativeMessage, 'fixture message'); assert.equal(error.origin, 'session'); assert.equal(error.timedOut, false);
            assert.equal(error.retryableGateContention, false); assert.match(error.message, /failed in the window session/);
            return true;
        });
        await assert.rejects(session.text(IMAGE.path, ['current_gate_busy'], 60000), (error: unknown) => error instanceof NativeProcessError && error.retryableGateContention);
        await assert.rejects(session.text(IMAGE.path, ['runtime'], 60000, 'x'.repeat(65537)), /exceeds 64 KiB/);
        assert.equal(children[0].invokes().length, 2, 'an oversized input is refused before it is sent');
    } finally { session.dispose(); }
});

const binding = (executable: string): RuntimeBinding => ({ installDir: path.resolve('fixture-install'), controlDir: path.resolve('fixture-control'), executable, launcher: null,
    runtimeSha256: '33'.repeat(32), version: '1.4.0', lifecycle: 'on-demand', verificationBasis: 'trusted_distribution' });

test('status polls against a stopped service go through the one window session and create no process', async (context: TestContext) => {
    const timers = new ManualTimers(), children: FakeChild[] = [];
    const window = openWindowSession({ timers, launch: () => {
        const child = new FakeChild(request => 'hello' in request.request ? hello() : { status: 'control', response: { status: 'failure', code: 'service_unavailable' } }, true);
        children.push(child); return child;
    } });
    const oneShots: string[] = [];
    context.mock.method(native, 'runText', async (command: string) => { oneShots.push(command); throw new Error('no one-shot process may start'); });
    try {
        windowNativeSession()!.offer(IMAGE);
        const host = new RuntimeHost(path.resolve('fixture-extension'), '1.4.0', { channel: 'stable' });
        for (let poll = 0; poll < 10; poll++) {
            const client = host.client(binding(IMAGE.path));
            await assert.rejects(client.rpc('health', 'health'), (error: unknown) => error instanceof ServiceError && error.code === 'service_unavailable');
            client.dispose();
        }
        assert.equal(windowNativeSession()!.launches, 1);
        const controls = children[0].requests.filter(item => 'control' in item.request);
        assert.equal(controls.length, 10);
        for (const item of controls) { assert.deepEqual(item.request, { control: { runtime_dir: path.resolve('fixture-control'), runtime_sha256: '33'.repeat(32), method: 'health' } }); }
        assert.deepEqual(oneShots, []);
    } finally { window.dispose(); }
    assert.equal(windowNativeSession(), undefined);
});

test('control views keep the console queue bound, deadline and closure codes', async () => {
    const { timers, children, session } = harness(() => undefined);
    try {
        const view = new SessionControlTransport(session, IMAGE.path, path.resolve('fixture-control'), '33'.repeat(32), 1000);
        const requests = Array.from({ length: 8 }, () => view.request('health'));
        await assert.rejects(view.request('health'), (error: unknown) => error instanceof ServiceError && error.code === 'console_queue_full' && !error.outcomeUnknown);
        await children[0].received(2);
        timers.advance(1000);
        const results = await Promise.allSettled(requests);
        results.forEach((result, index) => {
            assert.equal(result.status, 'rejected');
            if (result.status === 'rejected') { assert.ok(result.reason instanceof ServiceError); assert.equal(result.reason.code, 'console_deadline_exceeded'); assert.equal(result.reason.outcomeUnknown, index === 0); }
        });
        await children[0].received(3);
        assert.ok('cancel' in children[0].requests[2].request, 'the timed-out relay is cancelled in the session');
        await assert.rejects(view.request('health'), (error: unknown) => error instanceof ServiceError && error.code === 'console_transport_closed' && !error.outcomeUnknown);
        const fresh = new SessionControlTransport(session, IMAGE.path, path.resolve('fixture-control'), '33'.repeat(32), 1000);
        await assert.rejects(fresh.request({ value: 'é'.repeat(32768) }), (error: unknown) => error instanceof ServiceError && error.code === 'console_request_oversize');
        const pending = fresh.request('health');
        await children[0].received(4);
        fresh.dispose();
        await assert.rejects(pending, (error: unknown) => error instanceof ServiceError && error.code === 'console_client_closed' && error.outcomeUnknown);
        assert.equal(session.launches, 1);
    } finally { session.dispose(); }
});

async function images(context: TestContext) {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-session-'));
    context.after(() => fs.rm(root, { recursive: true, force: true }));
    const write = async (name: string, bytes: string) => { const file = path.join(root, name, 'airk'); await fs.mkdir(path.dirname(file)); await fs.writeFile(file, bytes); return file; };
    const bundle = await write('bundle', 'fixture session image'), installed = await write('installed', 'fixture session image'), predecessor = await write('predecessor', 'fixture older image');
    return { bundle, installed, predecessor, image: { path: bundle, sha256: createHash('sha256').update('fixture session image').digest('hex'), version: '1.4.0' } };
}

test('the bundle image and byte-identical installs run in the session; another build is an explicit one-shot', async (context: TestContext) => {
    const files = await images(context), children: FakeChild[] = [], oneShots: string[][] = [];
    context.mock.method(native, 'runText', async (command: string, args: string[]) => { oneShots.push([command, ...args]); return JSON.stringify({ one_shot: true }); });
    const window = openWindowSession({ timers: new ManualTimers(), launch: () => {
        const child = new FakeChild(request => 'hello' in request.request ? hello(files.image) : invoked({ session: request.request.invoke.argv }), true);
        children.push(child); return child;
    } });
    try {
        windowNativeSession()!.offer(files.image);
        assert.deepEqual(JSON.parse(await nativeText(files.bundle, ['runtime', 'inspect'], 60000)), { session: ['runtime', 'inspect'] });
        assert.deepEqual(JSON.parse(await nativeText(files.installed, ['runtime', 'source-scope'], 60000)), { session: ['runtime', 'source-scope'] });
        assert.deepEqual(JSON.parse(await nativeText(files.predecessor, ['runtime', 'self-test'], 60000)), { one_shot: true });
        assert.deepEqual(oneShots, [[files.predecessor, 'runtime', 'self-test']], 'only the different build runs as a temporary process');
        assert.equal(windowNativeSession()!.launches, 1);
    } finally { window.dispose(); }
    closeWindowSession();
    assert.deepEqual(JSON.parse(await nativeText(files.bundle, ['runtime', 'location'], 60000)), { one_shot: true }, 'without a window (uninstall worker) commands are one-shots');
    assert.equal(oneShots.length, 2);
});

test('a session without a verified image refuses work instead of starting a process', async () => {
    const children: FakeChild[] = [];
    const session = new NativeSession({ timers: new ManualTimers(), launch: () => { const child = new FakeChild(() => undefined, true); children.push(child); return child; } });
    try {
        await assert.rejects(session.serves(IMAGE.path), /native_session_image_unverified/);
        await assert.rejects(session.text(IMAGE.path, ['runtime', 'location'], 60000), /native_session_image_unverified/);
        assert.equal(children.length, 0);
    } finally { session.dispose(); }
});

test('a runtime that is not this session build keeps its own console relay', async (context: TestContext) => {
    const files = await images(context), calls: unknown[] = [];
    const { children, session } = harness(() => undefined, { image: files.image });
    try {
        const fallback: ControlTransport = { async request(method) { calls.push(method); return { status: 'health', observation: {} } as RecordValue; }, dispose() {} };
        const view = new SessionControlTransport(session, files.predecessor, path.resolve('fixture-control'), '33'.repeat(32), 1000, () => fallback);
        assert.equal((await view.request('health')).status, 'health');
        assert.deepEqual(calls, ['health']);
        assert.equal(children.length, 0, 'the session is not started for a different build');
    } finally { session.dispose(); }
});
