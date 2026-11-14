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
const CONTRACTS = new Set(['adminUiContract', 'explorerContracts', 'uiPilotContract', 'explorerRoutes', 'explorerFeatures', 'explorerErrors', 'explorerStatus', 'explorerStrategies', 'explorerDrafts', 'indexingCatalog', 'indexingSplit', 'catalogSession', 'publications']);
const WEBVIEW_PACKAGES = new Set(['preact', 'preact/hooks', 'preact/jsx-runtime', 'airk-explorer-help']);
const ASSISTANCE_BUILTINS = new Set(['node:crypto', 'node:fs', 'node:fs/promises', 'node:net', 'node:os', 'node:path', 'node:process', 'node:readline']);
const HOST_MARKERS = ['class ServiceClient', 'class NativeControlTransport', 'class NativeSession', 'class IntentJournal', 'class PrivateIntentStore', 'function probeMcp', 'class ServiceConsoleController', 'class RuntimeHost'];

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
function typeOnly(from: string, specifier: string): boolean {
    const tree = ts.createSourceFile(from, fs.readFileSync(from, 'utf8'), ts.ScriptTarget.Latest, true);
    return tree.statements.some(statement => ts.isImportDeclaration(statement) && ts.isStringLiteral(statement.moduleSpecifier)
        && statement.moduleSpecifier.text === specifier && statement.importClause?.isTypeOnly === true);
}

test('the standalone uninstall hook bundles only itself and Node built-ins (plan §8.5)',async()=>{
    const result=await bundle({absWorkingDir:path.dirname(source),entryPoints:['src/uninstallHook.ts'],bundle:true,write:false,metafile:true,
        platform:'node',format:'cjs',target:'node20',mainFields:['module','main'],logLevel:'silent'});
    const inputs=Object.keys(result.metafile!.inputs).map(name=>name.replaceAll('\\','/'));
    assert.deepEqual(inputs,['src/uninstallHook.ts'],'the hook removes editor-owned storage only; no native runner, agent or service module');
});

test('Explorer contract modules depend only on other contract modules', () => {
    for (const name of CONTRACTS) {
        const file = path.join(source, name + '.ts');
        if (!fs.existsSync(file)) { continue; }
        for (const specifier of imports(file)) {
            const generated = name === 'uiPilotContract' && specifier === './generated/i2' && typeOnly(file, specifier);
            assert.ok(contract(file, specifier) || generated, `${name} imports ${specifier}; contracts must stay free of Node, vscode and host modules`);
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
    // Rust I2 owns caller binding/selection types; this import emits no runtime dependency.
    assert.deepEqual(imports(file),['node:util','./generated/i2','./catalogSession']);
    assert.equal(typeOnly(file,'./generated/i2'),true);
    assert.ok(imports(path.join(source,'explorerEvidence.ts')).includes('./readRetry'));
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
            // The approved native management consumer reuses the one Admin
            // presentation and its credential-free native lifetime facade.
            // This exception belongs only to that named consumer, never to
            // arbitrary editor views or any Node/host implementation.
            const management = path.basename(file) === 'sharedManagement.tsx' && (
                ['../../ui/src/management', '../../ui/src/nativeSessionLifetime', '../../admin/src/session', '../../admin/src/envelope'].includes(target(file, specifier))
                || target(file, specifier) === '../../ui/src/apiPort' && typeOnly(file, specifier)
                || specifier === 'airk-shared-admin-css');
            const shared = target(file, specifier) === '../../ui/src/pilotPresentation'
                || target(file, specifier) === '../../ui/src/helpSearch'
                || target(file, specifier) === '../../ui/src/helpMarkdown'
                || target(file, specifier) === '../../ui/src/helpToolPresentation'
                || target(file, specifier) === '../../ui/src/navigation'
                || target(file, specifier) === '../../ui/generated/help-manifest.json' && typeOnly(file, specifier)
                || target(file, specifier) === '../../ui/src/pilotContract' && typeOnly(file, specifier);
            const allowed = WEBVIEW_PACKAGES.has(specifier) || within(file, specifier, 'explorerWebview') || contract(file, specifier) || shared || management;
            assert.ok(allowed, `${path.relative(source, file)} imports ${specifier}`);
        }
    }
});

