// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import test from 'node:test';
import { buildSync } from 'esbuild';

const file = path.resolve(__dirname, '../../src/explorerWebview/notices.tsx');
let bundle: string;

/** Exercise the actual component and bridge, retaining hooks until panel recreation. */
function noticeHost(saved: string[] = [], project?: string) {
    bundle ??= buildSync({ entryPoints: [file], bundle: true, write: false, platform: 'node', format: 'cjs',
        jsx: 'automatic', jsxImportSource: 'preact', external: ['preact', 'preact/hooks', 'preact/jsx-runtime'] }).outputFiles[0].text;
    const context = { dismissed: saved, project };
    const sent: any[] = [], slots: any[] = [], timers = new Map<number, () => void>();
    const events = new Map<string, (event: any) => void>();
    let cursor = 0, nextTimer = 0;
    const module = { exports: {} as any }, nativeRequire = createRequire(file);
    const hooks = {
        useContext: () => context,
        useState(initial: any) {
            const at = cursor++;
            if (!(at in slots)) { slots[at] = typeof initial === 'function' ? initial() : initial; }
            return [slots[at], (next: any) => { slots[at] = typeof next === 'function' ? next(slots[at]) : next; }];
        },
    };
    vm.runInNewContext(bundle, { module, exports: module.exports,
        require: (id: string) => id === 'preact/hooks' ? hooks : nativeRequire(id),
        acquireVsCodeApi: () => ({ getState() {}, setState() {}, postMessage: (message: any) => sent.push(message) }),
        window: { addEventListener: (name: string, callback: (event: any) => void) => events.set(name, callback) },
        setTimeout: (fn: () => void) => { const id = ++nextTimer; timers.set(id, fn); return id; }, clearTimeout: (id: number) => timers.delete(id),
    });
    return {
        context, sent, timers,
        key: module.exports.noticeKey as (kind: string, ...identity: unknown[]) => string,
        render(key: string, global = false) {
            cursor = 0;
            return module.exports.Notice({ noticeKey: key, children: 'Fictional operation completed.', global });
        },
        async reply(ok: boolean) {
            const request = sent.at(-1);
            assert.ok(request, 'a dismissal request must be sent before host acknowledgment');
            events.get('message')!({ data: { v: 1, type: 'result', requestId: request.requestId,
                ...(ok ? { ok: true, data: null } : { ok: false, error: { code: 'HOST_UNAVAILABLE', message: 'Preference storage failed.' } }) } });
            for (let i = 0; i < 5; i++) { await Promise.resolve(); }
        },
    };
}

function nodes(node: any): any[] {
    if (Array.isArray(node)) { return node.flatMap(nodes); }
    return !node || typeof node !== 'object' ? [] : [node, ...nodes(node.props?.children)];
}
function text(node: any): string {
    if (node === undefined || node === null || typeof node === 'boolean') { return ''; }
    return Array.isArray(node) ? node.map(text).join('') : typeof node === 'object' ? text(node.props?.children) : String(node);
}
function dismiss(tree: any) { return nodes(tree).find(node => node.type === 'button' && node.props['aria-label'] === 'Dismiss notice'); }

test('usage read errors use the persistent project notice while retaining explicit report retry', async () => {
    const usageFile = path.resolve(__dirname, '../../src/explorerWebview/usage.tsx'), nativeRequire = createRequire(usageFile);
    const built = buildSync({ entryPoints: [usageFile], bundle: true, write: false, platform: 'node', format: 'cjs',
        jsx: 'automatic', jsxImportSource: 'preact', external: ['preact', 'preact/hooks', 'preact/jsx-runtime'] }).outputFiles[0].text;
    const sent: any[] = [], timers = new Map<number, unknown>(), slots: any[] = ['<p>Retained report</p>', false, 'Native usage read failed.'];
    let cursor = 0;
    const module = { exports: {} as any };
    vm.runInNewContext(built, { module, exports: module.exports,
        require: (id: string) => id === 'preact/hooks' ? {
            useEffect() {}, useRef: (current: unknown) => ({ current }),
            useState: () => { const index = cursor++; return [slots[index], (value: unknown) => { slots[index] = value; }]; },
        } : nativeRequire(id),
        acquireVsCodeApi: () => ({ getState() {}, setState() {}, postMessage: (message: unknown) => sent.push(message) }),
        window: { addEventListener() {} }, setTimeout: () => { timers.set(1, true); return 1; }, clearTimeout: () => timers.clear(),
    });
    const tree = module.exports.UsageReport({ available: true }), notice = nodes(tree).find(node => node.type?.name === 'Notice');
    assert.ok(notice, 'the actual usage failure must pass through shared persistent dismissal');
    assert.equal(notice.props.noticeKey, JSON.stringify(['usage-error', 'Native usage read failed.']));
    assert.equal(notice.props.global, undefined, 'usage failures retain project scope');
    assert.match(text(notice), /previous report remains below/);
    const host = noticeHost([], 'project-a');
    dismiss(host.render(notice.props.noticeKey)).props.onClick(); await host.reply(true);
    assert.equal(host.render(notice.props.noticeKey), null);
    const reopened = noticeHost([host.sent[0].key], 'project-a');
    assert.equal(reopened.render(notice.props.noticeKey), null, 'the same error stays dismissed after reopening');
    assert.ok(noticeHost([host.sent[0].key], 'project-b').render(notice.props.noticeKey), 'another project retains its own failure');
    nodes(notice).find(node => node.type === 'button' && text(node) === 'Retry report').props.onClick();
    assert.equal(sent.length, 1); assert.equal(sent[0].type, 'usage.action'); assert.equal(sent[0].action, 'refresh');
    assert.equal(slots[2], undefined, 'explicit retry clears the old error while a new result is pending');
});

