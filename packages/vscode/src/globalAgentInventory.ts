// SPDX-License-Identifier: Apache-2.0
/** Bounded read-only inventory. Config location never grants source authority. */
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import {readJsonConfig} from './agentConfig';
import {globalConfigDeclarations,type ConfigurationAgent,type ConfigurationDeclaration,type ConfigurationFile} from './unifiedAgentConfig';
export {globalConfigDeclarations,transformGlobalContainer} from './unifiedAgentConfig';
export type GlobalAgent=ConfigurationAgent;
export interface GlobalInventoryOptions {
    home:string;env:Readonly<Record<string,string|undefined>>;platform?:NodeJS.Platform;
    /** Actual extension-host URI, never a workspace path or setting. */
    globalStoragePath?:string;
    /** Exact User directories supplied by a host/profile API. */
    vscodeUserDirectories?:readonly string[];
    check?:()=>void;
}
export interface GlobalInventoryBlocker {code:string;destination?:string}
export type GlobalDeclaration=ConfigurationDeclaration;
export type GlobalConfigFile=ConfigurationFile;
export interface GlobalConfigInventory {files:GlobalConfigFile[];checked:string[];blockers:GlobalInventoryBlocker[]}
interface Location {agent:GlobalAgent;destination:string}
const FILE_BYTES=1_048_576,TOTAL_BYTES=8*FILE_BYTES,MAX_FILES=64,MAX_PROFILES=32,MAX_DECLARATIONS=512;
function fail(code:string):never{throw new Error(code);}
function same(a:string,b:string):boolean{return process.platform==='win32'?a.toLowerCase()===b.toLowerCase():a===b;}
export function globalConfigLocations(options:GlobalInventoryOptions):{locations:Location[];vscodeRoots:string[];blockers:GlobalInventoryBlocker[]}{
    const platform=options.platform??process.platform,p=platform==='win32'?path.win32:path.posix;
    const locations:Location[]=[],vscodeRoots:string[]=[],blockers:GlobalInventoryBlocker[]=[];
    const absolute=(value:string|undefined,label:string):string|undefined=>{
        if(value===undefined)return;
        if(!value||value.length>4096||/[\x00-\x1f]/.test(value)||!p.isAbsolute(value)){blockers.push({code:'invalid_'+label});return;}
        return p.normalize(value);
    };
    const home=absolute(options.home,'user_home');if(!home)return {locations,vscodeRoots,blockers};
    const add=(agent:GlobalAgent,destination:string)=>{if(!locations.some(row=>row.agent===agent&&(platform==='win32'?row.destination.toLowerCase()===destination.toLowerCase():row.destination===destination)))locations.push({agent,destination});};
    add('codex',p.join(home,'.codex','config.toml'));
    const codex=absolute(options.env.CODEX_HOME,'codex_home');if(codex)add('codex',p.join(codex,'config.toml'));
    add('claude',p.join(home,'.claude.json'));
    const claude=absolute(options.env.CLAUDE_CONFIG_DIR,'claude_config_dir');if(claude)add('claude',p.join(claude,'.claude.json'));
    const roots=(value:string|undefined)=>{if(value&&!vscodeRoots.includes(value))vscodeRoots.push(value);};
    if(platform==='win32'){
        const base=absolute(options.env.APPDATA,'appdata')??p.join(home,'AppData','Roaming');
        for(const product of ['Code','Code - Insiders'])roots(p.join(base,product,'User'));
    }else if(platform==='darwin'){
        for(const product of ['Code','Code - Insiders'])roots(p.join(home,'Library','Application Support',product,'User'));
    }else{
        const base=absolute(options.env.XDG_CONFIG_HOME,'xdg_config_home')??p.join(home,'.config');
        for(const product of ['Code','Code - Insiders'])roots(p.join(base,product,'User'));
    }
    const portable=absolute(options.env.VSCODE_PORTABLE,'vscode_portable');if(portable)roots(p.join(portable,'user-data','User'));
    const storage=absolute(options.globalStoragePath,'vscode_global_storage');
    if(storage){
        const global=p.dirname(storage),user=p.dirname(global);
        if(p.basename(storage).toLowerCase()!=='iokaio.airk'||p.basename(global)!=='globalStorage')blockers.push({code:'unknown_vscode_profile_layout'});
        else if(p.basename(user)==='User')roots(user);
        else if(p.basename(p.dirname(user))==='profiles'&&p.basename(p.dirname(p.dirname(user)))==='User'){
            roots(p.dirname(p.dirname(user)));add('copilot',p.join(user,'mcp.json'));
        }else blockers.push({code:'unknown_vscode_profile_layout'});
    }
    if((options.vscodeUserDirectories?.length??0)>8)blockers.push({code:'vscode_profile_root_bound'});
    for(const supplied of (options.vscodeUserDirectories??[]).slice(0,8))roots(absolute(supplied,'vscode_user_directory'));
    for(const root of vscodeRoots)add('copilot',p.join(root,'mcp.json'));
    return {locations,vscodeRoots,blockers};
}
async function directDirectory(directory:string):Promise<boolean>{
    try{const info=await fs.lstat(directory);if(!info.isDirectory()||info.isSymbolicLink()||!same(await fs.realpath(directory),directory))fail('redirected_configuration_directory');return true;}
    catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return false;throw error;}
}
export async function inventoryGlobalAgentConfigs(options:GlobalInventoryOptions):Promise<GlobalConfigInventory>{
    const planned=globalConfigLocations(options),result:GlobalConfigInventory={files:[],checked:[],blockers:[...planned.blockers]},locations=[...planned.locations];
    const block=(code:string,destination?:string)=>result.blockers.push({code,destination});
    for(const root of planned.vscodeRoots){
        options.check?.();const profiles=path.join(root,'profiles');
        try{if(!await directDirectory(profiles))continue;const directory=await fs.opendir(profiles);let count=0;
            try{for await(const item of directory){options.check?.();if(++count>MAX_PROFILES){block('vscode_profile_count_bound',profiles);break;}
                if(!item.isDirectory()||item.isSymbolicLink()){block('unsupported_vscode_profile_entry',path.join(profiles,item.name));continue;}
                locations.push({agent:'copilot',destination:path.join(profiles,item.name,'mcp.json')});
            }}finally{await directory.close().catch(()=>undefined);}
        }catch{block('vscode_profiles_unreadable_or_redirected',profiles);}
    }
    // Codex named profiles are immediate *.config.toml siblings of config.toml.
    // Bound directory entries as well as matching files; never descend sessions.
    for(const location of planned.locations.filter(row=>row.agent==='codex')){
        options.check?.();const root=path.dirname(location.destination);
        try{if(!await directDirectory(root))continue;const directory=await fs.opendir(root);let count=0,profiles=0;
            try{for await(const item of directory){options.check?.();if(++count>256){block('codex_directory_entry_bound',root);break;}
                if(!item.name.endsWith('.config.toml'))continue;
                if(++profiles>32){block('codex_profile_count_bound',root);break;}
                locations.push({agent:'codex',destination:path.join(root,item.name)});
            }}finally{await directory.close().catch(()=>undefined);}
        }catch{block('codex_profiles_unreadable_or_redirected',root);}
    }
    let total=0,declarations=0;
    for(const location of locations){
        options.check?.();if(result.checked.includes(location.destination))continue;
        if(result.checked.length>=MAX_FILES){block('configuration_file_count_bound');break;}
        result.checked.push(location.destination);
        try{
            if(!await directDirectory(path.dirname(location.destination)))continue;
            const source=await readJsonConfig(location.destination);if(source===undefined)continue;
            total+=Buffer.byteLength(source);if(total>TOTAL_BYTES){block('configuration_total_size_bound');break;}
            const rows=globalConfigDeclarations(location.agent,source);declarations+=rows.length;if(declarations>MAX_DECLARATIONS){block('configuration_declaration_bound');break;}
            result.files.push({...location,source,sha256:createHash('sha256').update(source,'utf8').digest('hex'),declarations:rows});
        }catch(error){block(error instanceof Error&&/^(unsupported|invalid|duplicate|orphan|configuration|redirected)_/.test(error.message)?error.message:'configuration_unreadable_or_redirected',location.destination);}
    }
    options.check?.();return result;
}
