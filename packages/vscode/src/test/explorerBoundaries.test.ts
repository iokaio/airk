// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import test from 'node:test';
import * as ts from 'typescript';
import {build as bundle} from 'esbuild';

// Import boundaries for the AIRK Explorer. The repository has no linter, so this
// test reads the TypeScript sources directly (out/test → ../../src).
const source = path.resolve(__dirname, '..', '..', 'src');
const compiled = path.resolve(__dirname, '..');
const CONTRACTS = new Set(['explorerContracts', 'explorerRoutes', 'explorerFeatures', 'explorerErrors', 'explorerStatus', 'explorerStrategies', 'explorerDrafts', 'indexingCatalog', 'indexingSplit', 'catalogSession', 'publications']);
const WEBVIEW_PACKAGES = new Set(['preact', 'preact/hooks', 'preact/jsx-runtime', 'airk-explorer-help']);
const ASSISTANCE_BUILTINS = new Set(['node:crypto', 'node:fs', 'node:fs/promises', 'node:net', 'node:os', 'node:path', 'node:process', 'node:readline']);
const HOST_MARKERS = ['class ServiceClient', 'class NativeControlTransport', 'class IntentJournal', 'class PrivateIntentStore', 'function probeMcp', 'class ServiceConsoleController', 'class RuntimeHost'];

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

test('standalone uninstall configuration transforms do not pull in host config discovery',async()=>{
    const pure=path.join(source,'unifiedAgentConfig.ts');
    assert.deepEqual(imports(pure).sort(),['./agentToml','./model','jsonc-parser']);
    const result=await bundle({absWorkingDir:path.dirname(source),entryPoints:['src/uninstallHook.ts'],bundle:true,write:false,metafile:true,
        platform:'node',format:'cjs',target:'node20',mainFields:['module','main'],logLevel:'silent'});
    const inputs=Object.keys(result.metafile!.inputs).map(name=>name.replaceAll('\\','/'));
    assert.ok(inputs.includes('src/unifiedAgentConfig.ts'),'the real uninstall bundle exercises the shared pure transforms');
    assert.ok(inputs.includes('src/nativeOutcome.ts'),'uninstall retains the shared error class and complete native outcome');
    assert.deepEqual(imports(path.join(source,'nativeOutcome.ts')),[],'shared error metadata must remain inert and transport-free');
    for(const forbidden of ['privateIntentStore','serviceIntents','intentAcceptedEvidence','serviceConsoleController'])
        assert.equal(inputs.includes(`src/${forbidden}.ts`),false,`${forbidden} must not enter the standalone worker`);
    assert.equal(inputs.includes('src/globalAgentInventory.ts'),false,'host inventory must not become a standalone uninstall dependency');
    assert.equal(inputs.includes('src/serviceExtension.ts'),false);
});

test('Explorer contract modules depend only on other contract modules', () => {
    for (const name of CONTRACTS) {
        const file = path.join(source, name + '.ts');
        if (!fs.existsSync(file)) { continue; }
        for (const specifier of imports(file)) {
            assert.ok(contract(file, specifier), `${name} imports ${specifier}; contracts must stay free of Node, vscode and host modules`);
        }
    }
});

test('bundle and source audits retain the same exact pure contracts including the error registry',()=>{
    const build=fs.readFileSync(path.resolve(source,'..','scripts','build-explorer.cjs'),'utf8');
    const declaration=/const CONTRACTS = \[([^\]]+)\];/.exec(build)?.[1];
    assert.ok(declaration,'bundle has an explicit contract list');
    const bundled=[...declaration.matchAll(/'src\/([A-Za-z]+)\.ts'/g)].map(match=>match[1]).sort();
    assert.deepEqual(bundled,[...CONTRACTS].sort());
    const file=path.join(source,'explorerErrors.ts');
    assert.deepEqual(imports(file),[],'the error registry has no host, browser or Node imports');
    const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
    for(const statement of tree.statements){
        assert.ok(ts.isTypeAliasDeclaration(statement)||ts.isInterfaceDeclaration(statement)||ts.isFunctionDeclaration(statement)||ts.isVariableStatement(statement),
            'the error registry contains only types, constants and inert function bodies');
        if(ts.isVariableStatement(statement))for(const declaration of statement.declarationList.declarations){
            const initializer=declaration.initializer;
            assert.ok(initializer&&ts.isSatisfiesExpression(initializer)&&ts.isAsExpression(initializer.expression)
                &&ts.isObjectLiteralExpression(initializer.expression.expression),'the top-level error registry is a literal constant');
        }
    }
});

