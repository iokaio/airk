// SPDX-License-Identifier: Apache-2.0
// Bundles the AIRK Explorer webview into flat out/ files. Host modules stay
// tsc-compiled CommonJS; bundles may only contain allowlisted inputs so host
// classes (service clients, journals, MCP sessions) never ship a second copy.
const esbuild = require('esbuild');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
// Pure contract modules shared by the host, the webview and the Assistance server.
const CONTRACTS = ['src/explorerContracts.ts', 'src/explorerRoutes.ts', 'src/explorerFeatures.ts', 'src/explorerErrors.ts', 'src/explorerStatus.ts'];

function preactBanner() {
    const manifest = JSON.parse(read('package.json'));
    const preact = JSON.parse(read('node_modules/preact/package.json'));
    if (preact.name !== 'preact' || preact.version !== manifest.devDependencies.preact || preact.license !== 'MIT') {
        throw new Error('Unexpected Preact identity; expected the pinned MIT devDependency.');
    }
    const license = read('node_modules/preact/LICENSE').trim();
    if (license.includes('*/') || !license.includes('Permission is hereby granted')) { throw new Error('Unexpected Preact license text.'); }
    return `/*! AIRK Explorer webview. Includes Preact ${preact.version} (MIT).\n${license}\n*/`;
}

function checkInputs(name, metafile, allowed) {
    for (const input of Object.keys(metafile.inputs)) {
        const normalized = input.replaceAll('\\', '/');
        if (!allowed.some(rule => typeof rule === 'string' ? normalized === rule : rule.test(normalized))) {
            throw new Error(`${name} bundle input outside the allowlist: ${normalized}`);
        }
    }
}

async function build() {
    const common = { absWorkingDir: root, bundle: true, metafile: true, sourcemap: false, legalComments: 'none', logLevel: 'warning', charset: 'utf8' };
    const webview = await esbuild.build({
        ...common,
        entryPoints: ['src/explorerWebview/main.tsx'],
        outfile: 'out/explorerWebview.js',
        format: 'iife',
        platform: 'browser',
        target: 'es2022',
        jsx: 'automatic',
        jsxImportSource: 'preact',
        banner: { js: preactBanner() },
    });
    checkInputs('Webview', webview.metafile, [/^src\/explorerWebview\//, ...CONTRACTS, /^node_modules\/preact\//]);
}

build().catch(error => { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; });
