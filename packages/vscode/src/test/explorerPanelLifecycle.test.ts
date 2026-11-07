// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import Module from 'node:module';
import test from 'node:test';

function fixture() {
    const panels: any[] = [], closedTabs: any[][] = [], errors: string[] = [];
    let close: (tabs: any[]) => Promise<boolean> = async () => true;
    const disposable = { dispose() {} };
    const tabListeners = new Set<(event: any) => void>(), groupListeners = new Set<(event: any) => void>();
    const stub = {
        ViewColumn: { Active: 1 },
        Uri: { joinPath: (base: any, ...parts: string[]) => ({ toString: () => `${base.path}/${parts.join('/')}` }) },
        window: {
            tabGroups: { all: [] as any[], close: (tabs: any[]) => { closedTabs.push(tabs); return close(tabs); },
                onDidChangeTabs: (listener: (event: any) => void) => { tabListeners.add(listener); return { dispose() { tabListeners.delete(listener); } }; },
                onDidChangeTabGroups: (listener: (event: any) => void) => { groupListeners.add(listener); return { dispose() { groupListeners.delete(listener); } }; } },
            showErrorMessage: async (error: string) => { errors.push(error); },
            createWebviewPanel: () => {
                let receive: (value: unknown) => void = () => {}, disposed: () => void = () => {};
                const panel = {
                    disposed: false, reveals: 0, posted: [] as any[],
                    webview: {
                        html: '', cspSource: 'vscode-resource:', asWebviewUri: (uri: unknown) => uri,
                        postMessage: async (message: unknown) => { panel.posted.push(message); return true; },
                        onDidReceiveMessage: (handler: typeof receive) => { receive = handler; return disposable; },
                    },
                    onDidChangeViewState: () => disposable,
                    onDidDispose: (handler: typeof disposed) => { disposed = handler; return disposable; },
                    reveal() { this.reveals++; },
                    dispose() { if (!this.disposed) { this.disposed = true; disposed(); } },
                    send: (value: unknown) => receive(value),
                };
                panels.push(panel); return panel;
            },
        },
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const load = () => {
        delete require.cache[require.resolve('../explorerPanel')];
        loader._load = function (request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
        try { return require('../explorerPanel').ExplorerPanel as typeof import('../explorerPanel').ExplorerPanel; }
        finally { loader._load = original; }
    };
    const create = (Panel = load()) => {
        const visible: boolean[] = [], messages: unknown[] = [];
        return { owner: new Panel({ path: '/extension' } as any, { visible: value => visible.push(value), message: value => messages.push(value) }), visible, messages };
    };
    const tabsChanged = (opened: any[] = [], closed: any[] = []) => { for (const listener of [...tabListeners]) { listener({ opened, closed, changed: [] }); } };
    const groupsChanged = (closed: any[]) => { for (const listener of [...groupListeners]) { listener({ opened: [], closed, changed: [] }); } };
    return { panels, closedTabs, errors, stub, create, load, tabsChanged, groupsChanged, setClose(value: typeof close) { close = value; } };
}
const flush = () => new Promise(resolve => setImmediate(resolve));
const ready = { v: 1, type: 'ready', requestId: 'ready' };

test('repeated Explorer opens reveal one panel, including after closing it', () => {
    const f = fixture(), view = f.create();
    try {
        view.owner.reveal(); view.owner.reveal(); view.owner.reveal();
        assert.equal(f.panels.length, 1); assert.equal(f.panels[0].reveals, 2);
        f.panels[0].dispose(); assert.equal(view.owner.open, false);
        view.owner.reveal(); assert.equal(f.panels.length, 2);
        assert.deepEqual(view.visible, [true, false, true]);
    } finally { view.owner.dispose(); }
    assert.deepEqual(view.visible, [true, false, true, false]);
});

test('successor CommonJS graphs close the previous Explorer owner before creating a replacement', () => {
    const f = fixture(), first = f.create(), second = f.create();
    try {
        first.owner.reveal(); second.owner.reveal();
        assert.equal(f.panels.length, 2); assert.equal(f.panels[0].disposed, true);
        assert.equal(f.panels.filter(panel => !panel.disposed).length, 1);
        assert.deepEqual(first.visible, [true, false]);
        first.owner.reveal(); assert.equal(f.panels.length, 2, 'superseded owner cannot reclaim the panel');
        first.owner.dispose(); second.owner.reveal();
        assert.equal(f.panels[1].disposed, false, 'old disposal cannot clear successor ownership');
    } finally { first.owner.dispose(); second.owner.dispose(); }
});

test('orphan cleanup coalesces opens and delivers queued navigation/state to the single new panel', async () => {
    const f = fixture(), view = f.create();
    let release!: (closed: boolean) => void;
    const gate = new Promise<boolean>(resolve => { release = resolve; });
    f.setClose(() => gate);
    const orphan = { input: { viewType: 'mainThreadWebview-airk.explorer' } }, other = { input: { viewType: 'other.explorer' } };
    f.stub.window.tabGroups.all = [{ tabs: [orphan, other] }];
    try {
        view.owner.reveal(); view.owner.reveal();
        view.owner.post({ v: 1, type: 'navigate', route: '#indexing/library' });
        view.owner.post({ v: 1, type: 'state', state: { revision: 1 } } as any);
        view.owner.post({ v: 1, type: 'state', state: { revision: 2 } } as any);
        assert.equal(view.owner.open, true); assert.equal(f.panels.length, 0);
        assert.deepEqual(f.closedTabs, [[orphan]], 'other webviews stay open');
        release(true); await flush();
        assert.equal(f.panels.length, 1); f.panels[0].send(ready);
        assert.deepEqual(f.panels[0].posted.map((message: any) => message.type), ['navigate', 'state']);
        assert.equal(f.panels[0].posted[1].state.revision, 2);
    } finally { view.owner.dispose(); }
});

test('disposing during orphan cleanup prevents a late Explorer from reopening', async () => {
    const f = fixture(), view = f.create();
    let release!: (closed: boolean) => void;
    f.setClose(() => new Promise<boolean>(resolve => { release = resolve; }));
    f.stub.window.tabGroups.all = [{ tabs: [{ input: { viewType: 'airk.explorer' } }] }];
    view.owner.reveal(); view.owner.dispose(); release(true); await flush();
    assert.equal(f.panels.length, 0); assert.equal(view.owner.open, false);
});

test('failed orphan cleanup reports failure and does not create duplicate Explorer tabs', async () => {
    const f = fixture(), view = f.create();
    f.setClose(async () => false);
    f.stub.window.tabGroups.all = [{ tabs: [{ input: { viewType: 'airk.explorer' } }] }];
    try {
        view.owner.reveal(); await flush();
        assert.equal(f.panels.length, 0); assert.equal(view.owner.open, false);
        assert.match(f.errors[0], /could not reopen.*did not close/);
    } finally { view.owner.dispose(); }
});

test('closing the exact editor tab clears a retained webview handle and permits reopening', () => {
    const f = fixture(), view = f.create();
    try {
        view.owner.reveal();
        const group: any = { tabs: [] }, tab = { input: { viewType: 'mainThreadWebview-airk.explorer' }, group };
        group.tabs.push(tab); f.stub.window.tabGroups.all = [group]; f.tabsChanged([tab]);
        // Merely hiding the editor (without closing its Tab) must retain it.
        f.tabsChanged(); assert.equal(view.owner.open, true);
        group.tabs = []; f.tabsChanged([], [tab]);
        assert.equal(view.owner.open, false, 'Tab close does not depend on a native onDidDispose callback');
        assert.equal(f.panels[0].disposed, true); assert.deepEqual(view.visible, [true, false]);
        view.owner.reveal(); assert.equal(f.panels.length, 2); assert.equal(view.owner.open, true);
        const next = { input: { viewType: 'mainThreadWebview-airk.explorer' }, group };
        group.tabs = [next]; f.tabsChanged([next]);
        f.tabsChanged([], [tab]);
        assert.equal(view.owner.open, true, 'a delayed former-tab close cannot dispose the successor');
        assert.equal(f.panels[1].disposed, false);
    } finally { view.owner.dispose(); }
});

test('closing the owned editor group also clears a retained Explorer handle', () => {
    const f = fixture(), view = f.create();
    try {
        view.owner.reveal();
        const group: any = { tabs: [] }, tab = { input: { viewType: 'mainThreadWebview-airk.explorer' }, group };
        group.tabs.push(tab); f.stub.window.tabGroups.all = [group]; f.tabsChanged([tab]);
        f.stub.window.tabGroups.all = []; f.groupsChanged([group]);
        assert.equal(view.owner.open, false); assert.equal(f.panels[0].disposed, true);
        assert.deepEqual(view.visible, [true, false]);
    } finally { view.owner.dispose(); }
});
