// SPDX-License-Identifier: Apache-2.0
// Bundles the AIRK Explorer webview and Assistance server into flat out/ files. Host modules stay
// tsc-compiled CommonJS; bundles may only contain allowlisted inputs so host
// classes (service clients, journals, MCP sessions) never ship a second copy.
const esbuild = require('esbuild');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
// Pure contract modules shared by the host, the webview and the Assistance server.
const CONTRACTS = ['src/explorerContracts.ts', 'src/explorerRoutes.ts', 'src/explorerFeatures.ts', 'src/explorerErrors.ts', 'src/explorerStatus.ts', 'src/explorerStrategies.ts', 'src/explorerDrafts.ts'];

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

const TOPIC_KEYS = ['id', 'title', 'route', 'area', 'aliases', 'related_airk_help'];
const PLANNED_MARKER = 'Until this screen ships';

/** airk_help topic ids, read from the native catalog so cross-references cannot drift. */
function engineTopics() {
    const source = fs.readFileSync(path.resolve(root, '..', '..', 'crates', 'airk-mcp', 'src', 'help', 'topics.rs'), 'utf8');
    const ids = [...source.matchAll(/\bid:\s*"([a-z][a-z0-9-]*)"/g)].map(match => match[1]);
    if (ids.length < 20) { throw new Error('Could not read the airk_help topic catalog.'); }
    return new Set(ids);
}

/** Compile src/explorerHelp/*.md into one validated, version-matched catalog. */
function helpCatalog(directory = path.join(root, 'src', 'explorerHelp')) {
    const { ROUTE_KEYS } = require(path.join(root, 'out', 'explorerRoutes.js'));
    const { FEATURES } = require(path.join(root, 'out', 'explorerFeatures.js'));
    const engine = engineTopics();
    const topics = [];
    for (const file of fs.readdirSync(directory).filter(name => name.endsWith('.md')).sort()) {
        const text = fs.readFileSync(path.join(directory, file), 'utf8').replaceAll('\r\n', '\n');
        const match = /^---\n([\s\S]*?)\n---\n([\s\S]*)$/.exec(text);
        if (!match) { throw new Error(`${file}: missing front matter`); }
        const fields = {};
        for (const line of match[1].split('\n')) {
            const entry = /^([a-z_]+):\s*(.*)$/.exec(line);
            if (!entry || !TOPIC_KEYS.includes(entry[1]) || entry[1] in fields) { throw new Error(`${file}: invalid front matter line "${line}"`); }
            fields[entry[1]] = entry[2].trim();
        }
        if (TOPIC_KEYS.some(key => !(key in fields))) { throw new Error(`${file}: front matter needs ${TOPIC_KEYS.join(', ')}`); }
        const body = match[2].trim();
        const list = value => value.split(',').map(item => item.trim()).filter(Boolean);
        if (!/^[a-z][a-z0-9.-]{2,63}$/.test(fields.id) || topics.some(topic => topic.id === fields.id)) { throw new Error(`${file}: invalid or duplicate id`); }
        if (!fields.title || fields.title.length > 120) { throw new Error(`${file}: invalid title`); }
        if (fields.route && !ROUTE_KEYS.includes(fields.route)) { throw new Error(`${file}: route ${fields.route} is not in the route manifest`); }
        const related = list(fields.related_airk_help);
        for (const id of related) { if (!engine.has(id)) { throw new Error(`${file}: unknown airk_help topic ${id}`); } }
        if (body.length > 8000 || /\]\(/.test(body) || /<[a-z!/]/i.test(body)) { throw new Error(`${file}: body must be plain Markdown without links or HTML, at most 8000 characters`); }
        const applicability = fields.route ? FEATURES[fields.route] ?? 'unsupported' : 'implemented';
        if (applicability === 'implemented' && body.includes(PLANNED_MARKER)) { throw new Error(`${file}: the route is implemented; replace the planned text with current behavior`); }
        topics.push({ id: fields.id, title: fields.title, ...(fields.route ? { route: fields.route } : {}), area: fields.area, applicability,
            aliases: list(fields.aliases), relatedAirkHelp: related, source: `packages/vscode/src/explorerHelp/${file}`, body });
    }
    const manifest = JSON.parse(read('package.json'));
    return { catalogVersion: 1, extensionVersion: manifest.version, topics };
}

function helpPlugin(catalog) {
    return { name: 'airk-explorer-help', setup(build) {
        build.onResolve({ filter: /^airk-explorer-help$/ }, () => ({ path: 'catalog', namespace: 'airk-explorer-help' }));
        build.onLoad({ filter: /.*/, namespace: 'airk-explorer-help' }, () => ({ contents: JSON.stringify(catalog), loader: 'json' }));
    } };
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
    const catalog = helpCatalog();
    const common = { absWorkingDir: root, bundle: true, metafile: true, sourcemap: false, legalComments: 'none', logLevel: 'warning', charset: 'utf8',
        plugins: [helpPlugin(catalog)] };
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
    checkInputs('Webview', webview.metafile, [/^src\/explorerWebview\//, ...CONTRACTS, /^node_modules\/preact\//, /^airk-explorer-help:/]);
    const assistance = await esbuild.build({
        ...common,
        entryPoints: ['src/explorerAssistance/main.ts'],
        outfile: 'out/explorerAssistance.js',
        format: 'cjs',
        platform: 'node',
        target: 'node20',
        banner: { js: `/*! AIRK Explorer Assistance MCP server ${catalog.extensionVersion}. No third-party code. */` },
    });
    // Node built-ins only: no packages, no host modules, no VS Code API.
    checkInputs('Assistance', assistance.metafile, [/^src\/explorerAssistance\//, ...CONTRACTS, /^airk-explorer-help:/]);
}

module.exports = { helpCatalog, engineTopics };
if (require.main === module) {
    build().catch(error => { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; });
}
