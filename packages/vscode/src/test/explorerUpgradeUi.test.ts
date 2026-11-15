// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import test, { before } from 'node:test';
import { build, type Plugin } from 'esbuild';
import { ROUTE_KEYS, parseRoute } from '../explorerRoutes';

const directory = path.resolve(__dirname, '../../src/explorerWebview');
const bundles = new Map<string, string>();
const {sharedAdminStylePlugin}=require(path.resolve(__dirname,'../../scripts/build-explorer.cjs')) as {sharedAdminStylePlugin():Plugin};
before(async()=>{
    const result=await build({entryPoints:[path.join(directory,'app.tsx')],bundle:true,write:false,
        outfile:path.join(directory,'test-bundle.cjs'),platform:'node',format:'cjs',jsx:'automatic',jsxImportSource:'preact',
        external:['preact','preact/hooks','preact/jsx-runtime','airk-explorer-help'],plugins:[sharedAdminStylePlugin()]});
    const javascript=result.outputFiles.find(file=>file.path.endsWith('.cjs'));assert.ok(javascript);
    bundles.set('app.tsx',javascript.text);
});

function browserModule(name: string, state?: unknown, route?: unknown) {
    const file = path.join(directory, name), nativeRequire = createRequire(file);
    let hook = 0;
    const hooks = {
        useEffect() {},
        useLayoutEffect() {},
        useState(initial: unknown) {
            const next = hook++;
            return [state !== undefined && next === 0 ? state : route !== undefined && next === 1 ? route : typeof initial === 'function' ? initial() : initial, () => undefined];
        },
        useRef: (current: unknown) => ({ current }),
        useMemo: (get: () => unknown) => get(),
    };
    const built = bundles.get(name);
    assert.ok(built,`The real ${name} module must be bundled before its callback checks.`);
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
