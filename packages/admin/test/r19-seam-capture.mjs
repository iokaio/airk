// SPDX-License-Identifier: Apache-2.0
// R19a source-only capture/relocation. No compiler, bundle or test is executed.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../..');
const src = resolve(here, '../src');
const args = process.argv.slice(2);
const option = (name) => { const at = args.indexOf(name); return at < 0 ? null : args[at + 1]; };
const digest = (text) => createHash('sha256').update(text).digest('hex');
const same = (a, b) => a.replace(/\r\n/g, '\n') === b.replace(/\r\n/g, '\n');
const parse = (name, text) => ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const find = (node, predicate) => {
  if (predicate(node)) return node;
  let found;
  ts.forEachChild(node, (child) => { found ??= find(child, predicate); });
  return found;
};
const variable = (node, name) => find(node, (n) => ts.isVariableDeclaration(n) && n.name.getText() === name);
const functionOf = (node, name) => find(node, (n) => ts.isFunctionDeclaration(n) && n.name?.text === name);
const statement = (declaration) => declaration.parent.parent;
const helpers = (file, names) => names.map((name) => statement(variable(file, name)).getText()).join('\n');
const range = (node, text) => ({ start: node.getStart(), end: node.end, text });

function capture(revision) {
  if (revision !== 'current' && !/^[0-9a-f]{7,40}$/.test(revision)) throw new Error('source must be current or a commit id');
  const read = (name) => revision === 'current' ? readFileSync(resolve(src, name), 'utf8')
    : execFileSync('git', ['-c', `safe.directory=${root.replaceAll('\\', '/')}`, 'show', `${revision}:packages/admin/src/${name}`], { cwd: root, encoding: 'utf8' });
  const activity = read('activity.tsx'), catalog = read('indexLibrary.tsx');
  const a = parse('activity.tsx', activity), c = parse('indexLibrary.tsx', catalog);
  const page = functionOf(c, 'CatalogList'), detail = functionOf(c, 'IndexDetail');
  const open = variable(functionOf(a, 'Activity'), 'openJob');
  const change = variable(functionOf(a, 'Activity'), 'jobChange');
  const reload = variable(functionOf(a, 'useIndexes'), 'reload');
  const indexInfo = a.statements.find((node) => ts.isTypeAliasDeclaration(node) && node.name.text === 'IndexInfo');
  const namePages = statement(variable(a, 'MAX_NAME_PAGES'));
  const projectPages = statement(variable(a, 'MAX_PROJECT_PAGES'));
  const load = variable(page, 'load');
  const cacheWrite = detail.body.statements.find((node) => ts.isIfStatement(node) && node.expression.getText() === 'loaded !== null');
  const display = statement(variable(detail, 'd'));
  const itemType = c.statements.find((node) => ts.isTypeAliasDeclaration(node) && node.name.text === 'Item');
  const item = functionOf(c, 'itemOf');
  if (![open, change, reload, indexInfo, load, cacheWrite, display, itemType, item].every(Boolean)
    || ![open, change, reload, load].every(node => ts.isArrowFunction(node.initializer) && ts.isBlock(node.initializer.body))) {
    throw new Error('original R19 procedures are not present; use --source 625c1766 for a retained before capture');
  }
  const activityNames = ['rec', 'okValue', 'str', 'num', 'arr'];
  const activityHelpers = helpers(a, activityNames);
  const catalogHelpers = helpers(c, ['rec', 'str', 'num', 'arr', 'text']);
  const jobSource = `// SPDX-License-Identifier: Apache-2.0
// Activity's request/state procedures, relocated without changing acceptance.
import type { CallResult } from './api';
import { body, MAX_INDEX_LIST_LIMIT, OPS, type Operation } from './operations';
type Rec = Record<string, unknown>;
${activityHelpers}
export interface JobCallbacks {
  runJob: (operation: Operation, body: unknown) => Promise<CallResult>;
  runJobAction: (operation: Operation, body: unknown) => Promise<CallResult>;
  setJobId: (id: string) => void;
  setJobRecord: (record: Rec | null) => void;
}
export function createJobRequests(callbacks: JobCallbacks) {
  const { runJob, runJobAction, setJobId, setJobRecord } = callbacks;
  const openJob = ${open.initializer.getText()};
  const jobChange = async (jobId: string, kind: 'retry' | 'cancel'): Promise<void> => ${change.initializer.body.getText()};
  return { openJob, jobChange };
}
${indexInfo.getText()}
${namePages.getText()}
${projectPages.getText()}
export interface IndexCallbacks {
  runCatalog: (operation: Operation, body: unknown) => Promise<CallResult>;
  runProjects: (operation: Operation, body: unknown) => Promise<CallResult>;
  setRegistryKnown: (known: boolean) => void;
  setPartial: (partial: boolean) => void;
  setIndexes: (indexes: IndexInfo[]) => void;
}
export function createIndexRequests(callbacks: IndexCallbacks) {
  const { runCatalog, runProjects, setRegistryKnown, setPartial, setIndexes } = callbacks;
  const reload = ${reload.initializer.getText()};
  return { reload };
}
`;
  const catalogSource = `// SPDX-License-Identifier: Apache-2.0
// CatalogList's request/state procedure and IndexDetail's retained-detail projection.
// R19a preserves captured-row merging and the original detail-cache behavior.
import type { CallResult } from './api';
import { body, OPS, type CatalogKind, type Operation } from './operations';
type Rec = Record<string, unknown>;
${catalogHelpers}
export ${itemType.getText()}
export ${item.getText()}
export interface CatalogListProps {
  kind: CatalogKind;
  onRevision: (revision: number) => void;
  onItems?: (items: Item[], complete: boolean) => void;
}
export interface CatalogCallbacks {
  run: (operation: Operation, body: unknown) => Promise<CallResult>;
  setItems: (items: Item[]) => void;
  setCursor: (cursor: unknown) => void;
  setHeader: (header: { namespace: string; revision: number }) => void;
}
export function createCatalogRequests(callbacks: CatalogCallbacks) {
  const { run, setItems, setCursor, setHeader } = callbacks;
  const load = async (after: unknown, props: CatalogListProps,
    filter: { indexId: string | null; projectId: string | null }, items: Item[]): Promise<void> => ${load.initializer.body.getText()};
  return { load };
}
export type IndexDetailCache = { current: { id: string; detail: Rec } | null };
export function indexDetailValue(props: { id: string }, detail: { phase: string }, loaded: Rec | null, lastGood: IndexDetailCache): Rec | null {
  ${cacheWrite.getText()}
  ${display.getText()}
  return d;
}
`;
  return {
    schema: 'airk.admin.r19-source-capture/1', source: revision,
    original: { 'activity.tsx': activity, 'indexLibrary.tsx': catalog },
    hashes: { 'activity.tsx': digest(activity), 'indexLibrary.tsx': digest(catalog) },
    bodies: { openJob: open.initializer.body.getText(), jobChange: change.initializer.body.getText(), reload: reload.initializer.body.getText(), load: load.initializer.body.getText(), cacheWrite: cacheWrite.getText(), display: display.getText() },
    helpers: { activity: activityHelpers, activityNames, catalog: catalogHelpers },
    modules: { activityRequests: jobSource, catalogRequests: catalogSource },
    replacements: {
      'activity.tsx': [range(statement(open), 'const jobRequests = createJobRequests({ runJob, runJobAction, setJobId, setJobRecord });\n  const openJob = jobRequests.openJob;'), range(statement(change), "const jobChange = (kind: 'retry' | 'cancel'): Promise<void> => jobRequests.jobChange(jobId, kind);"), range(statement(reload), 'const indexRequests = createIndexRequests({ runCatalog, runProjects, setRegistryKnown, setPartial, setIndexes });\n  const reload = indexRequests.reload;'), range(indexInfo, ''), { start: namePages.getFullStart(), end: namePages.end, text: '' }, { start: projectPages.getFullStart(), end: projectPages.end, text: '' }],
      'indexLibrary.tsx': [range(itemType, ''), range(item, ''), range(statement(load), 'const catalogRequests = createCatalogRequests({ run, setItems, setCursor, setHeader });\n  const load = (after: unknown): Promise<void> => catalogRequests.load(after, props, filter, items);'), { start: cacheWrite.getStart(), end: display.end, text: 'const d = indexDetailValue(props, detail, loaded, lastGood);' }],
    },
  };
}

