// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import {HttpConnectionInfo,vscodeCredentialInput,vscodeCredentialInputId,vscodeCredentialInputForId,httpUrl,preservedImportedHttpServerKey} from './serviceHttp';
import * as path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { applyEdits, findNodeAtLocation, getNodeValue, modify, Node, ParseError, parseTree } from 'jsonc-parser';
import { Client, LaunchSpec, ServerEntry, launchArgs, unifiedEntry, serializeServers, httpServerFields } from './model';

export type JsonClient = 'claude' | 'gemini' | 'antigravity';
export function isJsonClient(client: Client): client is JsonClient {
    return client === 'claude' || client === 'gemini' || client === 'antigravity';
}
export function jsonConfigPath(client: JsonClient, spec: LaunchSpec): string {
    return path.join(spec.root, ...({ claude: ['.mcp.json'], gemini: ['.gemini', 'settings.json'],
        antigravity: ['.agents', 'mcp_config.json'] }[client]));
}
const limit = 1_048_576;

function configurationTree(source: string): Node {
    if (Buffer.byteLength(source) > limit) { throw new Error('Configuration exceeds the 1 MiB editing limit. Use manual setup.'); }
    const errors: ParseError[] = [];
    const tree = parseTree(source, errors, { allowTrailingComma: true });
    if (errors.length || tree?.type !== 'object') { throw new Error('Configuration must be a valid JSON object. Fix it before installing AIRK.'); }
    const unique = (node: Node): void => {
        if (node.type === 'object') {
            const keys = node.children!.map(property => property.children![0].value);
            if (new Set(keys).size !== keys.length) { throw new Error('Configuration contains duplicate keys. Resolve them before installing AIRK.'); }
        }
        node.children?.forEach(unique);
    };
    unique(tree);
    return tree;
}

/** Shared strict parser for config edits and secret-resolution origin checks. */
export function configurationObject(source:string):Record<string,any>{return getNodeValue(configurationTree(source));}
function matchingCredentialInput(value:unknown,wanted:ReturnType<typeof vscodeCredentialInput>):boolean{
    if(!value||typeof value!=='object'||Array.isArray(value))return false;const input=value as Record<string,any>;
    return Object.keys(input).sort().join(',')==='args,command,id,type'&&input.id===wanted.id&&input.type===wanted.type&&input.command===wanted.command&&input.args&&typeof input.args==='object'&&!Array.isArray(input.args)&&Object.keys(input.args).join(',')==='connection_id'&&input.args.connection_id===wanted.args.connection_id;
}
export function syncVscodeHttpInput(source:string,info:HttpConnectionInfo,remove=false):string{
    return syncCredentialInput(source,vscodeCredentialInput(info),remove);
}
/** Historical credential disposal never manufactures a live HTTP selection. */
export function removeVscodeHttpInputForId(source:string,id:string):string{
    return syncCredentialInput(source,vscodeCredentialInputForId(id),true);
}
function syncCredentialInput(source:string,wanted:ReturnType<typeof vscodeCredentialInputForId>,remove:boolean):string{
    const tree=configurationTree(source),node=findNodeAtLocation(tree,['inputs']);
    if(node&&node.type!=='array')throw Error('VS Code inputs must be an array; existing configuration preserved.');
    const inputs:Record<string,unknown>[]=node?getNodeValue(node):[],seen=new Set<string>();
    for(const input of inputs){if(!input||typeof input!=='object'||Array.isArray(input)||typeof input.id!=='string'||seen.has(input.id))throw Error('Ambiguous VS Code inputs; existing configuration preserved.');seen.add(input.id);}
    const existing=inputs.find(input=>input.id===wanted.id);
    if(existing&&!matchingCredentialInput(existing,wanted))throw Error('Conflicting AIRK credential input; existing configuration preserved.');
    const marker='${input:'+wanted.id+'}',references=JSON.stringify(getNodeValue(tree)).split(marker).length-1;
    if(!remove&&references!==1)throw Error('AIRK credential input must belong to exactly one HTTP Authorization field.');
    if(remove){if(!existing||references)return source;}
    else if(existing)return source;
    const value=remove?inputs.filter(input=>input.id!==wanted.id):[...inputs,wanted];
    return applyEdits(source,modify(source,['inputs'],value,{formattingOptions:{insertSpaces:true,tabSize:2,eol:source.includes('\r\n')?'\r\n':'\n'}}));
}
/** No caller path is accepted. The host locates this exact workspace file and
 * validates the complete native connection before releasing a credential. */
