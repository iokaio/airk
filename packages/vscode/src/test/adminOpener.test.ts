// SPDX-License-Identifier: Apache-2.0
// Open AIRK Admin (U97): the `airkctl admin open|url` argument construction,
// the remembered "In VS Code" / "In my browser" choice, and the rule that the
// single-use sign-in URL is never logged, kept or reused.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { ADMIN_PAGES } from '../explorerContracts';
import { AdminOpener, adminArgs, adminUrl, type AdminOpenerHost, type AdminOpenIn } from '../airkctl/features/adminOpener';
import type { InvokeResult } from '../airkctl/session';

const index = 'idx_' + '12'.repeat(16), set = 'prj_' + '34'.repeat(16);
const URL_SECRET = 'https://127.0.0.1:47123/admin/launch#c=one-time-sign-in-code&route=index-library';
const ok = (stdout = ''): InvokeResult => ({ exitCode: 0, class: 'success', stdout, stderr: 'warning: this URL signs in to AIRK Admin' });

function host(openIn: AdminOpenIn, overrides: Partial<AdminOpenerHost> = {}) {
    const calls: { open: string[][]; url: string[][]; shown: string[]; offers: string[]; logs: string[]; remembered: AdminOpenIn[] } = { open: [], url: [], shown: [], offers: [], logs: [], remembered: [] };
    let setting = openIn;
    const value: AdminOpenerHost = {
        open: async args => { calls.open.push([...args]); return ok(); },
        url: async args => { calls.url.push([...args]); return ok(JSON.stringify({ url: URL_SECRET })); },
        setting: () => setting,
        remember: async choice => { calls.remembered.push(choice); setting = choice; },
        choose: async () => undefined,
        showInVsCode: async url => { calls.shown.push(url); },
        offerBrowser: async message => { calls.offers.push(message); return false; },
        log: line => { calls.logs.push(line); },
        ...overrides,
    };
    return { value, calls };
}
const neverLogged = (calls: { logs: string[] }) => {
    for (const line of calls.logs) { assert.ok(!line.includes('one-time-sign-in-code') && !line.includes('https://'), `URL logged: ${line}`); }
};

test('admin arguments name the page and at most one index-library prefill, in airkctl flag form', () => {
    for (const page of ADMIN_PAGES) { assert.deepEqual(adminArgs({ page }), ['--page', page]); }
    assert.deepEqual(adminArgs({ page: 'index-library', forFolder: 'C:\\Users\\me\\My Docs' }), ['--page', 'index-library', '--for-folder', 'C:\\Users\\me\\My Docs']);
    assert.deepEqual(adminArgs({ page: 'index-library', showIndex: index }), ['--page', 'index-library', '--show-index', index]);
    assert.deepEqual(adminArgs({ page: 'index-library', showSet: set }), ['--page', 'index-library', '--show-set', set]);
    for (const bad of [
        { page: 'settings' }, { page: 'overview', forFolder: '/src' }, { page: 'index-library', showIndex: set }, { page: 'index-library', showSet: index },
        { page: 'index-library', forFolder: '/src', showIndex: index }, { page: 'index-library', forFolder: '--page' }, { page: 'index-library', forFolder: '' },
    ]) { assert.throws(() => adminArgs(bad as never), /AIRK Admin cannot open this page/, JSON.stringify(bad)); }
});

test('only an https URL from admin url --output json is accepted', () => {
    assert.equal(adminUrl(JSON.stringify({ url: URL_SECRET })), URL_SECRET);
    for (const stdout of [JSON.stringify({ url: 'http://127.0.0.1:1/admin/launch#c=x' }), JSON.stringify({ url: 'javascript:alert(1)' }), JSON.stringify({ link: URL_SECRET }), 'not json', '']) {
        assert.equal(adminUrl(stdout), undefined, stdout);
    }
});

test('external opening runs airkctl admin open with the page and prefill and never asks for a URL', async () => {
    const { value, calls } = host('external');
    assert.equal(await new AdminOpener(value).open({ page: 'index-library', forFolder: '/work/app' }), 'external');
    assert.deepEqual(calls.open, [['--page', 'index-library', '--for-folder', '/work/app']]);
    assert.deepEqual(calls.url, []); assert.deepEqual(calls.shown, []); assert.deepEqual(calls.remembered, []);
    const failed = host('external', { open: async () => ({ exitCode: 3, class: 'service_missing', stdout: '', stderr: '' }) });
    await assert.rejects(new AdminOpener(failed.value).open({ page: 'overview' }), /exited with service_missing/);
});

