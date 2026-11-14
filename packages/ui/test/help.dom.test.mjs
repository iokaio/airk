// SPDX-License-Identifier: Apache-2.0
// Real rendering controls only. This does not claim live native transport parity.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { browserAvailable, browserHarness } from './browserHarness.mjs';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const manifest = JSON.parse(await readFile(new URL('../generated/help-manifest.json', import.meta.url), 'utf8'));
let browser;
before(async () => { if (browserAvailable) browser = await browserHarness(); });
after(async () => { await browser?.close(); });

for (const host of ['browser', 'editor']) {
  test(`shared Help ${host}: real full-text selection, article focus and complete native tool definition at390px`,
    { skip: browserAvailable ? false : 'Set AIRK_BROWSER to Chromium for real DOM qualification.' }, async () => {
      const browserHost = host === 'browser';
      const source = browserHost ? 'packages/admin/src' : 'packages/vscode/src/explorerWebview';
      const contents = browserHost
        ? `import {h,render} from 'preact'; import {BundledHelp} from './bundledHelp'; render(h(BundledHelp,{}),document.body);`
        : `import {h,render} from 'preact'; import {HelpScreen} from './help'; render(h(HelpScreen,{route:{view:'help'},state:{trusted:false,nativeExecutionEnabled:false,connection:'disconnected'},go(){}}),document.body);`;
      const isolated = contents.replace('render(h(', `const host=document.createElement('main');document.body.append(host);window.unmountHelp=()=>render(null,host);render(h(`).replace(',document.body);', ',host);');
      const bundle = await build({ stdin: { contents: isolated, resolveDir: `${root}/${source}`, sourcefile: 'help-dom.tsx' },
        bundle: true, write: false, format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact',
        plugins: [{ name: 'offline-help', setup(plugin) {
          plugin.onResolve({ filter: /^airk-explorer-help$/ }, () => ({ path: 'manifest', namespace: 'offline-help' }));
          plugin.onLoad({ filter: /.*/, namespace: 'offline-help' }, () => ({ contents: JSON.stringify(manifest), loader: 'json' }));
          plugin.onResolve({ filter: /^\.\/api$/ }, args => !browserHost && args.importer.includes('explorerWebview') ? { path: 'api', namespace: 'offline-api' } : undefined);
          plugin.onLoad({ filter: /.*/, namespace: 'offline-api' }, () => ({ contents: `export function request(){window.unexpectedNativeCalls++;return Promise.reject(Error('offline'));}` }));
        } }],
      });
      await browser.call('Emulation.setDeviceMetricsOverride', { width: 390, height: 760, deviceScaleFactor: 1, mobile: false });
      await browser.evaluate('window.unmountHelp?.();document.body.replaceChildren();window.unexpectedNativeCalls=0;');
      const css = await readFile(new URL(browserHost ? '../../admin/src/app.css' : '../../vscode/src/explorerWebview/styles.css', import.meta.url), 'utf8');
      await browser.evaluate(`document.head.replaceChildren();{const style=document.createElement('style');style.textContent=${JSON.stringify(css)};document.head.append(style);}`);
      await browser.evaluate(bundle.outputFiles[0].text);
      await browser.observed(`document.querySelector('input[type="search"],input[aria-label="Search help"]')`);
      const phrase = 'source freshness';
      const expected = manifest.topics.filter(topic => topic.body.toLowerCase().includes(phrase));
      assert.ok(expected.length, 'independent literal task phrase exists in emitted content');
      await browser.evaluate(`{const input=document.querySelector('input[type="search"],input[aria-label="Search help"]');input.value=${JSON.stringify(phrase)};input.dispatchEvent(new Event('input',{bubbles:true}));}`);
      await browser.observed(`document.body.textContent.includes('matching')`);
      const title = expected[0].title;
      await browser.evaluate(`{const button=[...document.querySelectorAll('button')].find(button=>button.textContent.trim()===${JSON.stringify(title)});if(!button)throw Error('Full-text topic absent');button.click();}`);
      await browser.observed(`document.activeElement?.tagName==='ARTICLE'`);
      assert.equal(await browser.evaluate(`document.activeElement.textContent.includes(${JSON.stringify(title)})`), true);
      assert.ok(await browser.evaluate('document.activeElement.getBoundingClientRect().top<100'), 'selected article is first in the compact viewport');
      await browser.evaluate(`{const details=[...document.querySelectorAll('details')].find(details=>details.querySelector('summary')?.textContent==='Bundled public tool reference');details.open=true;const select=details.querySelector('select');select.value='help';select.dispatchEvent(new Event('change',{bubbles:true}));}`);
      await browser.observed(`document.body.textContent.includes('Complete native tool definition (JSON)')`);
      const tool = manifest.tools.find(tool => tool.name === 'help');
      const raw = await browser.evaluate(`JSON.parse([...document.querySelectorAll('pre')].find(pre=>{try{return JSON.parse(pre.textContent)?.name==='help';}catch{return false;}}).textContent)`);
      assert.deepEqual(raw, tool);
      assert.equal(await browser.evaluate('window.unexpectedNativeCalls'), 0);
    });
}