test('the shared Indexing contract adds no host/browser dependencies to the uninstall worker',()=>{
    const file=path.join(source,'indexingCatalog.ts');
    assert.deepEqual(imports(file),[],'inert validators must not acquire Node, editor, controller or browser imports');
    const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
    for(const statement of tree.statements){
        assert.ok(ts.isInterfaceDeclaration(statement)||ts.isTypeAliasDeclaration(statement)||ts.isFunctionDeclaration(statement)||ts.isVariableStatement(statement),
            'shared contract contains only declarations, constants and inert function bodies');
        if(ts.isVariableStatement(statement))for(const declaration of statement.declarationList.declarations){
            const initializer=declaration.initializer;
            assert.ok(!initializer||ts.isNumericLiteral(initializer)||ts.isStringLiteral(initializer)||ts.isBinaryExpression(initializer)||ts.isAsExpression(initializer),
                'contract constants cannot perform startup work');
        }
    }
});

test('the standalone uninstall bundle retains no Indexing controller, Preact or browser implementation', {skip:!fs.existsSync(path.join(compiled,'uninstallHook.js'))},()=>{
    const text=fs.readFileSync(path.join(compiled,'uninstallHook.js'),'utf8');
    assert.doesNotMatch(text,/\bclass\s+IndexingCatalogController\b|src\/explorerWebview\//);
    assert.doesNotMatch(text,/\bclass\s+WindowScopeSelection\b|src\/scopeSelection\.ts/);
    assert.doesNotMatch(text,/\bclass\s+EvidenceClient\b|src\/explorerEvidence\.ts|src\/explorerErrors\.ts|src\/nativeMetadata\.ts/);
    assert.doesNotMatch(text,/\brequire\s*\(\s*["'](?:preact(?:\/[^"']*)?|vscode)["']/);
    assert.doesNotMatch(text,/\b(?:window|document)\.(?:addEventListener|createElement|querySelector|body)\b/);
});

test('native read retry validators retain a pure shared dependency boundary',()=>{
    const file=path.join(source,'readRetry.ts');
    assert.deepEqual(imports(file),['node:util','./catalogSession']);
    for(const name of ['native','explorerEvidence']){
        const dependencies=imports(path.join(source,name+'.ts'));
        assert.ok(dependencies.includes('./readRetry'));
        if(name==='native')assert.ok(!dependencies.includes('./explorerEvidence'));
    }
    const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
    for(const statement of tree.statements)assert.ok(ts.isImportDeclaration(statement)||ts.isFunctionDeclaration(statement),
        'shared retry validators perform no startup work');
});

test('catalog launch model imports only the pure selector contract and keeps host selection out of uninstall',()=>{
    const model=path.join(source,'model.ts'),selector=path.join(source,'catalogSession.ts');
    assert.ok(imports(model).includes('./catalogSession'));
    assert.ok(!imports(model).includes('./scopeSelection'));
    assert.deepEqual(imports(selector),['./indexingCatalog']);
    const tree=ts.createSourceFile(selector,fs.readFileSync(selector,'utf8'),ts.ScriptTarget.Latest,true);
    for(const statement of tree.statements)assert.ok(ts.isImportDeclaration(statement)||ts.isInterfaceDeclaration(statement)||ts.isFunctionDeclaration(statement),
        'selector contract contains only imports, DTO declarations and inert validator functions');
    const build=fs.readFileSync(path.resolve(source,'..','scripts','build-explorer.cjs'),'utf8');
    assert.ok(build.includes("'src/catalogSession.ts'"));
    assert.ok(!build.includes("'src/scopeSelection.ts'"),'host selection controller must never be allowlisted in bundles');
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
