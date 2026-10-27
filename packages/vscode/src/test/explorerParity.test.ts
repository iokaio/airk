// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';
import { COMMAND_ROUTES } from '../explorerCommands';
import { MANAGE_ACTIONS } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';
import { parseRoute, routeKey } from '../explorerRoutes';

const root = path.resolve(__dirname, '..', '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const contributed = new Set<string>(manifest.contributes.commands.map((entry: { command: string }) => entry.command));
const source = (name: string) => fs.readFileSync(path.join(root, 'src', name), 'utf8');

test('every former Service Console command opens an implemented Explorer screen', () => {
    for (const [name, target] of Object.entries(COMMAND_ROUTES)) {
        assert.ok(contributed.has(`airk.${name}`), `airk.${name} is contributed`);
        const route = parseRoute(target.route);
        assert.ok(route, `${name} → ${target.route} is a manifest route`);
        assert.equal(FEATURES[routeKey(route!)], 'implemented', `${name} lands on an implemented screen`);
        if (target.action) { assert.ok((MANAGE_ACTIONS as readonly string[]).includes(target.action), `${name} runs an allowlisted console action`); }
    }
});

test('the Service Console view is gone: no view, setting, focus command or HTML renderer remains', () => {
    for (const file of ['serviceExtension.ts', 'serviceConsole.ts', 'explorerController.ts', 'explorerSidebar.ts']) {
        assert.doesNotMatch(source(file), /console\.command\(|airk\.service\.focus|registerWebviewViewProvider\('airk\.service'/, file);
    }
    assert.doesNotMatch(source('serviceConsoleView.ts'), /renderConsole|consoleContent|<button/);
    assert.equal(manifest.contributes.views.airk.find((entry: { id: string }) => entry.id === 'airk.service'), undefined);
    const properties = (Array.isArray(manifest.contributes.configuration) ? manifest.contributes.configuration[0] : manifest.contributes.configuration).properties;
    assert.equal(properties['airk.explorer.showServiceConsole'], undefined);
});

test('commands that keep their own flow still exist alongside the routed ones', () => {
    for (const name of ['index', 'indexMode', 'pause', 'resume', 'reindex', 'diagnose', 'currentMaintenance', 'status', 'usage', 'setupAgent', 'verifyMcp',
        'manageVscodeMcp', 'installCli', 'openTools', 'chooseStorage', 'explorer.open', 'explorer.search']) {
        assert.ok(contributed.has(`airk.${name}`), name);
    }
});
