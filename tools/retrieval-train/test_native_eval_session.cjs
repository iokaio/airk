// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { EventEmitter } = require('node:events');
const { PassThrough, Writable } = require('node:stream');
const { createPinnedSession, sameDirectory } = require('./native_eval_session.cjs');
const library = require('../../packages/vscode/out/airkctl/session.js');
const ROOT = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const frame = value => { const raw = Buffer.from(JSON.stringify(value));const size = Buffer.alloc(4);size.writeUInt32BE(raw.length);return Buffer.concat([size, raw]); };

function fixture(scenario = {}) {
    const directory = fs.mkdtempSync(path.join(ROOT, 'artifacts/native-session-unit-'));
    const pair = path.join(directory, 'pair');fs.mkdirSync(pair);
    const executable = Buffer.from('unit fake executable; never spawned');
    fs.writeFileSync(path.join(pair, 'airkctl.exe'), executable);
    fs.writeFileSync(path.join(pair, 'airk-runtime.json'), JSON.stringify({ version: '1.9.0', airkctl_sha256: sha(executable) }));
    const ready = { schema: 'airk.private-arbiter-fixture/1', evaluation: true, release_qualified: false,
        control_root: directory, health: { instance: 'a'.repeat(32), catalog_clock: { namespace: 'b'.repeat(32) } } };
    const bindings = { schema: 'airk.private-arbiter-index-bindings/1', evaluation: true, release_qualified: false,
        control_root: directory, instance: ready.health.instance, namespace: ready.health.catalog_clock.namespace,
        bindings: [{ index_id: 'unit-index', window_scope_selection: { namespace: 'b'.repeat(32), expected_catalog_revision: 1, index_ids: ['unit-index'], project_ids: [] } }] };
    const readyRaw = Buffer.from(JSON.stringify(ready)), bindingRaw = Buffer.from(JSON.stringify(bindings));
    const readyFile = path.join(directory, 'ready.json'), bindingFile = path.join(directory, 'bindings.json');
    fs.writeFileSync(readyFile, readyRaw);fs.writeFileSync(bindingFile, bindingRaw);
    const args = path.join(directory, 'arguments.json');fs.writeFileSync(args, '{}');
    const requests = [];let child;
    const launch = () => {
        child = new EventEmitter();child.stdout = new PassThrough();child.stderr = new PassThrough();
        child.exitCode = null;child.signalCode = null;
        const reader = new library.FrameReader();
        child.stdin = new Writable({ write(bytes, encoding, done) {
            for (const raw of reader.push(bytes)) {
                const request = JSON.parse(raw);requests.push(request.request);
                queueMicrotask(() => {
                    let outcome;
                    if (request.request.hello) outcome = { status: 'hello', protocol: 2, version: '1.9.0',
                        i2_contract: 1, capabilities: [], executable_sha256: sha(executable),
                        service_instance: scenario.changedHello ? 'c'.repeat(32) : ready.health.instance, window_id: 'd'.repeat(32) };
                    else if (request.request.scope_select) outcome = { status: 'scope' };
                    else if (request.request.invoke) outcome = scenario.busy ? { status: 'failed', code: 'session_busy', retryable: true }
                        : { status: 'invoked', exit_code: 0, class: 'success', stdout: 'Café 日本語 🚀 full native stdout', stderr: 'complete native stderr' };
                    else return;
                    child.stdout.write(frame({ id: request.id, outcome }));
                });
            }
            done();
        } });
        child.stdin.on('finish', () => queueMicrotask(() => { child.exitCode = 0;child.emit('exit'); }));
        child.kill = () => { child.exitCode = 1;child.emit('exit');return true; };
        return child;
    };
    const session = createPinnedSession({ runtime_pair: pair, control_root: directory,
        fixture_ready: readyFile, ready_sha256: sha(readyRaw), index_bindings: bindingFile,
        bindings_sha256: sha(bindingRaw), index_id: 'unit-index', transcript_dir: path.join(directory, 'transcript') },
    { library, runner: { airkctlEnvironment: () => ({}) }, spawn: launch });
    return { session, requests, args, get child() { return child; }, cleanup() {
        const resolved = path.resolve(directory), relative = path.relative(path.join(ROOT, 'artifacts'), resolved);
        assert(!relative.startsWith('..') && !path.isAbsolute(relative));
        fs.rmSync(resolved, { recursive: true, force: true });
    } };
}

test('existing compiled I2 client preserves full channels and omits --index', async () => {
    const current = fixture();
    try {
        const ready = await current.session.initialize();assert.equal(ready.launch_count, 1);
        const value = await current.session.invoke({ id: 'one', tool: 'search', args_file: current.args, deadline_ms: 1000 });
        assert.equal(value.result.stdout, 'Café 日本語 🚀 full native stdout');
        assert.equal(value.result.stderr, 'complete native stderr');
        assert(!value.argv.includes('--index'));
        const closed = await current.session.close();assert.equal(closed.poisoned, null);assert.equal(closed.child_exit_observed, true);
    } finally { await current.session.close();current.cleanup(); }
});

test('busy response permanently refuses before the library can retry', async () => {
    const current = fixture({ busy: true });
    try {
        await current.session.initialize();
        await assert.rejects(current.session.invoke({ id: 'one', tool: 'search', args_file: current.args, deadline_ms: 1000 }));
        await assert.rejects(current.session.invoke({ id: 'two', tool: 'search', args_file: current.args, deadline_ms: 1000 }), /permanently refused/);
        assert.equal(current.requests.filter(value => value.invoke).length, 1);
        assert.equal(current.session.poisoned, 'session_busy_no_retry');
    } finally { await current.session.close();current.cleanup(); }
});

test('changed original hello refuses before scope selection and native resync never reopens', async () => {
    const changed = fixture({ changedHello: true });
    try {
        await assert.rejects(changed.session.initialize());
        assert.equal(changed.requests.filter(value => value.scope_select).length, 0);
    } finally { await changed.session.close();changed.cleanup(); }
    const current = fixture();
    try {
        await current.session.initialize();
        current.child.stdout.write(frame({ event: { kind: 'resync', reason: 'service_restarted' } }));
        await assert.rejects(current.session.invoke({ id: 'one', tool: 'search', args_file: current.args, deadline_ms: 1000 }), /permanently refused/);
        assert.equal(current.requests.filter(value => value.invoke).length, 0);
    } finally { await current.session.close();current.cleanup(); }
});

test('actual directory identity accepts Windows extended path and rejects a different directory', () => {
    const directory=fs.mkdtempSync(path.join(ROOT,'artifacts/native-session-alias-'));
    try {
        fs.mkdirSync(path.join(directory,'other'));
        const alias=process.platform==='win32' ? '\\\\?\\'+directory : path.join(directory,'.');
        assert.equal(sameDirectory(directory,alias),true);
        assert.equal(sameDirectory(directory,path.join(directory,'other')),false);
    } finally {
        const relative=path.relative(path.join(ROOT,'artifacts'),path.resolve(directory));
        assert(!relative.startsWith('..')&&!path.isAbsolute(relative));fs.rmSync(directory,{recursive:true,force:true});
    }
});
