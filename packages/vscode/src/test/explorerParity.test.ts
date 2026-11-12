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

test('former Service Console commands are retired to the admin pages; their Explorer routes still parse', () => {
    // Plan §8.5: indexing, storage, service and recovery commands become admin links.
    for (const [name, target] of Object.entries(COMMAND_ROUTES)) {
        // Start and Connect are retained guidance ids (plan §8.1): they run airkctl, not a console route.
        assert.equal(contributed.has(`airk.${name}`), ['serviceStart', 'serviceConnect'].includes(name), `airk.${name} is retired or guidance only`);
        const route = parseRoute(target.route);
        assert.ok(route, `${name} → ${target.route} is a manifest route`);
        assert.equal(FEATURES[routeKey(route!)], 'implemented', `${name} lands on an implemented screen`);
        if (target.action) { assert.ok((MANAGE_ACTIONS as readonly string[]).includes(target.action), `${name} runs an allowlisted console action`); }
    }
    assert.ok(contributed.has('airk.openAdmin'), 'management opens through airkctl admin open');
});

test('the Service Console view is gone: no view, setting, focus command or HTML renderer remains', () => {
    for (const file of ['serviceExtension.ts', 'explorerController.ts', 'explorerSidebar.ts']) {
        assert.doesNotMatch(source(file), /console\.command\(|airk\.service\.focus|registerWebviewViewProvider\('airk\.service'/, file);
    }
    assert.doesNotMatch(source('serviceConsoleView.ts'), /renderConsole|consoleContent|<button/);
    assert.equal(manifest.contributes.views.airk.find((entry: { id: string }) => entry.id === 'airk.service'), undefined);
    const properties = (Array.isArray(manifest.contributes.configuration) ? manifest.contributes.configuration[0] : manifest.contributes.configuration).properties;
    assert.equal(properties['airk.explorer.showServiceConsole'], undefined);
});

test('the split command set: guidance, agent connections, Explorer and Workbench; no install or management command', () => {
    // Reviewed, checked-in list (plan §8.1, §8.5).
    assert.deepEqual([...contributed].sort(), ['airk.checkAgain', 'airk.downloadService', 'airk.enableNativeExecution', 'airk.explorer.open', 'airk.explorer.search',
        'airk.explorer.showSuggestions', 'airk.httpCredential', 'airk.installationInstructions', 'airk.manageVscodeMcp', 'airk.manageWorkspaceTrust', 'airk.openAdmin',
        'airk.reconnectAgents', 'airk.serviceConnect', 'airk.serviceStart', 'airk.setupAgent', 'airk.showExtensionOutput', 'airk.workbench.clearSessions',
        'airk.workbench.focus', 'airk.workbench.newSession', 'airk.workbench.searchSelection']);
    const properties = (Array.isArray(manifest.contributes.configuration) ? manifest.contributes.configuration[0] : manifest.contributes.configuration).properties;
    for (const retired of ['airk.cli.autoInstall', 'airk.cli.shellIntegration', 'airk.service.channel', 'airk.service.installDir', 'airk.service.managedSource',
        'airk.service.managedSha256', 'airk.service.managedLifecycle', 'airk.service.managedStateDir', 'airk.storage.path']) {
        assert.equal(properties[retired], undefined, retired);
    }
    assert.ok(!manifest.files.includes('native/**'), 'the VSIX carries no native runtime');
});
