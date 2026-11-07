// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import test from 'node:test';
import { buildSync } from 'esbuild';
import type { ExplorerViewState } from '../explorerContracts';
import { ROUTE_KEYS, parseRoute } from '../explorerRoutes';

type Upgrade = NonNullable<ExplorerViewState['runtimeUpgrade']>;
const upgrade: Upgrade = { fromVersion: '1.8.7', toVersion: '1.8.9', stage: 'activating', message: 'Native activation is running.', startedAt: Date.now() - 3000, updatedAt: Date.now() };
const directory = path.resolve(__dirname, '../../src/explorerWebview');
const bundles = new Map<string, string>();

function browserModule(name: string, state?: unknown, route?: unknown) {
    const file = path.join(directory, name), nativeRequire = createRequire(file);
    let hook = 0;
    const hooks = {
        useEffect() {},
        useState(initial: unknown) {
            const next = hook++;
            return [state !== undefined && next === 0 ? state : route !== undefined && next === 1 ? route : typeof initial === 'function' ? initial() : initial, () => undefined];
        },
        useRef: (current: unknown) => ({ current }),
        useMemo: (get: () => unknown) => get(),
    };
    let built = bundles.get(name);
    if (!built) {
        built = buildSync({ entryPoints: [file], bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic', jsxImportSource: 'preact',
            external: ['preact', 'preact/hooks', 'preact/jsx-runtime', 'airk-explorer-help'] }).outputFiles[0].text;
        bundles.set(name, built);
    }
    const module = { exports: {} as any };
    vm.runInNewContext(built, { module, exports: module.exports,
        require: (id: string) => id === 'preact/hooks' ? hooks : id === 'airk-explorer-help' ? { topics: [] } : nativeRequire(id),
        acquireVsCodeApi: () => ({ getState() {}, setState() {}, postMessage() {} }),
        window: { addEventListener() {} }, setTimeout, clearTimeout, setInterval, clearInterval });
    return module.exports;
}

function children(node: any): any[] {
    if (Array.isArray(node)) { return node.flatMap(children); }
    if (!node || typeof node !== 'object') { return []; }
    return [node, ...children(node.props?.children)];
}
function renderedText(node: any): string {
    if (node === undefined || node === null || typeof node === 'boolean') { return ''; }
    if (Array.isArray(node)) { return node.map(renderedText).join(''); }
    if (typeof node !== 'object') { return String(node); }
    return renderedText(node.props?.children);
}

test('the actual App shares upgrade progress and persisted notice preferences on every Explorer screen', () => {
    const state = { revision: 1, extensionVersion: '1.8.9', trusted: true, projects: [], features: {}, connection: 'ready', runtimeUpgrade: upgrade };
    const dismissed = ['dismissed-update'];
    for (const key of ROUTE_KEYS) {
        const app = browserModule('app.tsx', { ...state, dismissedNotices: dismissed }, parseRoute(key)), tree = app.App();
        const banners = children(tree).filter(node => node.type?.name === 'RuntimeUpgradeProgress');
        assert.equal(banners.length, 1, key);
        assert.equal(banners[0].props.upgrade, upgrade, key);
        assert.equal(tree.props.value.dismissed, dismissed, key);
    }
    const app = browserModule('app.tsx', { ...state, runtimeUpgrade: undefined }, { workspace: 'indexing' });
    assert.equal(children(app.App()).filter(node => node.type?.name === 'RuntimeUpgradeProgress').length, 0);
});