test('notice dismissal waits for persistence and survives panel recreation with host preferences', async () => {
    const f = noticeHost([], 'project-a'), key = f.key('runtime', '1.8.11', '1.8.12', 'ready');
    const tree = f.render(key, true), close = dismiss(tree);
    assert.ok(close); assert.equal(close.props.title, 'Dismiss this notice permanently');
    close.props.onClick();
    assert.equal(f.sent.length, 1); assert.equal(f.sent[0].type, 'notice.dismiss'); assert.equal(f.sent[0].key, key);
    assert.ok(f.render(key, true), 'pending persistence must not hide the notice');
    assert.equal(dismiss(f.render(key, true)).props.disabled, true);
    await f.reply(true);
    assert.equal(f.render(key, true), null);
    assert.equal(f.timers.size, 0);
    const reopened = noticeHost([f.sent[0].key], 'project-b');
    assert.equal(reopened.render(key, true), null, 'a global dismissal survives panel recreation and project changes');
    assert.ok(reopened.render(reopened.key('runtime', '1.8.12', '1.8.13', 'ready'), true), 'a later update is a distinct notice');
    assert.equal(reopened.sent.length, 0, 'reading a saved dismissal sends no request');
});

test('failed preference writes keep the notice visible and allow an explicit retry', async () => {
    const f = noticeHost(), key = f.key('operation', 'completed');
    dismiss(f.render(key)).props.onClick(); await f.reply(false);
    const failed = f.render(key);
    assert.match(text(failed), /Could not save dismissal:.*Preference storage failed/);
    assert.equal(dismiss(failed).props.disabled, false);
    assert.equal(f.sent.length, 1, 'a failed dismissal is never automatically replayed');
    dismiss(failed).props.onClick(); await f.reply(true);
    assert.equal(f.render(key), null); assert.equal(f.sent.length, 2);
});

test('screen notices preserve exact content and project identity while ignoring navigation', async () => {
    const f = noticeHost([], 'project-a'), key = f.key('control-outcome', 'Paused');
    dismiss(f.render(key)).props.onClick(); await f.reply(true);
    const saved = f.sent[0].key;
    assert.equal(saved, f.key('project', 'project-a', key));
    const navigated = noticeHost([saved], 'project-a');
    assert.equal(navigated.render(key), null);
    assert.ok(navigated.render(navigated.key('control-outcome', 'Resumed')), 'a changed outcome remains visible');
    navigated.context.project = 'project-b';
    assert.ok(navigated.render(key), 'another project retains its own notices');
});

test('late dismissal outcomes belong to their original notice after its content changes', async () => {
    for (const persisted of [false, true]) {
        const f = noticeHost(), first = f.key('operation', 'first'), next = f.key('operation', 'next');
        dismiss(f.render(first)).props.onClick();
        assert.ok(f.render(next), 'the replacement notice remains visible while the previous write is pending');
        await f.reply(persisted);
        const replacement = f.render(next);
        assert.ok(replacement, `a late ${persisted ? 'success' : 'failure'} cannot dismiss the replacement notice`);
        assert.doesNotMatch(text(replacement), /Could not save dismissal|Preference storage failed/);
        assert.equal(dismiss(replacement).props.disabled, false);
        assert.equal(f.sent.length, 1, 'receiving a replacement notice never submits another write');
        const original = f.render(first);
        if (persisted) { assert.equal(original, null); }
        else { assert.match(text(original), /Could not save dismissal:.*Preference storage failed/); }
    }
});
