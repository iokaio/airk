// SPDX-License-Identifier: Apache-2.0
// Standalone Admin launches through the verified airkctl image.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { ADMIN_PAGES } from '../explorerContracts';
import { AdminOpener, adminArgs } from '../airkctl/features/adminOpener';
import type { InvokeResult } from '../airkctl/session';

const index = 'idx_' + '12'.repeat(16), set = 'prj_' + '34'.repeat(16);
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

test('standalone Admin invokes the browser launcher with the reviewed page', async () => {
    const calls: string[][] = [];
    const opener = new AdminOpener({
        open: async args => {
            calls.push([...args]);
            return { exitCode: 0, class: 'success', stdout: '', stderr: '' };
        },
        log: () => {},
    });
    await opener.open({ page: 'index-library', showIndex: index });
    assert.deepEqual(calls, [['--page', 'index-library', '--show-index', index]]);
});

test('a failed launcher does not retain its stdout', async () => {
    const opener = new AdminOpener({
        open: async () => ({ exitCode: 4, class: 'refused', stdout: 'sensitive stdout', stderr: '' }),
        log: () => {},
    });
    await assert.rejects(opener.open({ page: 'overview' }), (error: { result?: InvokeResult }) => {
        assert.equal(error.result?.stdout, '');
        return true;
    });
});
