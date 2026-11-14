// SPDX-License-Identifier: Apache-2.0
// Bundles the AIRK Explorer webview and Assistance server into flat out/ files. Host modules stay
// tsc-compiled CommonJS; bundles may only contain allowlisted inputs so host
// classes (service clients, journals, MCP sessions) never ship a second copy.
const esbuild = require('esbuild');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const root = path.resolve(__dirname, '..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
// Native Help can contain meaningful trailing whitespace; encode it in strings.
const JAVASCRIPT_SUPPORTED = Object.freeze({ 'template-literal': false });
// Pure contract modules shared by the host, the webview and the Assistance server.
const CONTRACTS = ['src/adminUiContract.ts', 'src/explorerContracts.ts', 'src/uiPilotContract.ts', 'src/explorerRoutes.ts', 'src/explorerFeatures.ts', 'src/explorerErrors.ts', 'src/explorerStatus.ts', 'src/explorerStrategies.ts', 'src/explorerDrafts.ts', 'src/indexingCatalog.ts', 'src/indexingSplit.ts', 'src/catalogSession.ts', 'src/publications.ts'];
const PILOT_RUNTIME_SOURCES = ['packages/ui/src/nativePilot.ts', 'packages/ui/src/pilotContract.ts', 'packages/vscode/src/uiPilotContract.ts', 'packages/vscode/scripts/build-explorer.cjs'];
const SHARED_MANAGEMENT_INPUTS = [
    ...['activity.tsx', 'activityRequests.ts', 'agents.tsx', 'api.ts', 'app.tsx', 'arbiter.tsx', 'bundledHelp.tsx',
        'catalogRequests.ts', 'concurrencyNote.tsx', 'envelope.ts', 'folderPicker.tsx', 'generatedActions.ts', 'help.tsx',
        'indexLibrary.tsx', 'operations.ts', 'pages.tsx', 'pageVisibility.ts', 'pilotPane.tsx', 'preparedActions.tsx', 'preparedMutations.ts',
        'progress.tsx', 'render.tsx', 'scopeConcurrency.ts', 'session.ts', 'usage.tsx'].map(file => `../admin/src/${file}`),
    ...['browserPilot.ts', 'helpMarkdown.ts', 'helpRoute.ts', 'helpSchema.ts', 'helpSearch.ts', 'helpToolPresentation.ts',
        'management.ts', 'nativeSessionLifetime.ts', 'navigation.ts', 'pilotContract.ts', 'pilotPresentation.ts', 'pilotProjection.ts', 'usagePresentation.ts']
        .map(file => `../ui/src/${file}`), '../ui/generated/help-manifest.json',
];
function pilotRuntimeSourceSeal(repository = path.resolve(root, '../..')) {
    const hash = crypto.createHash('sha256').update('AIRK-UI-PILOT-RUNTIME-1\0');
    for (const source of PILOT_RUNTIME_SOURCES) hash.update(source).update('\0').update(fs.readFileSync(path.join(repository, source))).update('\0');
    return hash.digest('hex');
}

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

/** Rust-emitted native Help inputs; source literals are never parsed here. */
function nativeHelpContent(file = path.resolve(root, '../..', 'crates/airk-mcp/src/help/content.json'), expectedVersion = JSON.parse(read('package.json')).version) {
    const content = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (!content || content.schema !== 'airk.help.native-content/1' || content.version !== expectedVersion
        || !Array.isArray(content.topics) || content.topics.length < 20 || !Array.isArray(content.tools)
        || !content.tools.length || !content.cliCommands || typeof content.cliCommands !== 'object' || Array.isArray(content.cliCommands)
        || !content.helpRequestSchema || typeof content.helpRequestSchema !== 'object' || Array.isArray(content.helpRequestSchema)) {
        throw new Error('Native Help content is missing, malformed or version-mismatched; regenerate it through its Rust owner.');
    }
    const ids = new Set();
    for (const topic of content.topics) {
        if (!topic || !/^[a-z][a-z0-9-]{0,63}$/.test(topic.id) || typeof topic.title !== 'string' || typeof topic.text !== 'string' || ids.has(topic.id)) {
            throw new Error('Native Help contains an invalid or duplicate topic.');
        }
        ids.add(topic.id);
    }
    for (const tool of content.tools) {
        if (!tool || typeof tool.name !== 'string' || !tool.inputSchema || typeof tool.inputSchema !== 'object' || Array.isArray(tool.inputSchema)) throw new Error('Native Help contains an invalid complete tool schema.');
    }
    return content;
}
function engineTopics(content = nativeHelpContent()) { return new Set(content.topics.map(topic => topic.id)); }
function engineGuides(content = nativeHelpContent()) {
    return content.topics.map(({ id, title, text }) => ({ id: `engine.${id}`, title: `Engine: ${title}`, area: 'engine',
        aliases: [id, ...id.split('-')], applicability: 'implemented', relatedAirkHelp: [id],
        source: 'crates/airk-mcp/src/help/content.json', body: `# ${title}\n\n${text}` }));
}
function sharedHelpManifest(catalog, content = nativeHelpContent()) {
    if (catalog.extensionVersion !== content.version) throw new Error('Shared Help product and extension versions differ.');
    return { schema: 'airk.help.manifest/1', catalogVersion: catalog.catalogVersion,
        productVersion: content.version, extensionVersion: catalog.extensionVersion, channel: null,
        topics: catalog.topics, cliCommands: content.cliCommands, helpRequestSchema: content.helpRequestSchema,
        tools: content.tools, native: content };
}

/** Compile src/explorerHelp/*.md into one validated, version-matched catalog. */
function helpCatalog(directory = path.join(root, 'src', 'explorerHelp'), native = nativeHelpContent()) {
    const { ROUTE_KEYS } = require(path.join(root, 'out', 'explorerRoutes.js'));
    const { FEATURES } = require(path.join(root, 'out', 'explorerFeatures.js'));
    const engine = engineTopics(native);
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
    const guides = engineGuides(native);
    if (guides.some(guide => topics.some(topic => topic.id === guide.id))) { throw new Error('Explorer help cannot shadow an engine topic.'); }
    return { catalogVersion: 1, extensionVersion: manifest.version, topics: [...topics, ...guides] };
}

function helpPlugin(catalog) {
    return { name: 'airk-explorer-help', setup(build) {
        build.onResolve({ filter: /^airk-explorer-help$/ }, () => ({ path: 'catalog', namespace: 'airk-explorer-help' }));
        build.onLoad({ filter: /.*/, namespace: 'airk-explorer-help' }, () => ({ contents: JSON.stringify(catalog), loader: 'json' }));
    } };
}

function sharedAdminStylePlugin() {
    return { name: 'airk-shared-admin-style', setup(build) {
        build.onResolve({ filter: /^airk-shared-admin-css$/ }, () => ({ path: 'scoped', namespace: 'airk-shared-admin-css' }));
        build.onLoad({ filter: /.*/, namespace: 'airk-shared-admin-css' }, () => {
            const css = fs.readFileSync(path.resolve(root, '../admin/src/app.css'), 'utf8')
                .replace(/^(\s*):root\b/gm, '$1:scope').replace(/^body\b/gm, ':scope');
            return { contents: `@scope (.airk-shared-management) {\n${css}\n}\n.airk-shared-management .layout { min-height: auto; grid-template-columns: minmax(0, 1fr); }`, loader: 'css' };
        });
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
    const native = nativeHelpContent();
    const catalog = helpCatalog(undefined, native);
    const shared = sharedHelpManifest(catalog, native);
    const sharedBytes = JSON.stringify(shared, null, 2) + '\n';
    const sharedFile = path.resolve(root, '../ui/generated/help-manifest.json');
    if (process.argv.includes('--write-help-manifest')) {
        fs.mkdirSync(path.dirname(sharedFile), { recursive: true }); fs.writeFileSync(sharedFile, sharedBytes); return;
    }
    if (!fs.existsSync(sharedFile) || fs.readFileSync(sharedFile, 'utf8') !== sharedBytes) throw new Error('Shared Help manifest is stale or missing; run node scripts/build-explorer.cjs --write-help-manifest and review the generated bytes.');
    // Native MCP owns the unified public catalog. Keep its embedded UI schemas
    // byte-for-byte derived from the same validators used by the extension.
    const { toolDefinitions } = require(path.join(root, 'out', 'explorerAssistance', 'tools.js'));
    const { HelpIndex } = require(path.join(root, 'out', 'explorerAssistance', 'help.js'));
    const nativeCatalog = path.resolve(root, '..', '..', 'crates', 'airk-mcp', 'src', 'unified_explorer.json');
    const schemas = JSON.stringify(toolDefinitions(new HelpIndex(catalog)), null, 2) + '\n';
    // The native CLI serves list_templates/get_template offline from the same strategy pack.
    const { CORE_PACK, OPERATORS } = require(path.join(root, 'out', 'explorerStrategies.js'));
    const nativeTemplates = path.resolve(root, '..', '..', 'crates', 'airk-mcp', 'src', 'explorer_templates.json');
    const templates = JSON.stringify({ catalog_version: catalog.catalogVersion, extension_version: catalog.extensionVersion, templates: CORE_PACK, operators: OPERATORS }, null, 2) + '\n';
    for (const [file, text] of [[nativeCatalog, schemas], [nativeTemplates, templates]]) {
        if (process.argv.includes('--write-mcp-catalog')) { fs.writeFileSync(file, text); }
        else if (!fs.existsSync(file) || fs.readFileSync(file, 'utf8') !== text) {
            throw new Error(`Native Explorer catalog ${path.basename(file)} differs: run node scripts/build-explorer.cjs --write-mcp-catalog and review the diff.`);
        }
    }
    const common = { absWorkingDir: root, bundle: true, metafile: true, sourcemap: false, legalComments: 'none', logLevel: 'warning', charset: 'utf8',
        supported: JAVASCRIPT_SUPPORTED,
        plugins: [helpPlugin(shared)] };
    const pilot = await esbuild.build({ ...common, entryPoints: ['../ui/src/nativePilot.ts'], outfile: 'out/uiPilotRuntime.js',
        format: 'cjs', platform: 'node', target: 'node20',
        banner: { js: `/* SPDX-License-Identifier: Apache-2.0 */\n/* AIRK UI pilot runtime source-sha256: ${pilotRuntimeSourceSeal()} */` } });
    checkInputs('UI pilot runtime', pilot.metafile, ['../ui/src/nativePilot.ts', '../ui/src/pilotContract.ts', 'src/uiPilotContract.ts']);
    const webview = await esbuild.build({
        ...common,
        // All shared components use the consuming editor's one hook/runtime owner.
        alias: { preact: path.join(root, 'node_modules/preact') },
        plugins: [...common.plugins, sharedAdminStylePlugin()],
        entryPoints: ['src/explorerWebview/main.tsx'],
        outfile: 'out/explorerWebview.js',
        format: 'iife',
        platform: 'browser',
        target: 'es2022',
        jsx: 'automatic',
        jsxImportSource: 'preact',
        banner: { js: preactBanner() },
    });
    checkInputs('Webview', webview.metafile, [/^src\/explorerWebview\//, ...CONTRACTS, ...SHARED_MANAGEMENT_INPUTS, /^node_modules\/preact\//, /^airk-explorer-help:/, /^airk-shared-admin-css:/]);
    const workbench = await esbuild.build({ ...common, entryPoints: ['src/workbenchWebview/main.tsx'], outfile: 'out/workbenchWebview.js',
        format: 'iife', platform: 'browser', target: 'es2022', jsx: 'automatic', jsxImportSource: 'preact', banner: { js: preactBanner() } });
    checkInputs('Workbench', workbench.metafile, [/^src\/workbenchWebview\//, '../ui/src/answerPresentation.ts', 'src/workbenchContracts.ts', 'src/workbenchRender.ts', 'src/sourceVerification.ts', 'src/publications.ts', /^node_modules\/preact\//]);
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
    // This process must survive removal of the extension: bundle the hook alone.
    // It removes only editor-owned storage and imports Node built-ins only (plan §8.5).
    const uninstall = await esbuild.build({
        absWorkingDir: root, bundle: true, metafile: true, sourcemap: false, legalComments: 'none', logLevel: 'warning',
        entryPoints: ['src/uninstallHook.ts'], outfile: 'out/uninstallHook.js', format: 'cjs', platform: 'node', target: 'node20', mainFields: ['module', 'main'],
        banner: { js: `/*! AIRK uninstall hook ${catalog.extensionVersion}: removes editor-owned storage only. No third-party code. */` },
    });
    checkInputs('Uninstall', uninstall.metafile, ['src/uninstallHook.ts']);
    fs.writeFileSync(path.join(root, 'out/help-manifest.json'), sharedBytes);
}

module.exports = { helpCatalog, engineTopics, engineGuides, nativeHelpContent, sharedHelpManifest, pilotRuntimeSourceSeal, sharedAdminStylePlugin, JAVASCRIPT_SUPPORTED };
if (require.main === module) {
    build().catch(error => { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; });
}