function vscodeCredentialReferences(source:string,requireInput=true):{id:string;server:string;url:string}[]{
    const value=configurationObject(source),servers=value.servers;
    if(!servers||typeof servers!=='object'||Array.isArray(servers)||requireInput&&!Array.isArray(value.inputs))return [];
    const seen=new Set<string>();
    if(requireInput)for(const input of value.inputs){if(!input||typeof input!=='object'||Array.isArray(input)||typeof input.id!=='string'||seen.has(input.id))return [];seen.add(input.id);}
    const references:{id:string;server:string;url:string}[]=[];
    for(const [server,entry]of Object.entries(servers) as [string,any][]){
        if(!entry||typeof entry!=='object'||Array.isArray(entry)||entry.type!=='http'||'command'in entry||'args'in entry)continue;
        const auth=entry.headers?.Authorization,match=typeof auth==='string'?/^Bearer \$\{input:airkToken_([a-f0-9]{32})\}$/.exec(auth):undefined;if(!match)continue;
        try{httpUrl(entry.url);}catch{continue;}
        const id=match[1],input=vscodeCredentialInputForId(id),marker='${input:'+input.id+'}';
        if(requireInput){
            if(JSON.stringify(value).split(marker).length-1!==1)continue;
            const matches=value.inputs.filter((candidate:any)=>candidate?.id===input.id);
            if(matches.length!==1||!matchingCredentialInput(matches[0],input))continue;
        }
        references.push({id,server,url:entry.url});
    }
    return references;
}
export function vscodeCredentialConnectionIds(source:string):string[]{return vscodeCredentialReferences(source).map(value=>value.id);}
export function vscodeCredentialConnectionId(source:string):string|undefined{
    const references=vscodeCredentialReferences(source);return references.length===1?references[0].id:undefined;
}
/** Invalid command inputs must not bypass an owned manual server's disable policy. */
export function vscodeManualConnectionIds(source:string):string[]{return [...new Set(vscodeCredentialReferences(source,false).map(value=>value.id))];}
export function vscodeManualConfiguration(source:string,info:HttpConnectionInfo):boolean{
    return vscodeCredentialReferences(source,false).some(value=>value.id===info.connection_id&&value.url===info.endpoint.url
        &&(value.server==='airk'||preservedImportedHttpServerKey(info,value.server,['servers'])===value.server));
}
export function vscodeCredentialConfiguration(source:string,info:HttpConnectionInfo):boolean{
    const references=vscodeCredentialReferences(source).filter(value=>value.id===info.connection_id);
    return references.length===1&&references[0].url===info.endpoint.url
        &&(references[0].server==='airk'||preservedImportedHttpServerKey(info,references[0].server,['servers'])===references[0].server);
}
/** Insert only missing servers; retain comments, other settings and client approval policy. */
export function mergeJsonConfig(client: JsonClient, spec: LaunchSpec, source: string, entries: readonly ServerEntry[] = [unifiedEntry(spec)]): string {
    const tree = configurationTree(source);
    const servers = findNodeAtLocation(tree, ['mcpServers']);
    if (servers && servers.type !== 'object') { throw new Error('mcpServers must be an object. Existing configuration was preserved.'); }
    const wanted = JSON.parse(serializeServers(client, entries)).mcpServers as Record<string, Record<string,unknown>>;
    const missing: [string, unknown][] = [];
    // Check every entry before editing, so a conflict leaves the file exactly as it was.
    for (const [name, entry] of Object.entries(wanted)) {
        const existing = findNodeAtLocation(tree, ['mcpServers', name]);
        if (!existing) { missing.push([name, entry]); continue; }
        const value = getNodeValue(existing);
        if(entries.some(entry=>entry.http)){
            if(existing.type==='object'&&Object.entries(entry).every(([field,expected])=>{
                if(field!=='headers')return isDeepStrictEqual(value[field],expected);
                const headers=value.headers;if(!headers||typeof headers!=='object'||Array.isArray(headers))return false;
                const {['X-AIRK-Response-Mode']:mode,...rest}=headers;
                return (mode===undefined||mode==='terse'||mode==='full')&&isDeepStrictEqual(rest,{Authorization:(expected as Record<string,unknown>).Authorization});
            })&&!('command'in value)&&!('args'in value))continue;
            throw new Error(`An AIRK HTTP server named ${name} has different settings. Review it; nothing was replaced.`);
        }
        if (existing.type === 'object' && value.command === entry.command && JSON.stringify(value.args) === JSON.stringify(entry.args)
            && (!value.type || value.type === 'stdio') && !value.url && !value.serverUrl) { continue; }
        throw new Error(`An AIRK server named ${name} already has different settings. Open the configuration and review it; nothing was replaced.`);
    }
    const indent = source.match(/(?:^|\n)([\t ]+)"/u)?.[1];
    const formattingOptions = { insertSpaces: !indent?.includes('\t'), tabSize: indent?.length || 2, eol: source.includes('\r\n') ? '\r\n' : '\n' };
    return missing.reduce((result, [name, entry]) => applyEdits(result, modify(result, ['mcpServers', name], entry, { formattingOptions })), source);
}

async function configurationFile(client: JsonClient, spec: LaunchSpec, create = true): Promise<string> {
    const root = await fs.realpath(spec.root);
    const destination = jsonConfigPath(client, { ...spec, root }), directory = path.dirname(destination);
    if (directory !== root) {
        try { if (create) { await fs.mkdir(directory); } }
        catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') { throw error; } }
        const info = await fs.lstat(directory);
        if (!info.isDirectory() || info.isSymbolicLink() || await fs.realpath(directory) !== directory) {
            throw new Error('Configuration directory must be a regular directory inside this workspace. Use manual setup for redirected configuration.');
        }
    }
    return destination;
}

export interface LegacyJsonReview { readonly destination: string; readonly before: string; readonly after: string }
const legacyReviews = new WeakSet<LegacyJsonReview>();

function legacyReplacement(client: JsonClient, spec: LaunchSpec, source: string): string {
    if (!spec.service) { throw new Error('Legacy review requires a registered native service project.'); }
    const tree = configurationTree(source), name = `airk-${spec.collection}`;
    const node = findNodeAtLocation(tree, ['mcpServers', name]);
    if (node?.type !== 'object') { throw new Error('No same-named legacy AIRK server was found. Use normal installation or manual review.'); }
    const entry = getNodeValue(node);
    const args: unknown = entry.args;
    if (typeof entry.command !== 'string' || !['airk', 'airk.exe'].includes(path.basename(entry.command).toLowerCase())
        || !path.isAbsolute(entry.command) || 'url' in entry || 'serverUrl' in entry || (entry.type !== undefined && entry.type !== 'stdio')
        || !Array.isArray(args) || args.length !== 7 || args.some(value => typeof value !== 'string')
        || !path.isAbsolute(args[3])
        || JSON.stringify(args) !== JSON.stringify(launchArgs({ ...spec, service: undefined, dataDir: args[3] }))) {
        throw new Error('This entry does not match the bounded legacy AIRK launch for this root and collection. Review it manually.');
    }
    const replacement:Record<string,unknown> = spec.http?{...entry,...httpServerFields(client,spec.http)}:{...entry,command:spec.command,args:launchArgs(spec)};
    if(spec.http){
        const env=entry.env;if(env!==undefined&&(!env||typeof env!=='object'||Array.isArray(env)||Object.keys(env).some(key=>!['AIRK_CLIENT','AIRK_MCP_RESPONSE_MODE'].includes(key))))throw new Error('Review the former server environment before migrating to HTTP.');
        const mode=env?.AIRK_MCP_RESPONSE_MODE;if(mode!==undefined){if(mode!=='terse'&&mode!=='full')throw new Error('Invalid response preference; existing file preserved.');replacement.headers={...(replacement.headers as Record<string,unknown>),'X-AIRK-Response-Mode':mode};}
        delete replacement.command;delete replacement.args;delete replacement.env;
    }
    const indent = source.match(/(?:^|\n)([\t ]+)"/u)?.[1];
    return applyEdits(source, modify(source, ['mcpServers', name], replacement, { formattingOptions: {
        insertSpaces: !indent?.includes('\t'), tabSize: indent?.length || 2, eol: source.includes('\r\n') ? '\r\n' : '\n'
    } }));
}

/** Read-only candidate discovery. Matching arguments do not prove an old process owner. */
export async function reviewLegacyJsonConfig(client: JsonClient, spec: LaunchSpec, checkCurrent: () => void): Promise<LegacyJsonReview> {
    checkCurrent();
    const destination = await configurationFile(client, spec, false), before = await readJsonConfig(destination);
    if (before === undefined) { throw new Error('No project configuration exists to review.'); }
    const review = Object.freeze({ destination, before, after: legacyReplacement(client, spec, before) });
    checkCurrent();
    if (await configurationFile(client, spec, false) !== destination || await readJsonConfig(destination) !== before) {
        throw new Error('Configuration changed while preparing review. Review the current file again.');
    }
    legacyReviews.add(review); return review;
}

/** Apply only the exact reviewed candidate after fresh trust, scope and file checks. */
export async function replaceReviewedLegacyJsonConfig(client: JsonClient, spec: LaunchSpec, review: LegacyJsonReview,
    checkCurrent: () => void, save: (destination: string, before: string, after: string) => Promise<void>): Promise<void> {
    checkCurrent();
    if (!legacyReviews.has(review) || await configurationFile(client, spec, false) !== review.destination
        || legacyReplacement(client, spec, review.before) !== review.after || await readJsonConfig(review.destination) !== review.before) {
        throw new Error('Legacy configuration review expired. Review the current file again.');
    }
    checkCurrent();
    await save(review.destination, review.before, review.after);
    legacyReviews.delete(review);
}

export async function readJsonConfig(destination: string): Promise<string | undefined> {
    try {
        const info = await fs.lstat(destination);
        if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1 || info.size > limit) {
            throw new Error('Configuration must be a regular, unlinked file of at most 1 MiB. Use manual setup.');
        }
        const handle = await fs.open(destination, 'r');
        try {
            const opened = await handle.stat();
            if (info.ino !== opened.ino || info.dev !== opened.dev) { throw new Error('Configuration changed. Retry setup.'); }
            const buffer = Buffer.alloc(limit + 1);
            let length = 0;
            while (length < buffer.length) {
                const { bytesRead } = await handle.read(buffer, length, buffer.length - length, length);
                if (!bytesRead) { break; }
                length += bytesRead;
            }
            if (length > limit) { throw new Error('Configuration exceeds the 1 MiB editing limit.'); }
            return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(buffer.subarray(0, length));
        } finally { await handle.close(); }
    } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return; } throw error; }
}

/** Existing files are saved through the editor, which checks document versions and disk conflicts. */
export async function installJsonConfig(client: JsonClient, spec: LaunchSpec,
    checkCurrent: () => void, save: (destination: string, before: string, after: string) => Promise<void>,
    entries: readonly ServerEntry[] = [unifiedEntry(spec)], transformExisting: (source: string) => string = source => source): Promise<string> {
    checkCurrent();
    const destination = await configurationFile(client, spec);
    const before = await readJsonConfig(destination);
    const after = mergeJsonConfig(client, spec, transformExisting(before ?? '{}\n'), entries);
    checkCurrent();
    if (await configurationFile(client, spec) !== destination || await readJsonConfig(destination) !== before) {
        throw new Error('Configuration changed while preparing setup. Retry after reviewing the file.');
    }
    checkCurrent();
    if (before === undefined) {
        await fs.writeFile(destination, after, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    } else if (after !== before) { await save(destination, before, after); }
    return destination;
}
