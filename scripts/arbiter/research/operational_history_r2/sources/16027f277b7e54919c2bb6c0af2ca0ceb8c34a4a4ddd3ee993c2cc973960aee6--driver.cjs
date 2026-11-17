// SPDX-License-Identifier: Apache-2.0
exports.activate=()=>(async function privateDriver(){
    const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
    const directory=process.env.AIRK_HOT_UI_RUN,config=JSON.parse(await fs.readFile(path.join(directory,'driver-config.json'),'utf8'));
    const boot=crypto.randomBytes(16).toString('hex'),profile=vscode.env.appRoot+'|'+config.profile;
    const write=async(name,value)=>{const filename=path.join(directory,name+'.json'),stage=filename+'.stage-'+crypto.randomBytes(8).toString('hex');try{await fs.writeFile(stage,JSON.stringify(value),{flag:'wx',mode:0o600});await fs.rename(stage,filename);}finally{await fs.rm(stage,{force:true});}};
    try{
        const extension=vscode.extensions.getExtension('iokaio.airk');
        if(!extension||!extension.extensionPath.startsWith(config.extensions+path.sep))throw Error('Installed AIRK escaped the private extension root');
        if(vscode.workspace.getConfiguration('airk').get('runtime.nativeExecution')!==false)throw Error('Native execution must stay disabled');
        if(typeof require(path.join(extension.extensionPath,'out/extensionUpdateLoader.js')).ExtensionUpdateLoader!=='function')throw Error('Baseline does not implement the update loader');
        await extension.activate();await write('driver-ready',{pid:process.pid,boot,profile});
        for(let sequence=1;Date.now()<config.deadline;){
            let request;try{request=JSON.parse(await fs.readFile(path.join(directory,'request-'+sequence+'.json'),'utf8'));}
            catch(error){if(error.code!=='ENOENT')throw error;await new Promise(resolve=>setTimeout(resolve,60));continue;}
            let value,error;
            try{
                if(vscode.workspace.getConfiguration('airk').get('runtime.nativeExecution')!==false)throw Error('Native execution was unexpectedly enabled');
                if(request.action==='open')value=await vscode.commands.executeCommand('airk.explorer.open',request.route);
                else if(request.action==='close'){
                    const tabs=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.label==='AIRK Explorer');
                    value=await vscode.window.tabGroups.close(tabs,true);
                }else if(request.action==='identity')value={pid:process.pid,boot,profile};
                else if(request.action==='tabs')value=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.label==='AIRK Explorer').length;
                else if(request.action==='quit'){await write('reply-'+sequence,{value:true});await vscode.commands.executeCommand('workbench.action.quit');return;}
                else throw Error('Unknown private UI action');
            }catch(problem){error=String(problem);}
            await write('reply-'+sequence,{value:value??null,error});sequence++;
        }
        throw Error('Private UI driver deadline expired');
    }catch(error){await write('driver-error',{error:String(error),stack:error.stack});}
})();