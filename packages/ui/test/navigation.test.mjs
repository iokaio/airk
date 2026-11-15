// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const bundled = await build({ stdin: { contents: `export * from './packages/ui/src/navigation'; export {parseRoute as editorRoute} from './packages/vscode/src/explorerRoutes'; export {parseRoute as browserRoute} from './packages/admin/src/app';`,
  resolveDir: root }, bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic', jsxImportSource: 'preact',
  plugins: [{ name: 'route-observation', setup(plugin) {
    plugin.onLoad({ filter: /packages[\\/]admin[\\/]src[\\/]app\.tsx$/ }, async args => ({ contents: `${await readFile(args.path, 'utf8')}\nexport {parseRoute};`, loader: 'tsx', resolveDir: `${root}/packages/admin/src` }));
  } }],
});
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);

test('both hosts retain the seven intended main destinations and exact existing route targets', () => {
  assert.deepEqual(actual.MAIN_DESTINATIONS.map(({ title, browser, editor }) => [title, browser, editor]), [
    ['Overview', 'overview', 'overview'], ['Search', 'search', 'search/query'], ['Indexes', 'index-library', 'indexing/library'],
    ['Activity', 'activity', 'manage/activity'], ['Agents', 'agents', 'manage/agents'], ['Service', 'service', 'manage/service'], ['Help', 'help', 'help'],
  ]);
  for (const entry of actual.MAIN_DESTINATIONS) {
    assert.ok(actual.browserRoute(entry.browser, 1)); assert.ok(actual.editorRoute(entry.editor));
  }
});

test('all eleven browser deep links survive grouping and Help navigation remains inert', () => {
  for (const page of ['overview', 'index-library', 'activity', 'agents', 'storage', 'recovery', 'service', 'logs', 'usage', 'search', 'help']) {
    assert.equal(actual.browserRoute(`#${page}?guide=engine.installation`, 7).page, page);
  }
  assert.equal(actual.browserDestination('logs'), 'activity'); assert.equal(actual.browserDestination('usage'), 'activity');
  assert.equal(actual.browserDestination('storage'), 'service'); assert.equal(actual.browserDestination('recovery'), 'service');
  assert.equal(actual.browserRoute('#execute?command=anything', 1), null);
});

test('retained editor evidence workflows and management links still parse without authority changes', () => {
  for (const route of ['code/symbols', 'code/references', 'code/outline', 'docs/facts', 'evidence/inspector', 'evidence/checker', 'evidence/changes',
    'review/changes', 'review/impact', 'review/tests', 'strategies/library', 'strategies/studio', 'strategies/planner',
    'indexing/window', 'indexing/projects', 'manage/overview', 'manage/storage', 'manage/repair', 'manage/usage', 'manage/logs']) assert.ok(actual.editorRoute(route), route);
  assert.equal(actual.editorDestination('manage/logs'), 'activity'); assert.equal(actual.editorDestination('manage/repair'), 'service');
  assert.equal(actual.editorDestination('review/tests'), 'search');
  assert.equal(actual.editorRoute('search/tools').tab,'tools');
});


test('Evidence tools retains every guided route and browser limitations without inventing a generic runner',()=>{
  assert.deepEqual(actual.EVIDENCE_TOOLS.flatMap(group=>group.entries.map(([route])=>route)),[
    'code/symbols','code/references','code/outline','docs/facts','evidence/inspector','evidence/checker','evidence/changes',
    'review/changes','review/impact','review/tests','strategies/library','strategies/studio','strategies/planner']);
  for(const route of ['code/symbols','docs/facts','evidence/checker','review/tests','strategies/planner']){
    assert.ok(actual.editorRoute(route));assert.equal(actual.editorDestination(route),'search');assert.equal(actual.isEvidenceToolRoute(route),true);
  }
  assert.equal(actual.isEvidenceToolRoute('code/arbitrary'),false);assert.equal(actual.isEvidenceToolRoute('command:execute'),false);
  const flatten=node=>typeof node==='string'?[node]:[node,...node.children.flatMap(flatten)];
  assert.equal(flatten(actual.evidenceToolsPresentation('editor','code/symbols')).filter(node=>typeof node!=='string'&&node.route==='code/symbols').length,2);
  const browser=flatten(actual.evidenceToolsPresentation('browser','code/symbols'));
  assert.ok(browser.some(node=>typeof node==='string'&&node.includes('migration is not complete')));
  assert.equal(browser.some(node=>typeof node!=='string'&&node.route!==undefined),false,'browser never pretends to execute editor guided operations');
});


test('browser Evidence tools Help next steps resolve actual complete bundled topics',async()=>{
  const manifest=JSON.parse(await readFile(new URL('../generated/help-manifest.json',import.meta.url),'utf8'));
  for(const group of actual.EVIDENCE_TOOLS)assert.equal(manifest.topics.filter(topic=>topic.id===group.guide).length,1,group.title);
});


test('owning native Explorer catalog includes the inert hub while retaining every literal old navigation target',async()=>{
  const catalog=JSON.parse(await readFile(new URL('../../../crates/airk-mcp/src/unified_explorer.json',import.meta.url),'utf8'));
  const tool=catalog.find(tool=>tool.name==='airk_explorer_suggest_navigation');assert.ok(tool);
  assert.deepEqual(tool.inputSchema.properties.route.enum,[
    'overview','search/query','search/recipes','search/tools','code/symbols','code/references','code/outline','docs/facts',
    'evidence/inspector','evidence/checker','evidence/changes','review/changes','review/impact','review/tests',
    'strategies/library','strategies/studio','strategies/planner','indexing/window','indexing/library','indexing/projects',
    'manage/overview','manage/activity','manage/storage','manage/repair','manage/service','manage/agents','manage/usage','manage/logs','help']);
});
