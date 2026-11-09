// SPDX-License-Identifier: Apache-2.0
// Pure, atomic configuration transform. No filesystem or process access.
import { applyEdits, createScanner, findNodeAtLocation, getNodeValue, modify, parseTree, SyntaxKind, type Node, type ParseError } from 'jsonc-parser';
import {scanTomlStatements,TomlValue,parseTomlDeclarations,encodeTomlValue} from './agentToml';
import { EXPLORER_TOOL_RENAMES, RETRIEVAL_TOOLS, UNIFIED_TOOLS, type ServerEntry } from './model';

export type ConfigurationAgent='codex'|'claude'|'copilot';
export interface ConfigurationDeclaration {serverKey:string;containerPath:string[];fields:Record<string,unknown>}
export interface ConfigurationFile {agent:ConfigurationAgent;destination:string;source:string;sha256:string;declarations:ConfigurationDeclaration[]}
const configurationObject=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
function configurationFail(code:string):never{throw new Error(code);}
function uniqueConfiguration(node:Node):void{
    if(node.type==='object'){
        const keys=new Set<string>();for(const item of node.children??[]){const key=String(item.children?.[0].value);if(keys.has(key))configurationFail('duplicate_configuration_key');keys.add(key);}
    }
    for(const child of node.children??[])uniqueConfiguration(child);
}
function globalConfigurationTree(source:string):Node{
    const errors:ParseError[]=[],root=parseTree(source,errors,{allowTrailingComma:true});
    if(!root||root.type!=='object'||errors.length)configurationFail('invalid_json_configuration');uniqueConfiguration(root);return root;
}
/** Pure bounded declaration parsing shared by inventory, reset and uninstall.
 * This module never discovers config locations or reads the filesystem. */
export function globalConfigDeclarations(agent:ConfigurationAgent,source:string):ConfigurationDeclaration[]{
    if(Buffer.byteLength(source)>1_048_576)configurationFail('configuration_size_bound');
    if(agent==='codex')return parseTomlDeclarations(source);
    const root=globalConfigurationTree(source),value=getNodeValue(root) as Record<string,unknown>,rows:ConfigurationDeclaration[]=[];
    const group=(containerPath:string[],raw:unknown)=>{
        if(raw===undefined)return;if(!configurationObject(raw))configurationFail('invalid_mcp_server_group');
        for(const [serverKey,fields] of Object.entries(raw)){
            if(!configurationObject(fields))configurationFail('invalid_mcp_server_declaration');
            rows.push({serverKey,containerPath,fields});if(rows.length>512)configurationFail('configuration_declaration_bound');
        }
    };
    if(agent==='copilot')group(['servers'],value.servers);
    else{
        group(['mcpServers'],value.mcpServers);
        if(value.projects!==undefined){if(!configurationObject(value.projects))configurationFail('invalid_claude_projects');
            for(const [project,settings] of Object.entries(value.projects)){
                if(!configurationObject(settings))configurationFail('invalid_claude_project');
                group(['projects',project,'mcpServers'],settings.mcpServers);
            }
        }
    }
    return rows;
}
/** Transform exactly one container. Other projects/accounts/settings remain
 * byte-identical; the host still owns the file compare-and-save boundary. */
export function transformGlobalContainer(file:ConfigurationFile,containerPath:readonly string[],transform:(source:string)=>string):string{
    if(file.agent==='codex'){if(containerPath.join('.')!=='mcp_servers')configurationFail('invalid_toml_container');return transform(file.source);}
    const expected=file.agent==='copilot'?'servers':'mcpServers';
    if(containerPath.at(-1)!==expected||!file.declarations.some(row=>JSON.stringify(row.containerPath)===JSON.stringify(containerPath)))configurationFail('unknown_configuration_container');
    if(containerPath.length===1)return transform(file.source);
    const node=findNodeAtLocation(globalConfigurationTree(file.source),[...containerPath]);if(!node||node.type!=='object')configurationFail('configuration_container_changed');
    const before=file.source.slice(node.offset,node.offset+node.length),wrapped=`{${JSON.stringify(expected)}:${before}}`,after=transform(wrapped);
    const replacement=findNodeAtLocation(globalConfigurationTree(after),[expected]);if(!replacement||replacement.type!=='object')configurationFail('configuration_container_removed');
    return file.source.slice(0,node.offset)+after.slice(replacement.offset,replacement.offset+replacement.length)+file.source.slice(node.offset+node.length);
}

