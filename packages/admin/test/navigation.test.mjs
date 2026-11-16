// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const bundle=await build({stdin:{contents:"export * from '../ui/src/navigation';",resolveDir:fileURLToPath(new URL('../',import.meta.url)),sourcefile:'navigation-test.ts'},bundle:true,write:false,format:'esm',platform:'node'});
const {MAIN_DESTINATIONS,DESTINATION_ICONS,editorDestination}=await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const destinations=['overview','search','indexes','activity','agents','service','code','docs','evidence','review','strategies','help'];
test('twelve independently named rail destinations each have a unique code-native icon and Help is last',()=>{
 assert.deepEqual(MAIN_DESTINATIONS.map(entry=>entry.id),destinations);
 const icons=destinations.map(id=>DESTINATION_ICONS[id]);assert.ok(icons.every(path=>typeof path==='string'&&path.length>10));assert.equal(new Set(icons).size,12);
});
test('every evidence view highlights its own destination while management aliases retain theirs',()=>{
 for(const [route,destination]of [['code/references','code'],['docs/facts','docs'],['evidence/checker','evidence'],['review/tests','review'],['strategies/studio','strategies'],['manage/agents','agents'],['manage/storage','service'],['manage/logs','activity'],['indexing/projects','indexes'],['search/tools','search']])assert.equal(editorDestination(route),destination,route);
});