test('browser signed-out Help uses actual route capture and Back/Forward without a session or native read',
  { skip: browserAvailable ? false : 'Set AIRK_BROWSER to Chromium for real DOM qualification.' }, async () => {
    const server = createServer((_request, response) => { response.setHeader('Content-Type', 'text/html'); response.end('<!doctype html><html><head></head><body></body></html>'); });
    server.listen(0, '127.0.0.1'); await once(server, 'listening');
    try {
      await browser.call('Page.navigate', { url: `http://127.0.0.1:${server.address().port}/admin/#help` });
      await browser.observed(`location.pathname==='/admin/'&&document.readyState==='complete'`);
      const bundle = await build({ stdin: { resolveDir: `${root}/packages/admin/src`, sourcefile: 'offline-app-dom.tsx', contents: `
        import {h,render} from 'preact'; import {App} from './app'; import {takeLaunchCode} from './session';
        window.unexpectedNativeCalls=0;window.fetch=()=>{window.unexpectedNativeCalls++;return Promise.reject(Error('Offline Help must not fetch'));};
        window.mountOfflineApp=()=>{takeLaunchCode();const host=document.createElement('main');document.body.append(host);
          window.unmountHelp=()=>render(null,host);render(h(App,{initialGate:{kind:'none'}}),host);};window.mountOfflineApp();
      ` }, bundle: true, write: false, format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact' });
      await browser.evaluate(bundle.outputFiles[0].text);
      await browser.observed(`document.body.textContent.includes('Find a guide')`);
      await browser.evaluate(`{const search=document.querySelector('input[type="search"]');search.value='source freshness';search.dispatchEvent(new Event('input',{bubbles:true}));}`);
      await browser.observed(`new URLSearchParams(location.hash.split('?')[1]).get('q')==='source freshness'`);
      await browser.evaluate(`{[...document.querySelectorAll('button')].find(button=>button.textContent==='Create an index').click();}`);
      await browser.observed(`new URLSearchParams(location.hash.split('?')[1]).get('guide')==='indexing.library'&&document.querySelector('article')`);
      await browser.evaluate('history.back()');
      await browser.observed(`!new URLSearchParams(location.hash.split('?')[1]).has('guide')&&!document.querySelector('article')`);
      assert.equal(await browser.evaluate(`document.querySelector('input[type="search"]').value`), 'source freshness');
      await browser.evaluate('history.forward()');
      await browser.observed(`new URLSearchParams(location.hash.split('?')[1]).get('guide')==='indexing.library'&&document.querySelector('article')`);
      await browser.evaluate('window.unmountHelp();document.body.replaceChildren();window.mountOfflineApp();');
      await browser.observed(`document.querySelector('article')?.textContent.includes('Index library')`);
      assert.equal(await browser.evaluate(`document.querySelector('input[type="search"]').value`), 'source freshness');
      assert.equal(await browser.evaluate('window.unexpectedNativeCalls'), 0);
    } finally {
      await browser.evaluate('window.unmountHelp?.()');
      server.closeAllConnections(); await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    }
  });