type Fields = Record<string, unknown>;
type Family = 'retrieval' | 'explorer' | 'unified';
interface Existing { key: string; family: Family; fields: Fields }
export interface Consolidation {
    readonly retrievalKey: string;
    /** Native-authenticated independent declarations retain their original key. */
    readonly onlyKey?:boolean;
    readonly targetKey?:string;
    readonly selectedFamily?:Family;
    readonly entry: ServerEntry;
    readonly fields?:Fields;
    recognizes(family: Family, fields: Fields): boolean;
    warn(message: string): void;
    /** Explicit uninstall removes only entries that pass the same ownership checks. */
    remove?: boolean;
    /** Disposal-only HTTP recognition. This flag cannot create a connection. */
    removeHttp?: boolean;
}
const approvals = new Set(['autoApprove','alwaysAllow']);
const allows = new Set(['enabled_tools','includeTools','tools']);
const denies = new Set(['disabled_tools','disabledTools','excludeTools']);
const launch = new Set(['command','args','type']);
const httpLaunch=new Set(['url','serverUrl','headers','http_headers','env_http_headers','bearer_token_env_var']);
const rename = (name: string) => EXPLORER_TOOL_RENAMES[name] ?? (RETRIEVAL_TOOLS.some(tool => `airk_${tool}` === name) ? name.slice(5) : name);
function originalTools(family: Family): readonly string[] {
    return family === 'retrieval' ? RETRIEVAL_TOOLS.map(name=>`airk_${name}`) : family === 'explorer' ? Object.keys(EXPLORER_TOOL_RENAMES) : UNIFIED_TOOLS;
}
function toolPolicy(value: unknown, family: Family, approval: boolean, warn: Consolidation['warn']): string[] {
    if (!Array.isArray(value) || !value.every(tool=>typeof tool==='string')) { throw new Error('Unsupported AIRK tool policy; existing configuration preserved.'); }
    const expanded = value.flatMap(name=>name==='*' ? [...originalTools(family)] : [name]);
    if (expanded.some(name=>name.includes('*'))) { throw new Error('Custom AIRK tool patterns require review; existing configuration preserved.'); }
    const names = new Set(expanded.map(rename));
    if (approval && family==='explorer' && names.has('suggest_view') && !['navigation','review','management'].every(kind=>expanded.includes(`airk_explorer_suggest_${kind}`))) {
        names.delete('suggest_view'); warn('The combined suggest_view tool needs fresh client approval; only some former view tools were preapproved.');
    }
    return [...names];
}
function merge(entries: Existing[], options: Consolidation): Fields {
    const result: Fields = Object.assign(Object.create(null),options.fields??{command:options.entry.command,args:[...options.entry.args]});
    if(options.entry.http){
        const modes=entries.flatMap(entry=>{const headers=entry.fields.http_headers??entry.fields.headers;return headers&&typeof headers==='object'&&!Array.isArray(headers)&&'X-AIRK-Response-Mode'in headers?[(headers as Fields)['X-AIRK-Response-Mode']]:[];});
        if(modes.some(mode=>mode!=='full'&&mode!=='terse')||new Set(modes).size>1)throw new Error('Conflicting HTTP response modes; existing configuration preserved.');
        if(modes.length){const field=result.bearer_token_env_var?'http_headers':'headers';result[field]={...(result[field] as Fields??{}),'X-AIRK-Response-Mode':modes[0]};}
    }
    const keys = new Set(entries.flatMap(entry=>Object.keys(entry.fields)));
    for (const key of keys) {
        const values = entries.filter(entry=>key in entry.fields);
        if (launch.has(key)||options.entry.http&&httpLaunch.has(key)) { continue; }
        if (key==='disabled' || key==='enabled') {
            if (values.some(entry=>typeof entry.fields[key]!=='boolean')) { throw new Error('Invalid AIRK enabled/disabled setting; existing configuration preserved.'); }
            result[key]=key==='disabled'?values.some(entry=>entry.fields[key]===true):!values.some(entry=>entry.fields[key]===false);continue;
        }
        if (approvals.has(key) || allows.has(key) || denies.has(key)) {
            const selected = allows.has(key) ? entries : values;
            result[key]=[...new Set(selected.flatMap(entry=>toolPolicy(entry.fields[key] ?? ['*'],entry.family,approvals.has(key)||allows.has(key),options.warn)))];
            continue;
        }
        if (key==='env') {
            const env: Fields=Object.create(null);
            for (const entry of values) {
                const item=entry.fields.env;
                if (!item || typeof item!=='object' || Array.isArray(item)) { throw new Error('Unsupported AIRK environment; existing configuration preserved.'); }
                for (const [name,value] of Object.entries(item)) {
                    if (name in env && JSON.stringify(env[name])!==JSON.stringify(value)) { throw new Error('AIRK server environments conflict; existing configuration preserved.'); }
                    env[name]=value;
                }
            }
            if(options.entry.http){
                if(Object.keys(env).some(key=>!['AIRK_CLIENT','AIRK_MCP_RESPONSE_MODE'].includes(key)))throw new Error('HTTP migration requires review of the former server environment. Existing file preserved.');
                const mode=env.AIRK_MCP_RESPONSE_MODE;if(mode!==undefined){if(mode!=='full'&&mode!=='terse')throw new Error('Invalid prior response mode; existing file preserved.');const field=result.bearer_token_env_var?'http_headers':'headers';result[field]={...(result[field] as Fields??{}),'X-AIRK-Response-Mode':mode};}
            }else result.env=env;
            continue;
        }
        if (values.some(entry=>JSON.stringify(entry.fields[key])!==JSON.stringify(values[0].fields[key]))) {
            throw new Error(`AIRK server setting ${key} conflicts; existing configuration preserved.`);
        }
        result[key]=values[0].fields[key];
    }
    return result;
}
function family(key:string, options:Consolidation):Family { return options.onlyKey&&key===options.retrievalKey&&options.selectedFamily?options.selectedFamily:key==='airk'?'unified':key===options.retrievalKey?'retrieval':'explorer'; }
function check(entries:Existing[], options:Consolidation):void {
    for (const entry of entries) {
        if ((!options.entry.http&&!(options.remove&&options.removeHttp)&&('url' in entry.fields || 'serverUrl' in entry.fields || ('type' in entry.fields && entry.fields.type!=='stdio')))||!options.recognizes(entry.family,entry.fields)) {
            throw new Error(`Unrecognized ${entry.key} connection; existing configuration preserved for review.`);
        }
    }
}
function unique(node:Node):void {
    if (node.type==='object') { const names=node.children!.map(property=>property.children![0].value); if (new Set(names).size!==names.length) { throw new Error('Duplicate configuration keys; existing file preserved.'); } }
    node.children?.forEach(unique);
}
function retainComments(before:string,after:string):string {
    const comments=(source:string)=>{
        const scanner=createScanner(source,false),result:string[]=[];
        for(let token=scanner.scan();token!==SyntaxKind.EOF;token=scanner.scan()){
            if(token===SyntaxKind.LineCommentTrivia||token===SyntaxKind.BlockCommentTrivia){result.push(source.slice(scanner.getTokenOffset(),scanner.getTokenOffset()+scanner.getTokenLength()));}
        }
        return result;
    };
    const retained=new Map<string,number>();
    for(const comment of comments(after)){retained.set(comment,(retained.get(comment)??0)+1);}
    const missing=comments(before).filter(comment=>{
        const count=retained.get(comment)??0;if(!count)return true;retained.set(comment,count-1);return false;
    });
    const eol=before.includes('\r\n')?'\r\n':'\n';
    return missing.length?missing.join(eol)+eol+after:after;
}
export function consolidateJson(source:string, group:string, options:Consolidation):string {
    const errors:ParseError[]=[], tree=parseTree(source,errors,{allowTrailingComma:true});
    if (!tree || tree.type!=='object' || errors.length) { throw new Error('Invalid MCP JSON; existing file preserved.'); }
    unique(tree);
    const entries:Existing[]=[];
    for (const key of new Set(options.onlyKey?[options.retrievalKey]:[options.retrievalKey,'airk_explorer','airk'])) {
        const node=findNodeAtLocation(tree,[group,key]);
        if (!node) { continue; }
        if (node.type!=='object') { throw new Error('Invalid AIRK server entry; existing file preserved.'); }
        entries.push({key,family:family(key,options),fields:getNodeValue(node)});
    }
    if (!entries.length || !options.remove && !entries.some(entry=>entry.family!=='explorer')) { return source; }
    check(entries,options);
    if(options.remove){
        let result=source;
        for(const entry of entries)result=applyEdits(result,modify(result,[group,entry.key],undefined,{formattingOptions:{insertSpaces:true,tabSize:2}}));
        return retainComments(source,result);
    }
    const value=merge(entries,options);
    if (!options.entry.http&&(group==='servers' || entries.some(entry=>entry.fields.type==='stdio'))) { value.type='stdio'; }
    let result=source;
    const formattingOptions={insertSpaces:true,tabSize:2};
    // Return one complete edit. The host saves it only after source/version guards.
    const target=options.targetKey??'airk';
    for (const entry of entries.filter(entry=>entry.key!==target)) { result=applyEdits(result,modify(result,[group,entry.key],undefined,{formattingOptions})); }
    const current=findNodeAtLocation(tree,[group,target]);
    if (entries.length===1 && current && JSON.stringify(getNodeValue(current))===JSON.stringify(value)) { return source; }
    return retainComments(source,applyEdits(result,modify(result,[group,target],value,{formattingOptions})));
}
interface Table { start:number; end:number; key:string; nested:string; fields:Fields; comments:string[] }
export function consolidateToml(source:string,options:Consolidation):string {
    let statements:ReturnType<typeof scanTomlStatements>,declarations:ReturnType<typeof parseTomlDeclarations>;
    try{statements=scanTomlStatements(source);declarations=parseTomlDeclarations(source);}
    catch(error){throw new Error(`${error instanceof Error&&error.message.startsWith('duplicate_')?'Duplicate':'Unsupported'} TOML configuration; existing file preserved.`);}
    const headers=statements.filter(statement=>statement.text.startsWith('[')).map(statement=>{
        const array=statement.text.startsWith('[['),parser=new TomlValue(statement.text.slice(array?2:1,array?-2:-1));
        const parts=parser.keys();parser.complete();return {...statement,parts};
    });
    const tables:Table[]=[],keys=options.onlyKey?[options.retrievalKey]:[options.retrievalKey,'airk_explorer','airk'];
    for(const [index,header] of headers.entries()){
        if(header.parts[0]!=='mcp_servers'||!keys.includes(header.parts[1]))continue;
        const key=header.parts[1],nested=header.parts.length===3&&['env','http_headers','env_http_headers'].includes(header.parts[2])?header.parts[2]:'';
        if(header.parts.length!==2&&!nested)throw new Error('Custom AIRK TOML subtables require review; existing file preserved.');
        const entry=declarations.find(row=>row.serverKey===key&&row.containerPath.length===1&&row.containerPath[0]==='mcp_servers');
        if(!entry)throw new Error('Invalid AIRK TOML table; existing file preserved.');
        const nestedTables=headers.filter(row=>row.parts[0]==='mcp_servers'&&row.parts[1]===key&&row.parts.length===3).map(row=>row.parts[2]);
        const fields=nested?entry.fields[nested] as Fields:Object.fromEntries(Object.entries(entry.fields).filter(([field])=>!nestedTables.includes(field)));
        if(!fields||typeof fields!=='object'||Array.isArray(fields))throw new Error('Invalid AIRK TOML fields; existing file preserved.');
        const end=headers[index+1]?.start??source.length;
        const comments=statements.filter(statement=>statement.start>=header.start&&statement.start<end).flatMap(statement=>statement.comments);
        tables.push({start:header.start,end,key,nested,fields,comments});
    }
    const entries:Existing[]=tables.filter(table=>!table.nested).map(table=>({key:table.key,family:family(table.key,options),fields:{...table.fields}}));
    if(declarations.some(row=>row.containerPath.length===1&&keys.includes(row.serverKey)&&!tables.some(table=>table.key===row.serverKey&&!table.nested)))throw new Error('Inline or dotted AIRK TOML requires manual review; existing file preserved.');
    if (!entries.length || !options.remove && !entries.some(entry=>entry.family!=='explorer')) { return source; }
    for (const table of tables.filter(table=>table.nested)) {
        const entry=entries.find(entry=>entry.key===table.key);
        if (!entry || entry.fields[table.nested]!==undefined) { throw new Error('Conflicting AIRK connection tables; existing file preserved.'); }
        entry.fields[table.nested]=table.fields;
    }
    check(entries,options);
    // Inline/reserved spellings cannot be silently replaced by a duplicate table.
    // Ownership and policy checks above still govern every rewritten entry.
    if(options.remove){
        let result=source;
        for(const table of [...tables].sort((a,b)=>b.start-a.start))result=result.slice(0,table.start)+table.comments.join('\n')+'\n'+result.slice(table.end);
        return result;
    }
    const fields=merge(entries,options), nested=['env','http_headers','env_http_headers'],target=options.targetKey??'airk',header=target==='airk'?'airk':JSON.stringify(target);
    const lines=[...tables.flatMap(table=>table.comments),`[mcp_servers.${header}]`,...Object.entries(fields).filter(([key])=>!nested.includes(key)).map(([key,value])=>`${key} = ${encodeTomlValue(value)}`)];
    for(const key of nested)if(fields[key])lines.push('',`[mcp_servers.${header}.${key}]`,...Object.entries(fields[key] as Fields).map(([key,value])=>`${JSON.stringify(key)} = ${encodeTomlValue(value)}`));
    const eol=source.includes('\r\n')?'\r\n':'\n';
    const replacement=lines.join(eol)+eol+eol;
    let result=source;
    for (const table of [...tables].sort((a,b)=>b.start-a.start)) { result=result.slice(0,table.start)+(table===tables[0]?replacement:'')+result.slice(table.end); }
    return result===source?source:result;
}
