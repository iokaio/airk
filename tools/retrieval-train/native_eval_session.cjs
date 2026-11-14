// SPDX-License-Identifier: Apache-2.0
'use strict';
// Private evaluation adapter. I2 framing/DTOs remain the compiled native-owned
// AirkctlSession client's responsibility; this file never implements that wire.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');

const ROOT = path.resolve(__dirname, '..', '..');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const inside = (parent, child) => { const rel = path.relative(parent, child); return rel === '' || (!rel.startsWith('..'+path.sep) && rel !== '..' && !path.isAbsolute(rel)); };
function sameDirectory(left, right) {
    const a = fs.statSync(left, { bigint: true }), b = fs.statSync(right, { bigint: true });
    if (!a.isDirectory() || !b.isDirectory() || a.ino <= 0n || b.ino <= 0n) throw Error('directory identity unavailable');
    return a.dev === b.dev && a.ino === b.ino;
}

function createPinnedSession(config, dependencies = {}) {
    const directory = path.resolve(config.transcript_dir);
    if (!inside(path.join(ROOT, 'artifacts'), directory) || fs.existsSync(directory)) throw Error('fresh private transcript directory required');
    const seals = new Map();
    function retain(filename, expected) {
        const target = path.resolve(filename), bytes = fs.readFileSync(target);
        if (expected && digest(bytes) !== expected) throw Error('adapter input identity differs: '+path.basename(target));
        seals.set(target, bytes);return bytes;
    }
    for (const field of ['ready_sha256', 'bindings_sha256']) if (!/^[a-f0-9]{64}$/.test(config[field] || '')) throw Error('explicit input digest required: '+field);
    if (!dependencies.library) {
        for (const [name, field] of [['session.js', 'session_module_sha256'], ['runner.js', 'runner_module_sha256'], ['adminChunks.js', 'admin_chunks_sha256']]) {
            if (!/^[a-f0-9]{64}$/.test(config[field] || '')) throw Error('explicit compiled client digest required: '+field);
            retain(path.join(ROOT, 'packages/vscode/out/airkctl', name), config[field]);
        }
    }
    const library = dependencies.library || require(path.join(ROOT, 'packages/vscode/out/airkctl/session.js'));
    const runner = dependencies.runner || require(path.join(ROOT, 'packages/vscode/out/airkctl/runner.js'));
    for (const [file, raw] of seals) if (!fs.readFileSync(file).equals(raw)) throw Error('compiled client drift while loading');
    const launchProcess = dependencies.spawn || spawn;
    const readyRaw = retain(config.fixture_ready, config.ready_sha256);
    const bindingsRaw = retain(config.index_bindings, config.bindings_sha256);
    const ready = JSON.parse(readyRaw), bindings = JSON.parse(bindingsRaw);
    if (ready.schema !== 'airk.private-arbiter-fixture/1' || ready.evaluation !== true || ready.release_qualified !== false ||
        bindings.schema !== 'airk.private-arbiter-index-bindings/1' || bindings.evaluation !== true || bindings.release_qualified !== false ||
        bindings.instance !== ready.health.instance || bindings.namespace !== ready.health.catalog_clock.namespace ||
        !sameDirectory(bindings.control_root, ready.control_root) || !sameDirectory(config.control_root, ready.control_root)) throw Error('original native fixture authority differs');
    const rows = bindings.bindings.filter(row => row.index_id === config.index_id);
    if (rows.length !== 1 || !rows[0].window_scope_selection) throw Error('one native-emitted window scope selection required');
    const pair = path.resolve(config.runtime_pair);
    const metadata = JSON.parse(retain(path.join(pair, 'airk-runtime.json')));
    const image = { path: path.join(pair, 'airkctl.exe'), sha256: metadata.airkctl_sha256, version: metadata.version };
    retain(image.path, image.sha256);
    retain(__filename);
    fs.mkdirSync(directory, { recursive: true });
    fs.writeFileSync(path.join(directory, 'fixture-ready.json'), readyRaw);
    fs.writeFileSync(path.join(directory, 'index-bindings.json'), bindingsRaw);
    const identity = { ready_sha256: digest(readyRaw), bindings_sha256: digest(bindingsRaw),
        service_instance: ready.health.instance, catalog_namespace: ready.health.catalog_clock.namespace,
        index_id: config.index_id, image, input_seals: Object.fromEntries([...seals].map(([file, raw]) => [file, digest(raw)])) };
    const events = [];let poisoned = null, normalClose = false, launched = 0, child = null, childExit = null, session;
    function append(name, bytes) { fs.appendFileSync(path.join(directory, name), bytes); }
    function verify() {
        if (poisoned) throw Error('evaluation session permanently refused: '+poisoned);
        for (const [file, raw] of seals) if (!fs.readFileSync(file).equals(raw)) throw Error('evaluation session consumed input drift');
    }
    function poison(reason) {
        if (!poisoned) { poisoned = reason;append('events.jsonl', JSON.stringify({ kind: 'permanent_refusal', reason })+'\n'); }
        session?.dispose();
    }
    const launch = verifiedImage => {
        verify();
        if (++launched !== 1) { poison('respawn_forbidden');throw Error('evaluation respawn forbidden'); }
        const args = ['--service-runtime-dir', config.control_root, 'session', '--owner-watch', '--label', 'AIRK private public-fixture evaluation'];
        child = launchProcess(verifiedImage.path, args, { cwd: pair, windowsHide: true, shell: false,
            stdio: 'pipe', env: runner.airkctlEnvironment() });
        childExit = new Promise(resolve => child.on('exit', resolve));
        const reader = new library.FrameReader();
        // Installed BEFORE AirkctlSession's receiver: a busy response is retired
        // before the reusable product client can schedule its normal busy retry.
        child.stdout.on('data', bytes => {
            append('from-child.bin', bytes);
            try {
                for (const frame of reader.push(bytes)) {
                    const decoded = library.decodeFrame(JSON.parse(frame.toString('utf8')));
                    append('decoded-native.jsonl', JSON.stringify(decoded)+'\n');
                    if (decoded.kind === 'reply' && decoded.outcome.status === 'hello' && decoded.outcome.service_instance !== ready.health.instance) poison('original_service_instance_changed');
                    if (decoded.kind === 'reply' && decoded.outcome.code === 'session_busy') poison('session_busy_no_retry');
                    if (!normalClose && decoded.kind === 'event' && ['resync', 'closing'].includes(decoded.event.kind)) poison('native_'+decoded.event.kind);
                }
            } catch (error) { poison('raw_native_protocol_observation_failed'); }
        });
        child.stderr.on('data', bytes => append('child-stderr.bin', bytes));
        const originalWrite = child.stdin.write;
        child.stdin.write = function(chunk, ...rest) {
            const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, typeof rest[0] === 'string' ? rest[0] : 'utf8');
            append('to-child.bin', bytes);
            return originalWrite.call(this, chunk, ...rest);
        };
        return child;
    };
    session = new library.AirkctlSession({ clientVersion: image.version, launch, maxQueued: 1 }, verify);
    session.onEvent(event => {
        events.push(event);append('events.jsonl', JSON.stringify(event)+'\n');
        if (!normalClose && ['resync', 'session_lost', 'closing'].includes(event.kind)) poison('native_'+event.kind);
    });
    session.bind(image);
    return {
        async initialize() {
            verify();const selection = await session.scopeSelect(rows[0].window_scope_selection);verify();
            if (session.hello?.serviceInstance !== ready.health.instance || selection.status !== 'scope') { poison('original_window_selection_refused');throw Error('original window selection refused'); }
            const result = { kind: 'ready', identity, hello: session.hello, scope_result: selection,
                events: [...events], launch_count: launched, scope: 'Existing compiled I2 session and native window_route, original fixture only; no respawn or automatic busy retry.' };
            fs.writeFileSync(path.join(directory, 'session-ready.json'), JSON.stringify(result, null, 2)+'\n');return result;
        },
        async invoke(request) {
            verify();
            if (!request || !['search', 'context', 'read_file', 'answer'].includes(request.tool) || !Number.isInteger(request.deadline_ms) || request.deadline_ms < 1 || request.deadline_ms > 30000 || !path.isAbsolute(request.args_file) || !inside(path.join(ROOT, 'artifacts'), path.resolve(request.args_file))) throw Error('invalid private read invocation');
            const raw = fs.readFileSync(request.args_file);
            const parsed = JSON.parse(raw);
            if (raw.length > 65536 || parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) throw Error('invalid native read argument file');
            const argv = ['tools', 'call', request.tool, '--args-file', request.args_file];
            const started = performance.now();
            try {
                const result = await session.invoke(argv, { deadlineMs: request.deadline_ms });verify();
                if (session.hello?.serviceInstance !== ready.health.instance) throw Error('original session greeting changed');
                if (!fs.readFileSync(request.args_file).equals(raw)) throw Error('actual invocation arguments changed');
                return { kind: 'invoked', id: request.id, result, argv, arguments_sha256: digest(raw),
                    elapsed_seconds: (performance.now()-started)/1000, hello: session.hello,
                    identity, events: [...events], launch_count: launched };
            } catch (error) {
                poison(error.code || 'invoke_failed_no_retry');throw error;
            }
        },
        async close() {
            normalClose = true;session.dispose();
            if (childExit) {
                let timer;
                try { await Promise.race([childExit, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('session child close deadline')), 10000); })]); }
                finally { clearTimeout(timer); }
            }
            for (const [file, raw] of seals) if (!fs.readFileSync(file).equals(raw)) throw Error('evaluation session source drift at close');
            const result = { kind: 'closed', identity, launch_count: launched, poisoned, events, child_exit_observed: !!childExit,
                child_exit_code: child?.exitCode ?? null, child_signal_code: child?.signalCode ?? null,
                scope: 'Observed I2 child exit only; service/worker cleanup and exclusive-lease recovery require their own native receipts.' };
            fs.writeFileSync(path.join(directory, 'session-closed.json'), JSON.stringify(result, null, 2)+'\n');return result;
        },
        get poisoned() { return poisoned; },
    };
}

async function main() {
    const config = JSON.parse(fs.readFileSync(process.argv[2]));
    const session = createPinnedSession(config);
    const send = value => process.stdout.write(JSON.stringify(value)+'\n');
    try {
        send(await session.initialize());
        const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
        for await (const line of input) {
            if (Buffer.byteLength(line) > 192*1024) throw Error('private invocation line exceeds bound');
            const request = JSON.parse(line);
            if (request.kind === 'close') { input.close();break; }
            send(await session.invoke(request));
        }
    } catch (error) {
        send({ kind: 'failed', code: error.code || error.message, sent: error.sent ?? null,
            outcome_unknown: error.outcomeUnknown ?? null, retry: false });process.exitCode = 1;
    } finally { send(await session.close()); }
}

module.exports = { createPinnedSession, sameDirectory };
if (require.main === module) main().catch(error => { process.stderr.write(String(error)+'\n');process.exitCode = 1; });
