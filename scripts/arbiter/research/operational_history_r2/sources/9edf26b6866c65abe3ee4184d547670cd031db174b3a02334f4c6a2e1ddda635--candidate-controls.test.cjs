// SPDX-License-Identifier: Apache-2.0
// Private candidate tests compiled at owning public filenames; no source mount.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=path.resolve(__dirname,'../../../..');
for(const name of ['editor-owned-profile.test.cjs','editor-ui.test.cjs']){
 const filename=path.join(root,'scripts/tests',name),testModule=new Module(filename,module);
 testModule.filename=filename;testModule.paths=Module._nodeModulePaths(path.dirname(filename));
 testModule._compile(fs.readFileSync(path.join(__dirname,name),'utf8'),filename);
}
