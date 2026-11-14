// SPDX-License-Identifier: Apache-2.0
// Exercise both real producer policies without changing native content or assets.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {readFile} from 'node:fs/promises';
import {runInNewContext} from 'node:vm';
import {isDeepStrictEqual} from 'node:util';
import {JAVASCRIPT_SUPPORTED as adminSyntax} from '../../admin/build.mjs';
const require=createRequire(new URL('../../vscode/package.json',import.meta.url));
const {build}=require('esbuild');
const {JAVASCRIPT_SUPPORTED:editorSyntax}=require('./scripts/build-explorer.cjs');
const native=JSON.parse(await readFile(new URL('../../../crates/airk-mcp/src/help/content.json',import.meta.url),'utf8'));
const manifest=JSON.parse(await readFile(new URL('../generated/help-manifest.json',import.meta.url),'utf8'));
const whitespace=/[\t ]+$/m;
const literal={text:'# Native guide \t\r\n          \r\n```sh\r\nairkctl command \\\r\n  --path "$HOME/日本語/é"  \t\r\n```\r\n',syntax:'` ${literal} \\ $() \t\n\u2028\u2029',empty:'',spaces:'  \t  '};
async function generated(value,supported){
 const result=await build({stdin:{contents:`module.exports=${JSON.stringify(value)};`,loader:'js',sourcefile:'native-content.js'},bundle:true,write:false,
  platform:'node',format:'cjs',target:'es2022',minify:true,charset:'utf8',supported});
 const code=result.outputFiles[0].text,context={module:{exports:{}}};runInNewContext(code,context);
 return {code,value:JSON.parse(JSON.stringify(context.module.exports))};
}
function sameStrings(actual,expected){
 if(typeof expected==='string'){assert.deepEqual(Buffer.from(actual,'utf8'),Buffer.from(expected,'utf8'));return;}
 if(expected&&typeof expected==='object')for(const key of Object.keys(expected))sameStrings(actual[key],expected[key]);
}
test('original supported-template codegen reproduces trailing whitespace while retaining original Help bytes',async()=>{
 const result=await generated(native,undefined);
 assert.match(result.code,whitespace,'native authored indentation reproduces the physical generated-JS defect');
 assert.ok(isDeepStrictEqual(result.value,native),'the complete native object is unchanged');sameStrings(result.value,native);
});
for(const [host,policy] of [['Admin',adminSyntax],['Explorer',editorSyntax]]){
 test(`${host} real producer policy escapes complete native Help and Unicode shell examples without changing string bytes`,async()=>{
  assert.equal(policy['template-literal'],false);
  for(const original of [literal,native,manifest]){
   const result=await generated(original,policy);
   assert.doesNotMatch(result.code,whitespace,'physical JS lines contain no trailing whitespace');
   assert.ok(isDeepStrictEqual(result.value,original),'the complete source object is unchanged');sameStrings(result.value,original);
  }
 });
}