test('opening in VS Code passes the single-use URL only to the Simple Browser and never logs it', async () => {
    const { value, calls } = host('vscode');
    const opener = new AdminOpener(value);
    assert.equal(await opener.open({ page: 'index-library', showIndex: index }), 'vscode');
    await opener.followUp;
    assert.deepEqual(calls.url, [['--page', 'index-library', '--show-index', index]]);
    assert.deepEqual(calls.shown, [URL_SECRET]); assert.deepEqual(calls.open, []);
    assert.match(calls.offers[0], /certificate problem/, 'a certificate failure can still be sent to the browser');
    neverLogged(calls);
    for (const message of calls.offers) { assert.ok(!message.includes('one-time-sign-in-code')); }
});

test('a certificate report or a failed Simple Browser opens a fresh browser sign-in, never the same URL', async () => {
    const reported = host('vscode', { offerBrowser: async () => true });
    const opener = new AdminOpener(reported.value);
    await opener.open({ page: 'activity' }); await opener.followUp;
    assert.deepEqual(reported.calls.open, [['--page', 'activity']], 'the browser fallback asks airkctl for its own sign-in');
    neverLogged(reported.calls);

    const broken = host('vscode', { showInVsCode: async () => { throw new Error('command simpleBrowser.show not found'); } });
    broken.value.offerBrowser = async message => { broken.calls.offers.push(message); return true; };
    await new AdminOpener(broken.value).open({ page: 'storage' });
    assert.deepEqual(broken.calls.open, [['--page', 'storage']]); assert.match(broken.calls.offers[0], /could not be opened in VS Code/);
    neverLogged(broken.calls);

    const declined = host('vscode', { showInVsCode: async () => { throw new Error('no browser'); } });
    await new AdminOpener(declined.value).open({ page: 'storage' });
    assert.deepEqual(declined.calls.open, [], 'nothing opens unless the user chooses the browser');
});

test('plain http and failed url commands never reach the Simple Browser and keep no URL in the error', async () => {
    const http = host('vscode', { url: async () => ok(JSON.stringify({ url: 'http://127.0.0.1:1/admin/launch#c=one-time-sign-in-code' })) });
    await new AdminOpener(http.value).open({ page: 'overview' });
    assert.deepEqual(http.calls.shown, []); assert.match(http.calls.offers[0], /no HTTPS sign-in link/); neverLogged(http.calls);

    const failed = host('vscode', { url: async () => ({ exitCode: 4, class: 'refused', stdout: JSON.stringify({ url: URL_SECRET }), stderr: '' }) });
    await assert.rejects(new AdminOpener(failed.value).open({ page: 'overview' }), (error: { result?: InvokeResult }) => {
        assert.equal(error.result?.stdout, '', 'a failed result keeps no stdout'); return true;
    });
    assert.deepEqual(failed.calls.shown, []);
});

test('Choose where asks In VS Code or In my browser, remembers the answer and uses it next time', async () => {
    let asked = 0;
    const chosen = host('external', { choose: async current => { asked++; assert.equal(current, 'external'); return 'vscode'; } });
    const opener = new AdminOpener(chosen.value);
    await opener.open({ page: 'agents' }, true); await opener.followUp;
    assert.equal(asked, 1); assert.deepEqual(chosen.calls.remembered, ['vscode']); assert.deepEqual(chosen.calls.url, [['--page', 'agents']]);
    await opener.open({ page: 'agents' }); await opener.followUp;
    assert.equal(asked, 1, 'the remembered choice is used without asking'); assert.equal(chosen.calls.url.length, 2);

    const dismissed = host('vscode');
    assert.equal(await new AdminOpener(dismissed.value).open({ page: 'overview' }, true), undefined);
    assert.deepEqual(dismissed.calls.remembered, []); assert.deepEqual(dismissed.calls.url, []); assert.deepEqual(dismissed.calls.open, []);
});
