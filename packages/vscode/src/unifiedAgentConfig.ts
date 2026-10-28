// SPDX-License-Identifier: Apache-2.0
// Pure, atomic configuration transform. No filesystem or process access.
import { applyEdits, createScanner, findNodeAtLocation, getNodeValue, modify, parseTree, SyntaxKind, type Node, type ParseError } from 'jsonc-parser';
import { EXPLORER_TOOL_RENAMES, RETRIEVAL_TOOLS, UNIFIED_TOOLS, type ServerEntry } from './model';

type Fields = Record<string, unknown>;
type Family = 'retrieval' | 'explorer' | 'unified';
interface Existing { key: string; family: Family; fields: Fields }
export interface Consolidation {
    readonly retrievalKey: string;
    readonly entry: ServerEntry;
    recognizes(family: Family, fields: Fields): boolean;
    warn(message: string): void;
}
const approvals = new Set(['autoApprove','alwaysAllow']);
const allows = new Set(['enabled_tools','includeTools','tools']);
const denies = new Set(['disabled_tools','disabledTools','excludeTools']);
const launch = new Set(['command','args','type']);
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
    const result: Fields = Object.assign(Object.create(null), { command: options.entry.command, args: [...options.entry.args] });
    const keys = new Set(entries.flatMap(entry=>Object.keys(entry.fields)));
    for (const key of keys) {
        const values = entries.filter(entry=>key in entry.fields);
        if (launch.has(key)) { continue; }
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
            result.env=env; continue;
        }
        if (values.some(entry=>JSON.stringify(entry.fields[key])!==JSON.stringify(values[0].fields[key]))) {
            throw new Error(`AIRK server setting ${key} conflicts; existing configuration preserved.`);
        }
        result[key]=values[0].fields[key];
    }
    return result;
}
function family(key:string, options:Consolidation):Family { return key==='airk'?'unified':key===options.retrievalKey?'retrieval':'explorer'; }
function check(entries:Existing[], options:Consolidation):void {
    for (const entry of entries) {
        if ('url' in entry.fields || 'serverUrl' in entry.fields || ('type' in entry.fields && entry.fields.type!=='stdio') || !options.recognizes(entry.family,entry.fields)) {
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
    for (const key of [options.retrievalKey,'airk_explorer','airk']) {
        const node=findNodeAtLocation(tree,[group,key]);
        if (!node) { continue; }
        if (node.type!=='object') { throw new Error('Invalid AIRK server entry; existing file preserved.'); }
        entries.push({key,family:family(key,options),fields:getNodeValue(node)});
    }
    if (!entries.some(entry=>entry.family!=='explorer')) { return source; }
    check(entries,options);
    const value=merge(entries,options);
    if (group==='servers' || entries.some(entry=>entry.fields.type==='stdio')) { value.type='stdio'; }
    let result=source;
    const formattingOptions={insertSpaces:true,tabSize:2};
    // Return one complete edit. The host saves it only after source/version guards.
    for (const entry of entries.filter(entry=>entry.key!=='airk')) { result=applyEdits(result,modify(result,[group,entry.key],undefined,{formattingOptions})); }
    const current=findNodeAtLocation(tree,[group,'airk']);
    if (entries.length===1 && current && JSON.stringify(getNodeValue(current))===JSON.stringify(value)) { return source; }
    return retainComments(source,applyEdits(result,modify(result,[group,'airk'],value,{formattingOptions})));
}
interface Table { start:number; end:number; key:string; nested:boolean; fields:Fields; comments:string[] }
export function consolidateToml(source:string,options:Consolidation):string {
    if (source.includes('"""') || source.includes("'''")) { throw new Error('Multiline TOML requires manual review; existing file preserved.'); }
    const headers=[...source.matchAll(/^[ \t]*\[([^\]\r\n]+)\][ \t]*(?:#[^\r\n]*)?\r?$/gm)];
    const tables:Table[]=[];
    const keys=[options.retrievalKey,'airk_explorer','airk'];
    for (const [index,header] of headers.entries()) {
        const normalized=header[1].replace(/"([^"\\]+)"/g,'$1');
        const key=keys.find(key=>normalized===`mcp_servers.${key}` || normalized.startsWith(`mcp_servers.${key}.`));
        if (!key) { continue; }
        const nested=normalized===`mcp_servers.${key}.env`;
        if (normalized!==`mcp_servers.${key}` && !nested) { throw new Error('Custom AIRK TOML subtables require review; existing file preserved.'); }
        if (tables.some(table=>table.key===key&&table.nested===nested)) { throw new Error('Duplicate AIRK TOML table; existing file preserved.'); }
        const headerComment=header[0].slice(header[0].indexOf(']')+1).trim();
        const end=headers[index+1]?.index ?? source.length, fields:Fields=Object.create(null), comments:string[]=headerComment?[headerComment]:[];
        for (const line of source.slice(header.index!+header[0].length,end).split(/\r?\n/)) {
            if (!line.trim()) { continue; } if (line.trimStart().startsWith('#')) { comments.push(line); continue; }
            const match=/^\s*([A-Za-z0-9_-]+|"(?:[^"\\]|\\.)+")\s*=\s*(.*?)\s*$/.exec(line);
            if (!match) { throw new Error('Unsupported TOML keys; existing file preserved.'); }
            const key=match[1].startsWith('"')?JSON.parse(match[1]):match[1];
            if (key in fields) { throw new Error('Duplicate TOML keys; existing file preserved.'); }
            try { fields[key]=JSON.parse(match[2]); } catch { throw new Error('Unsupported TOML syntax; existing file preserved.'); }
        }
        tables.push({start:header.index!,end,key,nested,fields,comments});
    }
    const entries:Existing[]=tables.filter(table=>!table.nested).map(table=>({key:table.key,family:family(table.key,options),fields:{...table.fields}}));
    if (!entries.some(entry=>entry.family!=='explorer')) { return source; }
    for (const table of tables.filter(table=>table.nested)) {
        const entry=entries.find(entry=>entry.key===table.key);
        if (!entry || entry.fields.env!==undefined) { throw new Error('Conflicting AIRK environment tables; existing file preserved.'); }
        entry.fields.env=table.fields;
    }
    check(entries,options);
    // Inline/reserved spellings cannot be silently replaced by a duplicate table.
    if (/^\s*mcp_servers\s*=/m.test(source)) { throw new Error('Inline MCP TOML needs manual review.'); }
    const fields=merge(entries,options), env=fields.env;
    const lines=[...tables.flatMap(table=>table.comments),'[mcp_servers.airk]',...Object.entries(fields).filter(([key])=>key!=='env').map(([key,value])=>`${key} = ${JSON.stringify(value)}`)];
    if (env) { lines.push('','[mcp_servers.airk.env]',...Object.entries(env as Fields).map(([key,value])=>`${JSON.stringify(key)} = ${JSON.stringify(value)}`)); }
    const eol=source.includes('\r\n')?'\r\n':'\n';
    const replacement=lines.join(eol)+eol+eol;
    let result=source;
    for (const table of [...tables].sort((a,b)=>b.start-a.start)) { result=result.slice(0,table.start)+(table===tables[0]?replacement:'')+result.slice(table.end); }
    return result===source?source:result;
}
