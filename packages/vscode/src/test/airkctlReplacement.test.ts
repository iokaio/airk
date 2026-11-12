// SPDX-License-Identifier: Apache-2.0
// The I4 replacement notice (1.8.30): while setup replaces or removes the
// installed files, the extension releases the installed `airkctl` and
// reconnects once the notice is gone. Decoding expectations come from the
// shared, independently reviewed fixtures/split/i4/i4_cases.json.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { canonical, decodeReplacementNotice, InstallRefusal, replacementNoticePath } from '../airkctl/install';
import { Airkctl, AirkctlUnavailable } from '../airkctl/index';
import { deriveState, executable } from '../airkctl/states';
import type { FileFacts, VerifyHost } from '../airkctl/verify';

const repo = path.resolve(__dirname, '../../../..');
const i4 = JSON.parse(fs.readFileSync(path.join(repo, 'fixtures/split/i4/i4_cases.json'), 'utf8')) as {
    limits: Record<string, number>;
    replacement_notices: { name: string; text: string; now_unix_ms: number; expect: string }[];
};
const outcome = (run: () => unknown) => { try { run(); return 'ok'; } catch (error) { if (error instanceof InstallRefusal) { return error.code; } throw error; } };

test('I4 replacement notice cases decode or refuse as the fixture states', () => {
    assert.ok(i4.replacement_notices.length >= 10, 'zero or missing notice cases is a failure');
    assert.equal(i4.limits.replacement_notice, 4096);
    assert.equal(i4.limits.replacement_notice_lifetime_ms, 600_000);
    for (const item of i4.replacement_notices) {
        assert.equal(outcome(() => decodeReplacementNotice(Buffer.from(item.text, 'utf8'), item.now_unix_ms)), item.expect, item.name);
    }
});

test('the notice lives beside the pointer on every platform', () => {
    assert.equal(replacementNoticePath('C:\\Users\\u', 'win32'), 'C:\\Users\\u\\AppData\\Local\\AIRK\\install-pointer\\replacing.json');
    assert.equal(replacementNoticePath('/Users/u', 'darwin'), '/Users/u/Library/Application Support/AIRK/install-pointer/replacing.json');
    assert.equal(replacementNoticePath('/home/u', 'linux'), '/home/u/.config/airk/install-pointer/replacing.json');
});

test('a notice in force is the updating state, in which airkctl is never executed', () => {
    const state = deriveState({ extensionVersion: '1.8.30', replacing: 'C:\\P' });
    assert.deepEqual(state, { kind: 'updating', program: 'C:\\P' });
    assert.equal(executable(state), false);
    assert.equal(deriveState({ extensionVersion: '1.8.30', gate: 'workspace_untrusted', replacing: 'C:\\P' }).kind, 'gated', 'the gate is consulted first');
});

test('a window releases airkctl while a notice is in force and re-checks once it is gone', async () => {
    const now = 1_800_000_000_100;
    const notice = canonical({ schema: 'airk-install-replacing/1', operation_id: '0e'.repeat(16), program: 'C:\\Users\\u\\AppData\\Local\\AIRK Service\\program', created_unix_ms: now - 100, expires_unix_ms: now + 599_000 });
    const noticeFile = replacementNoticePath('C:\\Users\\u', 'win32');
    const files = new Map<string, string>();
    const facts = (size: number): FileFacts => ({ kind: 'file', size, nlink: 1, dev: 1, ino: 1, mtimeMs: 1 });
    const host: VerifyHost = {
        platform: 'win32', home: 'C:\\Users\\u', developmentMode: false,
        lstat: async file => files.has(file) ? facts(Buffer.byteLength(files.get(file)!)) : undefined,
        realpath: async file => file,
        read: async file => Buffer.from(files.get(file) ?? '', 'utf8'),
        sha256: async () => '0'.repeat(64),
    };
    const lines: string[] = [];
    const airkctl = new Airkctl({ clientVersion: '1.8.30', verifyHost: host, gate: () => undefined, noticePollMs: 0, now: () => now, log: line => lines.push(line) });
    const states: string[] = [];
    airkctl.onDidChangeState(state => states.push(state.kind));
    try {
        await airkctl.poll();
        assert.deepEqual(states, [], 'no notice: nothing changes');
        files.set(noticeFile, notice);
        await airkctl.poll();
        assert.equal(airkctl.state.kind, 'updating');
        await assert.rejects(airkctl.oneShot(['service', 'status']), (error: unknown) => error instanceof AirkctlUnavailable && error.reason === 'updating');
        await assert.rejects(airkctl.session.json(['projects']));
        assert.equal(airkctl.session.launches, 0, 'no airkctl process starts while setup replaces it');
        assert.equal((await airkctl.check()).kind, 'updating', 'Check again keeps the window released');
        files.delete(noticeFile);
        await airkctl.poll();
        assert.equal(airkctl.state.kind, 'not_installed', 'the window verifies the installation again (none in this fixture)');
        assert.deepEqual(states, ['updating', 'updating', 'not_installed']);
        assert.ok(lines.some(line => line.includes('releasing airkctl')) && lines.some(line => line.includes('reconnecting')));
    } finally { airkctl.dispose(); }
});

test('an expired or malformed notice is not in force', async () => {
    const now = 1_800_000_000_100;
    const file = replacementNoticePath('C:\\Users\\u', 'win32');
    for (const text of [
        canonical({ schema: 'airk-install-replacing/1', operation_id: '0e'.repeat(16), program: 'C:\\P', created_unix_ms: now - 600_000, expires_unix_ms: now }),
        '{"schema":"airk-install-replacing/1"}\n',
    ]) {
        const host: VerifyHost = {
            platform: 'win32', home: 'C:\\Users\\u', developmentMode: false,
            lstat: async name => name === file ? { kind: 'file', size: text.length, nlink: 1, dev: 1, ino: 1, mtimeMs: 1 } : undefined,
            realpath: async name => name, read: async () => Buffer.from(text, 'utf8'), sha256: async () => '0'.repeat(64),
        };
        const airkctl = new Airkctl({ clientVersion: '1.8.30', verifyHost: host, gate: () => undefined, noticePollMs: 0, now: () => now });
        try { assert.equal((await airkctl.check()).kind, 'not_installed'); } finally { airkctl.dispose(); }
    }
});