function relocate(saved) {
  for (const [name, changes] of Object.entries(saved.replacements)) {
    let text = readFileSync(resolve(src, name), 'utf8');
    if (digest(text) !== saved.hashes[name]) throw new Error(`${name} changed after source capture`);
    for (const change of [...changes].sort((a, b) => b.start - a.start)) text = text.slice(0, change.start) + change.text + text.slice(change.end);
    const added = name === 'activity.tsx' ? "import { createIndexRequests, createJobRequests, type IndexInfo } from './activityRequests';\nexport type { IndexInfo } from './activityRequests';\n" : "import { createCatalogRequests, indexDetailValue, itemOf, type Item } from './catalogRequests';\n";
    const anchor = text.indexOf("import type { ComponentChildren, JSX } from 'preact';");
    if (anchor < 0) throw new Error(`missing ${name} import anchor`);
    text = text.slice(0, anchor) + added + text.slice(anchor);
    if (name === 'activity.tsx') text = text.replace('body, isDigest, MAX_INDEX_LIST_LIMIT, OPS', 'body, isDigest, OPS');
    writeFileSync(resolve(src, name), text);
  }
  for (const [name, text] of Object.entries(saved.modules)) writeFileSync(resolve(src, `${name}.ts`), text);
}

function check(saved) {
  const a = parse('activityRequests.ts', readFileSync(resolve(src, 'activityRequests.ts'), 'utf8'));
  const c = parse('catalogRequests.ts', readFileSync(resolve(src, 'catalogRequests.ts'), 'utf8'));
  for (const name of ['openJob', 'jobChange', ...(saved.bodies.reload ? ['reload'] : []), 'load']) {
    const moved = variable(name === 'load' ? c : a, name)?.initializer.body.getText();
    if (!moved || !same(moved, saved.bodies[name])) throw new Error(`${name} body changed in the seam step`);
  }
  const projection = functionOf(c, 'indexDetailValue');
  for (const [key, actual] of [['cacheWrite', projection.body.statements[0].getText()], ['display', projection.body.statements[1].getText()]]) {
    if (!same(actual, saved.bodies[key])) throw new Error(`${key} changed in the seam step`);
  }
  if (!same(helpers(a, saved.helpers.activityNames ?? ['rec', 'okValue']), saved.helpers.activity) || !same(helpers(c, ['rec', 'str', 'num', 'arr', 'text']), saved.helpers.catalog)) throw new Error('relocated value parser changed');
  process.stdout.write('R19 relocated procedure bodies and value parsers match the captured source (CRLF/LF normalized).\n');
}

