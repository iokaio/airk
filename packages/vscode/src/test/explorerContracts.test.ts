// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { MANAGE_ACTIONS, MANAGE_COMMANDS, parseWebviewMessage, WEBVIEW_MESSAGES } from '../explorerContracts';
import { EXPLORER_ERRORS, explorerError } from '../explorerErrors';
import { availability, FEATURES } from '../explorerFeatures';
import { formatRoute, parseRoute, ROUTE_KEYS, routeKey, WORKSPACES } from '../explorerRoutes';

test('routes resolve default tabs, focus and reject anything outside the manifest', () => {
    assert.deepEqual(parseRoute('#overview'), { workspace: 'overview' });
    assert.deepEqual(parseRoute('search'), { workspace: 'search', tab: 'query' });
    assert.deepEqual(parseRoute('#manage/indexing?focus=extraction'), { workspace: 'manage', tab: 'indexing', focus: 'extraction' });
    for (const bad of ['#nowhere', '#search/unknown', '#overview/tab', '#search/query/extra', '#manage/indexing?focus=Bad!',
        '#manage/indexing?other=x', '#search?focus=a?b', '#' + 'x'.repeat(200), '#manage/indexing?focus=']) {
        assert.equal(parseRoute(bad), undefined, bad);
    }
    for (const key of ROUTE_KEYS) {
        const route = parseRoute(key)!;
        assert.equal(routeKey(route), key);
        assert.deepEqual(parseRoute(formatRoute(route)), route);
    }
    assert.equal(new Set(ROUTE_KEYS).size, ROUTE_KEYS.length);
    assert.equal(WORKSPACES.filter(workspace => workspace.placement === 'pinned').map(workspace => workspace.id).join(), 'manage');
    assert.equal(WORKSPACES.find(workspace => workspace.id === 'manage')!.tabs.length, 9);
});

test('every route has an availability and unknown keys are unsupported', () => {
    assert.deepEqual(Object.keys(FEATURES).sort(), [...ROUTE_KEYS].sort());
    for (const key of ROUTE_KEYS) { assert.ok(['implemented', 'prototype_only', 'unsupported', 'disabled'].includes(availability(key))); }
    assert.equal(availability('reports/builder'), 'unsupported');
    assert.ok(Object.isFrozen(FEATURES));
});

test('errors come from one registry and native codes pass through bounded', () => {
    for (const entry of Object.values(EXPLORER_ERRORS)) { assert.ok(['assistance', 'host', 'service', 'webview'].includes(entry.plane)); }
    assert.equal(explorerError('UI_NOT_ATTACHED').code, 'UI_NOT_ATTACHED');
    assert.equal(explorerError('console_transport_closed').code, 'console_transport_closed');
    assert.equal(explorerError('<script>').code, 'TOOL_ERROR');
    assert.ok(explorerError('TOOL_ERROR', 'x'.repeat(5000)).message.length < 800);
});

test('webview messages require the exact declared fields', () => {
    const search = { v: 1, type: 'search.run', requestId: 'r1', query: 'session refresh', mode: 'lexical', limit: 20, maxBytes: 16384, fullChunks: false };
    assert.deepEqual(parseWebviewMessage(search), search);
    assert.ok(parseWebviewMessage({ ...search, pathPrefix: 'src/auth/' }));
    const rejected: unknown[] = [
        null, [], 'search.run', { ...search, v: 2 }, { ...search, type: 'shell.exec' }, { ...search, requestId: 'has space' },
        { ...search, extra: true }, { ...search, command: 'workbench.action.reloadWindow' }, { ...search, limit: 101 },
        { ...search, limit: 1.5 }, { ...search, mode: 'semantic' }, { ...search, query: '' }, { ...search, query: 'a\u0000b' },
        { ...search, fullChunks: 'false' }, (({ maxBytes: _omit, ...rest }) => rest)(search),
        { v: 1, type: 'project.select', requestId: 'r', project: 'ABC' },
        { v: 1, type: 'project.select', requestId: 'r', project: '0'.repeat(63) },
        { v: 1, type: 'source.open', requestId: 'r', ref: '../../etc/passwd' },
        { v: 1, type: 'source.open', requestId: 'r', ref: 'h1', path: 'C:/x' },
    ];
    for (const message of rejected) { assert.equal(parseWebviewMessage(message), undefined, JSON.stringify(message)); }
    assert.ok(parseWebviewMessage({ v: 1, type: 'project.select', requestId: 'r', project: 'a'.repeat(64) }));
    assert.ok(parseWebviewMessage({ v: 1, type: 'docs.facts', requestId: 'd', kind: 'requirement', limit: 10 }));
    assert.equal(parseWebviewMessage({ v: 1, type: 'docs.facts', requestId: 'd', kind: 'secrets', limit: 10 }), undefined);
    assert.equal(parseWebviewMessage({ v: 1, type: 'code.relationships', requestId: 'c', symbol: 's1', direction: 'sideways', hops: 1, limit: 5 }), undefined);
    assert.equal(parseWebviewMessage({ v: 1, type: 'code.relationships', requestId: 'c', symbol: 's1', direction: 'incoming', hops: 4, limit: 5 }), undefined);
    assert.ok(parseWebviewMessage({ v: 1, type: 'code.outline', requestId: 'o' }));
    for (const fields of Object.values(WEBVIEW_MESSAGES)) {
        for (const [name, spec] of Object.entries(fields as Record<string, { kind: string }>)) {
            // A path- or command-like field is acceptable only as a fixed allowlist (enum) or the relative path scope.
            assert.ok(!/path$|command|args|body/i.test(name) || name === 'pathPrefix' || spec.kind === 'enum', `message field ${name} could carry a path or command`);
        }
    }
});

test('Management actions are exactly the console allowlist, and commands are fixed AIRK commands', () => {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const view = require('../serviceConsoleView') as typeof import('../serviceConsoleView');
    const consoleActions = new Set([...view.CONSOLE_SIMPLE_ACTIONS, ...view.CONSOLE_PROJECT_ACTIONS, 'select', 'repair', 'resumeLegacyStartup', 'retryJob', 'cancelJob', 'retryIntent', 'forgetIntent', 'forceIntent']);
    assert.deepEqual(new Set(MANAGE_ACTIONS), consoleActions);
    assert.deepEqual(WEBVIEW_MESSAGES['manage.action'].operation.values, view.CONSOLE_OPERATIONS);
    for (const command of MANAGE_COMMANDS) { assert.match(command, /^airk\.[a-zA-Z]+$/); }
    assert.equal(parseWebviewMessage({ v: 1, type: 'manage.command', requestId: 'c', command: 'workbench.action.reloadWindow' }), undefined);
    assert.equal(parseWebviewMessage({ v: 1, type: 'manage.action', requestId: 'a', action: 'index', project: 'x' }), undefined);
    assert.ok(parseWebviewMessage({ v: 1, type: 'manage.action', requestId: 'a', action: 'retryJob', id: 'ab'.repeat(32) }));
    assert.equal(parseWebviewMessage({ v: 1, type: 'manage.action', requestId: 'a', action: 'retryJob', id: 'zz'.repeat(32) }), undefined);
});