test('the shared pilot presenter has no browser, native or Preact runtime dependency', () => {
    const presenter = path.resolve(source, '../../ui/src/pilotPresentation.ts');
    assert.deepEqual(imports(presenter), ['./pilotContract', './pilotProjection']);
    assert.equal(typeOnly(presenter, './pilotContract'), true);
    const projector = path.resolve(source, '../../ui/src/pilotProjection.ts');
    assert.deepEqual(imports(projector), ['./pilotContract']);
    assert.equal(typeOnly(projector, './pilotContract'), true);
    const forbidden = new Set(['window', 'document', 'fetch', 'require', 'dangerouslySetInnerHTML', 'innerHTML']);
    for (const file of [presenter, projector]) {
        const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
        const visit = (node: ts.Node): void => {
            // Native payload properties such as response.document are data, not DOM globals.
            const property = node.parent !== undefined && ts.isPropertyAccessExpression(node.parent) && node.parent.name === node;
            if (ts.isIdentifier(node) && !property) assert.ok(!forbidden.has(node.text), node.text);
            ts.forEachChild(node, visit);
        };
        visit(tree);
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

// Process starts (split plan §4.4 "one spawning module"): only src/airkctl/
// starts a process. Every production site is listed here, reviewed, with its reason.
const PROCESS_STARTS: Record<string,string> = {
    'airkctl/runner.ts#spawnBounded:spawn': 'verified airkctl one-shots, and the platform verification tools run before the first airkctl execution (Windows PowerShell for Authenticode and ACLs, macOS codesign)',
    'airkctl/session.ts#launchSession:spawn': 'the one per-window `airkctl session --owner-watch` child',
    // Not constructed by the split composition root; they leave with the Explorer
    // management and Assistance untangling (follow-up of split Phase 6).
    'serviceClient.ts#NativeControlTransport.constructor:spawn': 'unreachable at run time: legacy per-client console transport',
    'privateDirectory.ts#run:execFile': 'unreachable at run time: ACL tool of the retired window-hosted Assistance pipe',
};
const CHILD_PROCESS = new Set(['spawn', 'spawnSync', 'exec', 'execSync', 'execFile', 'execFileSync', 'fork']);
function processStarts(): string[] {
    const sites: string[] = [];
    for (const file of sources(source).filter(name => !path.relative(source, name).startsWith('test' + path.sep))) {
        const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true, file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
        const name = path.relative(source, file).split(path.sep).join('/');
        const named = new Map<string, string>(), namespaces = new Set<string>();
        for (const statement of tree.statements) {
            if (!ts.isImportDeclaration(statement) || !statement.importClause || !ts.isStringLiteral(statement.moduleSpecifier)) { continue; }
            const specifier = statement.moduleSpecifier.text, bindings = statement.importClause.namedBindings;
            const startsProcesses = specifier === 'node:child_process' || specifier === 'child_process';
            if (startsProcesses && bindings && ts.isNamespaceImport(bindings)) { namespaces.add(bindings.name.text); }
            if (bindings && ts.isNamedImports(bindings)) {
                for (const element of bindings.elements) {
                    const original = (element.propertyName ?? element.name).text;
                    if (startsProcesses && CHILD_PROCESS.has(original) || specifier === './native' && ['runText', 'runSystemText'].includes(original)) { named.set(element.name.text, original); }
                }
            }
        }
        if (name === 'native.ts') { named.set('runText', 'runText'); named.set('runSystemText', 'runSystemText'); }
        const owner = (node: ts.Node) => {
            const names: string[] = [];
            for (let current = node.parent; current; current = current.parent) {
                if ((ts.isFunctionDeclaration(current) || ts.isMethodDeclaration(current)) && current.name) { names.unshift(current.name.getText()); }
                else if (ts.isConstructorDeclaration(current)) { names.unshift('constructor'); }
                else if (ts.isVariableDeclaration(current) && ts.isIdentifier(current.name) && current.initializer && (ts.isArrowFunction(current.initializer) || ts.isFunctionExpression(current.initializer))) { names.unshift(current.name.text); }
                else if (ts.isClassDeclaration(current) && current.name) { names.unshift(current.name.text); }
            }
            return names.join('.') || '<module>';
        };
        const visit = (node: ts.Node): void => {
            if (ts.isCallExpression(node)) {
                const callee = node.expression;
                const started = ts.isIdentifier(callee) ? named.get(callee.text)
                    : ts.isPropertyAccessExpression(callee) && ts.isIdentifier(callee.expression) && namespaces.has(callee.expression.text) && CHILD_PROCESS.has(callee.name.text) ? callee.name.text
                        : ts.isPropertyAccessExpression(callee) && callee.name.text === 'createTerminal' ? 'createTerminal' : undefined;
                if (started) { sites.push(`${name}#${owner(node)}:${started}`); }
            }
            ts.forEachChild(node, visit);
        };
        visit(tree);
    }
    return sites.sort();
}

test('production TypeScript starts processes only at the reviewed one-process sites', () => {
    const sites = processStarts();
    assert.ok(sites.length > 0, 'zero observed sites means the scan selected nothing');
    assert.deepEqual(sites, Object.keys(PROCESS_STARTS).sort(), 'a new process start needs review: route AIRK commands through the window session');
});

test('the composition root and the airkctl facade open no network transport and read no service credential', () => {
    const files = [path.join(source, 'serviceExtension.ts'), ...sources(path.join(source, 'airkctl'))];
    assert.ok(files.length >= 10, 'zero or few selected files is a failure');
    for (const file of files) {
        const text = fs.readFileSync(file, 'utf8');
        for (const specifier of imports(file)) {
            assert.ok(!/^(?:node:)?(?:net|tls|http|https|http2)$/.test(specifier), `${path.relative(source, file)} imports ${specifier}`);
        }
        assert.doesNotMatch(text, /\\\\\.\\pipe|127\.0\.0\.1:\d|http-1[\\/]|issuer\.pem|ca\.pem|AIRK_MCP_TOKEN_[0-9A-F]/, path.relative(source, file));
    }
});