test('upgrade rendering reports observed stages and elapsed time without invented completion percentages', () => {
    const ui = browserModule('runtimeUpgrade.tsx');
    const pending = ui.RuntimeUpgradeProgress({ upgrade }), text = renderedText(pending);
    assert.match(text, /Updating AIRK for your indexes/);
    assert.match(text, /1\.8\.7 → 1\.8\.9/);
    assert.match(text, /Activating the upgraded runtime/);
    assert.match(text, /Native activation is running/);
    assert.match(text, /Elapsed: \d+ seconds/);
    assert.doesNotMatch(text, /\d+%|rebuild/i);
    const progress = children(pending).find(node => node.type === 'progress');
    assert.ok(progress); assert.equal(progress.props.value, undefined);
    for (const stage of ['ready', 'failed']) {
        const terminal = ui.RuntimeUpgradeProgress({ upgrade: { ...upgrade, stage } });
        assert.equal(children(terminal).some(node => node.type === 'progress'), false);
        assert.equal(terminal.props.role, stage === 'failed' ? 'alert' : 'status');
        assert.match(renderedText(terminal), stage === 'failed' ? /upgrade has not completed/ : /runtime update finished/);
    }
});

test('split text distinguishes a real upgrade from unconfirmed or unsupported capability', () => {
    const ui = browserModule('runtimeUpgrade.tsx'), view = { generation: 1, busy: false };
    const pending = renderedText(ui.SplitAvailability({ view, upgrade }));
    assert.match(pending, /upgrading the runtime for this index to 1\.8\.9/);
    assert.match(pending, /automatically/);
    assert.match(pending, /Management → Service and the installation log/);
    assert.doesNotMatch(pending, /details above|progress is shown/);
    const unknown = renderedText(ui.SplitAvailability({ view }));
    assert.match(unknown, /not yet been confirmed/); assert.doesNotMatch(unknown, /is upgrading/);
    const unsupported = renderedText(ui.SplitAvailability({ view: { ...view, capabilities: { policy_contract: 1 } } }));
    assert.match(unsupported, /does not support splitting/); assert.doesNotMatch(unsupported, /is upgrading/);
    assert.equal(ui.upgradePending({ ...upgrade, stage: 'ready' }), false);
    assert.equal(ui.upgradePending({ ...upgrade, stage: 'failed' }), true);
});

test('upgrade notice identity ignores progress churn but separates completion, failures and later versions', () => {
    const ui = browserModule('runtimeUpgrade.tsx');
    const key = (value: Upgrade) => ui.RuntimeUpgradeProgress({ upgrade: value }).props.noticeKey;
    const active = key(upgrade);
    assert.equal(key({ ...upgrade, stage: 'reconnecting', message: 'Checking the connection.', updatedAt: Date.now() + 60000 }), active);
    const ready = key({ ...upgrade, stage: 'ready' });
    assert.notEqual(ready, active);
    assert.equal(key({ ...upgrade, stage: 'ready', startedAt: 1, updatedAt: 2, message: 'Connected again.' }), ready);
    assert.notEqual(key({ ...upgrade, stage: 'failed', message: 'Activation failed.' }), active);
    assert.notEqual(key({ ...upgrade, stage: 'failed', message: 'Activation failed.' }), key({ ...upgrade, stage: 'failed', message: 'Connection failed.' }));
    assert.notEqual(key({ ...upgrade, stage: 'ready', toVersion: '1.8.12' }), ready);
});

test('global error and permission notices use the same keys on all screens and do not replace their content', () => {
    const state = { revision: 1, extensionVersion: '1.8.12', trusted: false, workspaceTrusted: true,
        nativeExecutionEnabled: false, projects: [], features: {}, connection: 'disconnected',
        notice: { code: 'HOST_UNAVAILABLE', message: 'The local service could not be reached.' } };
    let prior: string[] | undefined;
    for (const key of ROUTE_KEYS) {
        const app = browserModule('app.tsx', state, parseRoute(key)), tree = app.App();
        const notices = children(tree).filter(node => node.type?.name === 'Notice');
        assert.equal(notices.length, 3, key);
        const identities = notices.map(node => node.props.noticeKey);
        if (prior) { assert.deepEqual(identities, prior, key); }
        prior = identities;
        assert.ok(children(tree).some(node => node.type?.name === (key === 'overview' ? 'Overview' : 'WorkspaceView')), key);
    }
});
