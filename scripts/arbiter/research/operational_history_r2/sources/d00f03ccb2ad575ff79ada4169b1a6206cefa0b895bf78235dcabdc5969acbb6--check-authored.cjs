// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=process.cwd(),prep=path.join(root,'artifacts/advanced-split/arbiter-help-preparation-r1');
const stage=path.join(prep,'authored-catalog-inputs');assert.ok(!fs.existsSync(stage));fs.mkdirSync(stage);
for(const name of fs.readdirSync(path.join(root,'packages/vscode/src/explorerHelp')).filter(n=>n.endsWith('.md'))){const proposed=path.join(prep,'files/packages/vscode/src/explorerHelp',name);fs.copyFileSync(fs.existsSync(proposed)?proposed:path.join(root,'packages/vscode/src/explorerHelp',name),path.join(stage,name));}
const build=require(path.join(root,'packages/vscode/scripts/build-explorer.cjs'));
const {HelpIndex}=require(path.join(root,'packages/vscode/out/explorerAssistance/help.js'));
const catalog=build.helpCatalog(stage),index=new HelpIndex(catalog);
for(const query of ['Arbiter Focus','Arbiter Context','Qwen','MiniLM'])assert.ok(index.search(query).length>0,query);
assert.ok(index.topic('management.service').body.includes('Qwen3-Reranker-0.6B'));
assert.ok(index.topic('explorer.search.query').body.includes('cannot find evidence omitted'));
const report={authored_parser:'actual build-explorer helpCatalog',search:'actual compiled HelpIndex',topics:catalog.topics.length,new_query_terms:4,public_outputs_written:false,native_content:'unchanged actual Rust-emitted input; new native paragraphs await root-owned generation'};
fs.writeFileSync(path.join(prep,'authored-catalog-check.json'),JSON.stringify(report,null,2)+'\n');process.stdout.write(JSON.stringify(report)+'\n');
