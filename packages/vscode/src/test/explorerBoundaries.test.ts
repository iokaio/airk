// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import test from 'node:test';
import * as ts from 'typescript';

// Import boundaries for the AIRK Explorer. The repository has no linter, so this
// test reads the TypeScript sources directly (out/test → ../../src).
const source = path.resolve(__dirname, '..', '..', 'src');
const compiled = path.resolve(__dirname, '..');
const CONTRACTS = new Set(['explorerContracts', 'explorerRoutes', 'explorerFeatures', 'explorerErrors', 'explorerStatus']);
const WEBVIEW_PACKAGES = new Set(['preact', 'preact/hooks', 'preact/jsx-runtime', 'airk-explorer-help']);
const ASSISTANCE_BUILTINS = new Set(['node:crypto', 'node:fs', 'node:fs/promises', 'node:net', 'node:os', 'node:path', 'node:process', 'node:readline']);
const HOST_MARKERS = ['class ServiceClient', 'class NativeControlTransport', 'class IntentJournal', 'function probeMcp', 'class ServiceConsoleController', 'class RuntimeHost'];

function sources(directory: string): string[] {
    if (!fs.existsSync(directory)) { return []; }
    return fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
        const file = path.join(directory, entry.name);
        return entry.isDirectory() ? sources(file) : /\.tsx?$/.test(entry.name) ? [file] : [];
    });
}
function imports(file: string): string[] {
    return ts.preProcessFile(fs.readFileSync(file, 'utf8'), true, true).importedFiles.map(entry => entry.fileName);
}
/** Source-relative target of a relative import, without extension, using forward slashes. */
function target(from: string, specifier: string): string {
    return path.relative(source, path.resolve(path.dirname(from), specifier)).replaceAll('\\', '/').replace(/\.(tsx?|js)$/, '');
}
const relative = (specifier: string) => specifier.startsWith('./') || specifier.startsWith('../');
const contract = (from: string, specifier: string) => relative(specifier) && CONTRACTS.has(target(from, specifier));
const within = (from: string, specifier: string, directory: string) => relative(specifier) && target(from, specifier).startsWith(directory + '/');

test('Explorer contract modules depend only on other contract modules', () => {
    for (const name of CONTRACTS) {
        const file = path.join(source, name + '.ts');
        if (!fs.existsSync(file)) { continue; }
        for (const specifier of imports(file)) {
            assert.ok(contract(file, specifier), `${name} imports ${specifier}; contracts must stay free of Node, vscode and host modules`);
        }
    }
});

test('Explorer webview sources import only Preact, their own files and contracts', () => {
    const files = sources(path.join(source, 'explorerWebview'));
    assert.ok(files.length > 0, 'webview sources are present');
    for (const file of files) {
        for (const specifier of imports(file)) {
            const allowed = WEBVIEW_PACKAGES.has(specifier) || within(file, specifier, 'explorerWebview') || contract(file, specifier);
            assert.ok(allowed, `${path.relative(source, file)} imports ${specifier}`);
        }
    }
});

test('Explorer Assistance sources never reach host, service or retrieval modules', () => {
    for (const file of sources(path.join(source, 'explorerAssistance'))) {
        for (const specifier of imports(file)) {
            const allowed = ASSISTANCE_BUILTINS.has(specifier) || specifier === 'airk-explorer-help' || within(file, specifier, 'explorerAssistance') || contract(file, specifier);
            assert.ok(allowed, `${path.relative(source, file)} imports ${specifier}; Assistance may not call vscode, the service or retrieval`);
        }
    }
});

test('Host modules load bundles by path, never by importing bundle sources', () => {
    const host = fs.readdirSync(source).filter(name => name.endsWith('.ts')).map(name => path.join(source, name));
    for (const file of host) {
        for (const specifier of imports(file)) {
            assert.ok(!within(file, specifier, 'explorerWebview') && !within(file, specifier, 'explorerAssistance'),
                `${path.relative(source, file)} imports bundle source ${specifier}`);
        }
    }
});

test('Built Explorer bundles contain no host implementation copies', { skip: !fs.existsSync(path.join(compiled, 'explorerWebview.js')) }, () => {
    for (const name of ['explorerWebview.js', 'explorerAssistance.js']) {
        const file = path.join(compiled, name);
        if (!fs.existsSync(file)) { continue; }
        const text = fs.readFileSync(file, 'utf8');
        for (const marker of HOST_MARKERS) { assert.ok(!text.includes(marker), `${name} contains ${marker}`); }
        assert.doesNotMatch(text, /\brequire\s*\(\s*["']vscode["']/, `${name} loads the VS Code API`);
    }
});
