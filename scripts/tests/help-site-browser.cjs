// SPDX-License-Identifier: Apache-2.0
// Executes the actual generated search script in a minimal DOM observer.
const assert=require('node:assert/strict');
assert.equal(process.versions.node,'24.15.0','Use the pinned site-test Node toolchain');
const fs=require('node:fs');const vm=require('node:vm');
const source=fs.readFileSync(process.argv[2],'utf8');
let listener;let focused=0;
const input={value:'',addEventListener:(name,callback)=>{assert.equal(name,'input');listener=callback;}};
const count={textContent:''};const announcement={textContent:''};
const heading={textContent:'Install AIRK',focus:options=>{assert.equal(options.preventScroll,true);focused++;}};
const rows=[{dataset:{search:'install first use verify service'},hidden:false},
            {dataset:{search:'index_status_unavailable readiness recovery'},hidden:false},
            {dataset:{search:'portable cli without vscode'},hidden:false}];
const document={querySelector:name=>({'#help-search':input,'#search-count':count,'article h1':heading,'#help-announcement':announcement}[name]),
                querySelectorAll:name=>{assert.equal(name,'[data-search]');return rows;}};
vm.runInNewContext(source,{document});
assert.equal(focused,1);assert.equal(heading.tabIndex,-1);assert.equal(announcement.textContent,'Opened Install AIRK');
input.value='readiness index_status_unavailable';listener();
assert.deepEqual(rows.map(row=>row.hidden),[true,false,true]);assert.equal(count.textContent,'1 of 3 topics');
input.value='  PORTABLE CLI  ';listener();assert.deepEqual(rows.map(row=>row.hidden),[true,true,false]);
input.value='';listener();assert.deepEqual(rows.map(row=>row.hidden),[false,false,false]);
console.log('full-content search and article focus passed');
