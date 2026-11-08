// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const os=require('node:os');
const {execFile}=require('node:child_process');
const {promisify}=require('node:util');
const {requireHistoricalMetadata}=require('../historical-runtime-fixture.cjs');

test('historical guards distinguish real older schema from current mislabeled pairs',()=>{
    for(const version of ['1.4.0','1.8.12','1.8.13']){
        const metadata={schema:1,version};assert.equal(requireHistoricalMetadata(metadata,'fixture'),metadata);
    }
    for(const metadata of [{schema:2,version:'1.8.14'},{schema:1,version:'1.8.14'},{schema:1,version:'1.8.15'},
        {schema:1,version:'1.9.0'},{schema:1,version:'2.0.0'},{schema:2,version:'1.4.0'},{schema:1,version:'1.8.14-preview'},{}]){
        assert.throws(()=>requireHistoricalMetadata(metadata,'fixture'),/historical_fixture_only/);
    }
});

test('retired manual fixtures reject current metadata before controller loading or fixture creation',async()=>{
    const owned=await fs.mkdtemp(path.join(os.tmpdir(),'airk-historical-refusal-'));
    try{
        await fs.writeFile(path.join(owned,'airk-runtime.json'),JSON.stringify({schema:2,version:'1.8.14'}));
        const before=await fs.readdir(owned);
        for(const name of ['check-service-frontend.cjs','check-service-console-client.cjs','check-service-runtime-transitions.cjs']){
            const arguments=[path.join(__dirname,'..',name),'--runtime',owned];
            if(name.includes('transitions'))arguments.push('--previous-runtime',owned);
            await assert.rejects(promisify(execFile)(process.execPath,arguments,{windowsHide:true,timeout:10000,maxBuffer:65536}),error=>{
                assert.match(error.stderr,/historical_fixture_only/);assert.match(error.stderr,/check-service-install.py/);
                assert.doesNotMatch(error.stderr,/Cannot find module|ENOENT/);return true;
            });
        }
        assert.deepEqual(await fs.readdir(owned),before);
    }finally{await fs.rm(owned,{recursive:true,force:true});}
});
