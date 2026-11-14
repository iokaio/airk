// SPDX-License-Identifier: Apache-2.0
// Builds the AIRK admin shell into crates/airk-cli/admin-assets/ (split §10.4, U18/U19).
// `node build.mjs` writes the sealed asset set; `node build.mjs --check` rebuilds into a
// temporary directory and fails when the output directory differs from that rebuild.
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const outDir = join(repo, 'crates', 'airk-cli', 'admin-assets');
const operationsFixture = join(repo, 'fixtures', 'split', 'i3', 'admin_operations.json');

const CONTENT_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
};

// The repository's license header, first in every emitted script and stylesheet.
const SPDX = '/* SPDX-License-Identifier: Apache-2.0 */';
// Keep native Help whitespace inside escaped strings, never physical JS lines.
export const JAVASCRIPT_SUPPORTED = Object.freeze({ 'template-literal': false });

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const sri = (bytes) => `sha256-${createHash('sha256').update(bytes).digest('base64')}`;
const contentType = (name) => {
  const type = CONTENT_TYPES[name.slice(name.lastIndexOf('.'))];
  if (!type) throw new Error(`no content type for ${name}`);
  return type;
};

// Every operation the shell calls must be a reviewed admin row (I3 operation table).
function checkOperations() {
  const table = JSON.parse(readFileSync(operationsFixture, 'utf8'));
  const admin = new Set(
    table.operations.filter((row) => row.surfaces.includes('admin')).map((row) => row.operation),
  );
  // The single pure C9/browser UI catalog retains the exact pre-migration OPS.
  const source = readFileSync(join(here, '../vscode/src/adminUiContract.ts'), 'utf8');
  const block = source.match(/export const OPS = \{([\s\S]*?)\} as const;/);
  if (!block) throw new Error('adminUiContract.ts: OPS table not found');
  const used = [...block[1].matchAll(/'([a-z_]+\.[a-z_]+)'/g)].map((m) => m[1]);
  if (used.length === 0) throw new Error('adminUiContract.ts: OPS table is empty');
  const missing = used.filter((op) => !admin.has(op));
  if (missing.length > 0) {
    throw new Error(`operations without an admin surface in ${operationsFixture}: ${missing.join(', ')}`);
  }
}

// Review M3: no HTML sinks in shell code (a stand-in for the lint rule).
const HTML_SINKS = /\b(innerHTML|outerHTML|insertAdjacentHTML|document\.write(ln)?|dangerouslySetInnerHTML|srcdoc)\b/;
function checkSinks(dir = join(here, 'src')) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) checkSinks(path);
    else if (/\.(tsx?|html)$/.test(entry.name) && HTML_SINKS.test(readFileSync(path, 'utf8'))) {
      throw new Error(`${path}: HTML sink is not allowed (security review M3)`);
    }
  }
}

async function bundle() {
  const js = await esbuild.build({
    entryPoints: [join(here, 'src', 'main.tsx')],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: ['es2022'],
    supported: JAVASCRIPT_SUPPORTED,
    minify: true,
    sourcemap: false,
    legalComments: 'eof',
    banner: { js: SPDX },
    charset: 'utf8',
    jsx: 'automatic',
    jsxImportSource: 'preact',
    define: { 'process.env.NODE_ENV': '"production"' },
    write: false,
    logLevel: 'warning',
  });
  const css = await esbuild.build({
    entryPoints: [join(here, 'src', 'app.css')],
    bundle: true,
    banner: { css: SPDX },
    minify: true,
    charset: 'utf8',
    write: false,
    logLevel: 'warning',
  });
  if (js.outputFiles.length !== 1 || css.outputFiles.length !== 1) {
    throw new Error('expected exactly one JavaScript and one CSS output');
  }
  return { js: Buffer.from(js.outputFiles[0].contents), css: Buffer.from(css.outputFiles[0].contents) };
}

async function build(target) {
  checkOperations();
  checkSinks();
  const { js, css } = await bundle();
  const files = new Map();
  const jsName = `app.${sha256(js).slice(0, 8)}.js`;
  const cssName = `app.${sha256(css).slice(0, 8)}.css`;
  files.set(jsName, js);
  files.set(cssName, css);

  const template = readFileSync(join(here, 'src', 'index.html'), 'utf8').replace(/\r\n/g, '\n');
  const html = Buffer.from(
    template
      .replace('{{APP_CSS}}', `/admin/assets/${cssName}`)
      .replace('{{APP_CSS_SRI}}', sri(css))
      .replace('{{APP_JS}}', `/admin/assets/${jsName}`)
      .replace('{{APP_JS_SRI}}', sri(js)),
    'utf8',
  );
  if (html.includes('{{')) throw new Error('src/index.html: unreplaced placeholder');
  // Security review M1: /admin/launch serves the same shell document as /admin/.
  files.set('index.html', html);
  files.set('launch.html', html);

  const names = [...files.keys()].sort();
  const manifestFiles = {};
  for (const name of names) {
    manifestFiles[name] = { content_type: contentType(name), sha256: sha256(files.get(name)) };
  }
  const manifest = { files: manifestFiles, schema: 'airk-admin-assets/1' };
  files.set('manifest.json', Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`, 'utf8'));

  mkdirSync(target, { recursive: true });
  for (const entry of readdirSync(target)) {
    if (!files.has(entry)) rmSync(join(target, entry), { recursive: true, force: true });
  }
  for (const [name, bytes] of files) writeFileSync(join(target, name), bytes);
  return files;
}

function compare(expected, actualDir) {
  const problems = [];
  const actual = existsSync(actualDir) ? readdirSync(actualDir) : [];
  for (const name of actual) if (!expected.has(name)) problems.push(`unexpected file ${name}`);
  for (const [name, bytes] of expected) {
    const path = join(actualDir, name);
    if (!existsSync(path)) problems.push(`missing file ${name}`);
    else if (!readFileSync(path).equals(bytes)) problems.push(`differs: ${name}`);
  }
  return problems;
}

async function main() {
  const check = process.argv.includes('--check');
  if (check) {
    const temp = mkdtempSync(join(tmpdir(), 'airk-admin-'));
    try {
      const rebuilt = await build(temp);
      const problems = compare(rebuilt, outDir);
      if (problems.length > 0) {
        console.error(`admin assets in ${outDir} are stale; run \`npm run build\` in packages/admin:`);
        for (const problem of problems) console.error(`  ${problem}`);
        process.exitCode = 1;
      } else {
        console.log(`admin assets up to date (${rebuilt.size} files)`);
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  } else {
    const files = await build(outDir);
    for (const [name, bytes] of [...files].sort(([a], [b]) => a.localeCompare(b))) {
      console.log(`${name}\t${bytes.length} bytes`);
    }
  }
}
// Importing the producer exposes its real code-generation policy without writes.
if (process.argv[1] && realpathSync(resolve(process.argv[1])) === fileURLToPath(import.meta.url)) await main();
