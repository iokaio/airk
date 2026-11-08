// SPDX-License-Identifier: Apache-2.0
// A fixture compatibility guard, not payload authentication or qualification.
const fs=require('node:fs/promises');
const path=require('node:path');
const message=label=>`historical_fixture_only: ${label} requires a schema-1 AIRK runtime older than 1.8.14. For current runtimes use check-service-install.py, check-service-clients.py and check-native-editor.cjs; their coverage differs as documented in docs/one-airk.md section 9.1.`;
function requireHistoricalMetadata(value,label){
    const match=typeof value?.version==='string'&&/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.exec(value.version);
    if(!match||value.schema!==1)throw Error(message(label));
    const version=match.slice(1).map(Number),limit=[1,8,14];
    if(version.some(number=>!Number.isSafeInteger(number))||!version.some((number,index)=>version.slice(0,index).every((part,at)=>part===limit[at])&&number<limit[index]))throw Error(message(label));
    return value;
}
async function requireHistoricalRuntime(directory,label){
    const file=path.join(directory,'airk-runtime.json'),stat=await fs.lstat(file);
    if(!stat.isFile()||stat.isSymbolicLink()||stat.nlink!==1||stat.size>4096)throw Error(message(label));
    const bytes=await fs.readFile(file);if(bytes.length!==stat.size)throw Error(message(label));
    let metadata;try{metadata=JSON.parse(bytes.toString('utf8'));}catch{throw Error(message(label));}
    return requireHistoricalMetadata(metadata,label);
}
module.exports={requireHistoricalMetadata,requireHistoricalRuntime};
