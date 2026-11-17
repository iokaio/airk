// SPDX-License-Identifier: Apache-2.0
import {createRequire} from 'node:module';
import {readFile,writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
const require=createRequire(new URL('../../packages/vscode/package.json',import.meta.url));
const {build}=require('esbuild');
const result=await build({entryPoints:['packages/ui/src/helpSearch.ts'],bundle:true,write:false,format:'esm',platform:'node'});
const {helpScore}=await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
const manifest=JSON.parse(await readFile('packages/ui/generated/help-manifest.json','utf8'));
const cases=[['install service without VS Code','engine.installation'],['silent install','engine.installation'],['portable locations','engine.portable'],['certificate TLS trust','engine.trust'],['choose indexes default scope','engine.agent-setup'],['lost response unknown outcome','engine.request-recovery'],['not readable no index set','engine.index-health'],['optional context Arbiter models','engine.arbiter']];
const observed=cases.map(([query,target])=>{const matches=manifest.topics.map(topic=>({id:topic.id,score:helpScore(topic,query)})).filter(x=>x.score>0).sort((a,b)=>b.score-a.score);return {query,target,rank:matches.findIndex(x=>x.id===target)+1,top:matches.slice(0,10)};});
await writeFile('artifacts/advanced-split/p9-task-findability.json',JSON.stringify({scope:'Actual merged Help search task observations; independent task/target roster, no runtime/native claim.',observed},null,2)+'\n');
console.log(observed.map(x=>({query:x.query,rank:x.rank})));
assert.ok(observed.every(x=>x.rank>0&&x.rank<=10));