function extend(saved, previous) {
  const currentModule = readFileSync(resolve(src, 'activityRequests.ts'), 'utf8');
  if (!same(currentModule, previous.modules.activityRequests)) throw new Error('activity request owner changed after the first seam');
  let text = readFileSync(resolve(src, 'activity.tsx'), 'utf8');
  const a = parse('activity.tsx', text);
  const reload = variable(functionOf(a, 'useIndexes'), 'reload');
  if (!same(reload.initializer.body.getText(), saved.bodies.reload)) throw new Error('index-name loader changed since baseline capture');
  const type = a.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === 'IndexInfo');
  const names = statement(variable(a, 'MAX_NAME_PAGES')), projects = statement(variable(a, 'MAX_PROJECT_PAGES'));
  const changes = [range(statement(reload), 'const indexRequests = createIndexRequests({ runCatalog, runProjects, setRegistryKnown, setPartial, setIndexes });\n  const reload = indexRequests.reload;'), range(type, ''),
    { start: names.getFullStart(), end: names.end, text: '' }, { start: projects.getFullStart(), end: projects.end, text: '' }];
  for (const change of changes.sort((a, b) => b.start - a.start)) text = text.slice(0, change.start) + change.text + text.slice(change.end);
  const oldImport = "import { createJobRequests } from './activityRequests';";
  if (!text.includes(oldImport)) throw new Error('missing first-seam activity import');
  text = text.replace(oldImport, "import { createIndexRequests, createJobRequests, type IndexInfo } from './activityRequests';\nexport type { IndexInfo } from './activityRequests';")
    .replace('body, isDigest, MAX_INDEX_LIST_LIMIT, OPS', 'body, isDigest, OPS');
  writeFileSync(resolve(src, 'activity.tsx'), text);
  writeFileSync(resolve(src, 'activityRequests.ts'), saved.modules.activityRequests);
}

if (option('--extend')) extend(JSON.parse(readFileSync(resolve(option('--extend')), 'utf8')),
  JSON.parse(readFileSync(resolve(option('--previous')), 'utf8')));
else if (option('--relocate')) relocate(JSON.parse(readFileSync(resolve(option('--relocate')), 'utf8')));
else if (option('--check')) check(JSON.parse(readFileSync(resolve(option('--check')), 'utf8')));
else {
  const out = option('--out');
  if (!out) throw new Error('supply --out, --relocate or --check');
  const saved = capture(option('--source') ?? 'current');
  mkdirSync(dirname(resolve(out)), { recursive: true });
  writeFileSync(resolve(out), JSON.stringify(saved, null, 2) + '\n');
  process.stdout.write(`Captured R19 source from ${saved.source}; no procedures executed.\n`);
}
