// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import {credentialEnvironment,httpUrl,validateHttpConnection,sameHttpAuthority,standaloneHttpServerKey,preservedImportedHttpServerKey,validateOwnedPredecessorHttpProof,type OwnedPredecessorHttpProof,type HttpConnectionInfo} from './serviceHttp';
import { consolidateJson, consolidateToml, globalConfigDeclarations, transformGlobalContainer, type ConfigurationFile as GlobalConfigFile } from './unifiedAgentConfig';
import { LaunchSpec, unifiedEntry, httpServerFields } from './model';
import { launchDescriptorPath } from './explorerAssistanceLaunch';
import { readJsonConfig, syncVscodeHttpInput,removeVscodeHttpInputForId } from './agentConfig';
import { parse } from 'jsonc-parser';
import { createHash } from 'node:crypto';

export type LocalAgent = 'codex' | 'claude' | 'copilot' | 'gemini' | 'antigravity' | 'cursor' | 'roo' | 'junie';
export const locations: Record<LocalAgent, string[]> = { codex: ['.codex', 'config.toml'], claude: ['.mcp.json'], copilot: ['.vscode', 'mcp.json'],
    gemini: ['.gemini','settings.json'], antigravity: ['.agents','mcp_config.json'], cursor: ['.cursor','mcp.json'], roo: ['.roo','mcp.json'], junie: ['.junie','mcp','mcp.json'] };
function same(left: string, right: string): boolean {
    return path.relative(process.platform === 'win32' ? path.toNamespacedPath(left) : left,
        process.platform === 'win32' ? path.toNamespacedPath(right) : right) === '';
}

/** The per-installation stable launcher, which earlier releases wrote into agent entries. */
function installLauncher(spec: LaunchSpec): string | undefined {
    return spec.service && path.join(spec.service.installDir, process.platform === 'win32' ? 'airk-launcher.exe' : 'airk-launcher');
}
const strings = (args: unknown): args is string[] => Array.isArray(args) && args.every(value => typeof value === 'string');
/** This installation's launchers: the current command, its per-installation launcher, or the universal launcher. */
function ownLauncher(spec: LaunchSpec, command: unknown): boolean {
    return typeof command === 'string' && path.isAbsolute(command) && (same(command, spec.command) || same(command, installLauncher(spec)!));
}
/** `runtime launch --install-dir <this install> -- …rest`, or just `…rest` for the universal launcher. */
function forwarded(spec: LaunchSpec, args: string[]): string[] | undefined {
    if (args[0] !== 'runtime') { return args; }
    return args.length > 5 && args[1] === 'launch' && args[2] === '--install-dir' && same(args[3], spec.service!.installDir) && args[4] === '--' ? args.slice(5) : undefined;
}

/** Accept only extension-owned legacy launches or this installation's launchers. Never execute the old command. */
function recognized(spec: LaunchSpec, storage: string, command: unknown, args: unknown, _rebind = false): boolean {
    if (!spec.service || typeof command !== 'string' || !path.isAbsolute(command) || !strings(args)) { return false; }
    const relative = path.relative(storage, command).split(path.sep);
    const legacyCommand = relative.join('/') === `cli/bin/${process.platform === 'win32' ? 'airk.exe' : 'airk'}`
        || relative.length === 3 && relative[0] === 'helpers' && /^[A-Za-z0-9._-]+$/.test(relative[1])
            && ['airk', 'airk.exe'].includes(relative[2]);
    if (legacyCommand && args.length === 7 && args[0] === '--root' && same(args[1], spec.root)
        && args[2] === '--data-dir' && path.isAbsolute(args[3]) && args[4] === '--collection'
        && args[5] === spec.collection && args[6] === 'mcp') { return true; }
    // Recognized installed entries keep their configured source/collection scope.
    // Old physical binding/data selectors must not prevent migration after all
    // local state (including the editor's registration receipt) has been erased.
    const rest = ownLauncher(spec, command) ? forwarded(spec, args) : undefined;
    if(rest?.length===3&&rest[0]==='--root'&&same(rest[1],spec.root)&&rest[2]==='mcp')return true;
    return !!rest && rest.length === 9 && rest[0] === '--root' && same(rest[1], spec.root)
        && rest[2] === '--data-dir' && path.isAbsolute(rest[3]) && rest[4] === '--collection' && rest[5] === spec.collection
        && rest[6] === '--service-project' && /^[a-f0-9]{64}$/.test(rest[7]) && rest[8] === 'mcp';
}

/** An Explorer Assistance entry this extension wrote: this installation's launcher and its launch descriptor. */
function recognizedAssistance(spec: LaunchSpec, storage: string, command: unknown, args: unknown): boolean {
    if (!spec.service || !ownLauncher(spec, command) || !strings(args)) { return false; }
    const rest = forwarded(spec, args);
    return !!rest && rest.length === 3 && rest[0] === 'explorer-assistance' && rest[1] === '--descriptor' && same(rest[2], launchDescriptorPath(storage));
}

/** One original native-authenticated standalone declaration changes transport
 * under the same server key. Disjoint scopes and aliases are never merged. */
export function migrateStandaloneHttpEntry(agent:LocalAgent,source:string,serverKey:string,originalFields:Record<string,unknown>,connection:HttpConnectionInfo,originalConfigurationSha256:string,originSha256?:string,originContainer?:readonly string[]):string{
    const info=validateHttpConnection(connection);
    if(!['codex','claude','copilot'].includes(agent)||info.selection.kind!=='imported_legacy'||info.selection.selection.configuration_receipt!==originalConfigurationSha256)throw Error('standalone_http_origin_unconfirmed');
    const provenance=info.selection.selection.grant.provenance;
    if(provenance?.kind!=='imported_standalone_adoption'||standaloneHttpServerKey(info,serverKey,originContainer)!==serverKey||!(provenance.adoption.configuration_claims as Record<string,unknown>[]).some(claim=>claim.config_sha256===originalConfigurationSha256&&claim.original_server===serverKey&&(originSha256===undefined||claim.origin_sha256===originSha256)&&(originContainer===undefined||isDeepStrictEqual(claim.container_path,originContainer))))throw Error('standalone_http_origin_unconfirmed');
    const options={retrievalKey:serverKey,targetKey:serverKey,onlyKey:true,selectedFamily:'retrieval' as const,entry:{key:serverKey,label:serverKey,command:'',args:[],http:info},fields:httpServerFields(agent==='copilot'?'vscode':agent,info),warn:()=>undefined,
        recognizes:(_family:unknown,fields:Record<string,unknown>)=>isDeepStrictEqual(JSON.parse(JSON.stringify(fields)),JSON.parse(JSON.stringify(originalFields)))};
    if(agent==='codex')return consolidateToml(source,options);
    const transformed=consolidateJson(source,agent==='copilot'?'servers':'mcpServers',options);
    return agent==='copilot'?syncVscodeHttpInput(transformed,info):transformed;
}

/** Consolidate only recognized connections in one guarded file save. */
export function migrateAgentConfig(agent: LocalAgent, spec: LaunchSpec, storage: string, source: string, includeAssistance = true, warn: (message: string) => void = () => undefined, rebind = false, remove = false, previous?:LaunchSpec,selectedKey?:string): string {
    if (!spec.service) { throw new Error('Registered service binding required.'); }
    if (Buffer.byteLength(source) > 1_048_576) { throw new Error('Configuration exceeds 1 MiB.'); }
    const descriptor=launchDescriptorPath(storage);
    const matches=(command:unknown,args:unknown)=>recognized(spec,storage,command,args,rebind)
        ||!!previous&&!!previous.service&&same(previous.root,spec.root)&&same(previous.service.installDir,spec.service!.installDir)
            &&previous.collection===spec.collection&&recognized(previous,storage,command,args);
    const fields=spec.http?httpServerFields(agent==='copilot'?'vscode':agent,spec.http):undefined;
    const options={retrievalKey:selectedKey??`airk-${spec.collection}`,...(selectedKey?{onlyKey:true,targetKey:selectedKey,selectedFamily:selectedKey==='airk'?'unified' as const:'retrieval' as const}:{}),entry:unifiedEntry(spec,includeAssistance?descriptor:undefined),fields,warn,remove,
        recognizes:(family:'retrieval'|'explorer'|'unified',fields:Record<string,unknown>)=>{
            if(spec.http&&family==='unified'&&typeof fields.url==='string'){
                const candidate=fields.url===spec.http.endpoint.url?spec:previous?.http&&fields.url===previous.http.endpoint.url?previous:undefined;
                if(!candidate?.http)return false;
                const expected=httpServerFields(agent==='copilot'?'vscode':agent,candidate.http);
                if('command'in fields||'args'in fields||'serverUrl'in fields||fields.type!==expected.type)return false;
                if(agent==='codex'){
                    const headers=fields.http_headers as Record<string,unknown>|undefined;
                    return fields.bearer_token_env_var===expected.bearer_token_env_var&&fields.headers===undefined&&fields.env_http_headers===undefined&&(!headers||typeof headers==='object'&&!Array.isArray(headers)&&Object.keys(headers).every(key=>key==='X-AIRK-Response-Mode')&&(headers['X-AIRK-Response-Mode']==='terse'||headers['X-AIRK-Response-Mode']==='full'));
                }
                const headers=fields.headers;if(!headers||typeof headers!=='object'||Array.isArray(headers))return false;
                const {['X-AIRK-Response-Mode']:mode,...actual}=headers as Record<string,unknown>;
                return (mode===undefined||mode==='full'||mode==='terse')&&(isDeepStrictEqual(actual,{Authorization:(expected.headers as Record<string,unknown>).Authorization})||agent==='copilot'&&isDeepStrictEqual(actual,{Authorization:'Bearer ${env:'+credentialEnvironment(candidate.http)+'}'}));
            }
            if(family==='explorer')return recognizedAssistance(spec,storage,fields.command,fields.args);
            if(family==='retrieval')return matches(fields.command,fields.args);
            if(!strings(fields.args))return false;
            const args=[...fields.args],at=args.indexOf('--explorer-descriptor');
            if(at>=0){if(at!==args.length-2||!same(args[at+1],descriptor))return false;args.splice(at,2);}
            return matches(fields.command,args);
        }};
    let result=agent==='codex'?consolidateToml(source,options):consolidateJson(source,agent==='copilot'?'servers':'mcpServers',options);
    if(agent==='copilot'&&spec.http&&result!==source)result=syncVscodeHttpInput(result,spec.http,remove);
    else if(agent==='copilot'&&spec.http&&!remove&&result===source)result=syncVscodeHttpInput(result,spec.http);
    if(Buffer.byteLength(result)>1_048_576){throw new Error('Updated configuration would exceed 1 MiB; existing file preserved.');}
    return result;
}

/** Existing HTTP credentials carry their native selection. Endpoint refresh
 * must never reconstruct that authority from a configuration directory. */
export function httpConfigurationConnectionId(agent:LocalAgent,fields:Record<string,unknown>):string|undefined{
    if(typeof fields.url!=='string')return;try{httpUrl(fields.url);}catch{return;}
    if(agent==='codex'){const value=fields.bearer_token_env_var;return typeof value==='string'?/^AIRK_MCP_TOKEN_([A-F0-9]{32})$/.exec(value)?.[1].toLowerCase():undefined;}
    const headers=fields.headers;if(!headers||typeof headers!=='object'||Array.isArray(headers))return;
    const value=(headers as Record<string,unknown>).Authorization;if(typeof value!=='string')return;
    const match=/^Bearer \$\{(?:(?:env:)?AIRK_MCP_TOKEN_([A-F0-9]{32})|input:airkToken_([a-f0-9]{32}))\}$/.exec(value);
    return match?(match[1]??match[2]).toLowerCase():undefined;
}
export function refreshHttpAgentConfig(agent:LocalAgent,source:string,connection:HttpConnectionInfo,previousUrl:string,serverKey='airk',previous?:HttpConnectionInfo):string{
    const info=validateHttpConnection(connection),url=httpUrl(previousUrl),client=agent==='copilot'?'vscode':agent;
    const prior=previous===undefined?info:validateHttpConnection(previous);
    if(previous!==undefined&&(prior.selection.kind!=='imported_legacy'||info.selection.kind!=='imported_legacy'
        ||prior.selection.selection.grant.provenance.kind!=='imported_standalone_adoption'||info.selection.selection.grant.provenance.kind!=='imported_standalone_adoption'
        ||!sameHttpAuthority(info.selection,prior.selection)||info.endpoint.installation!==prior.endpoint.installation||info.endpoint.user!==prior.endpoint.user
        ||!same(info.ca_certificate_file,prior.ca_certificate_file)||url!==prior.endpoint.url))throw Error('standalone_http_reattachment_unconfirmed');
    const expected=httpServerFields(client,info);
    const priorFields=httpServerFields(client,prior);
    if(serverKey!=='airk'&&preservedImportedHttpServerKey(info,serverKey)!==serverKey)throw Error('standalone_http_alias_unconfirmed');
    const options={retrievalKey:serverKey,targetKey:serverKey,onlyKey:true,selectedFamily:'unified' as const,entry:{key:serverKey,command:'',args:[],label:'AIRK',http:info},fields:expected,warn:()=>undefined,
        recognizes:(family:string,fields:Record<string,unknown>)=>{
            if(family!=='unified'||fields.url!==url||httpConfigurationConnectionId(agent,fields)!==prior.connection_id||'command'in fields||'args'in fields||'serverUrl'in fields||fields.type!==priorFields.type)return false;
            if(agent==='codex'){
                const headers=fields.http_headers;return fields.headers===undefined&&fields.env_http_headers===undefined&&(!headers||typeof headers==='object'&&!Array.isArray(headers)&&Object.keys(headers).every(key=>key==='X-AIRK-Response-Mode')&&['full','terse'].includes(String((headers as Record<string,unknown>)['X-AIRK-Response-Mode'])));
            }
            const headers=fields.headers as Record<string,unknown>,{['X-AIRK-Response-Mode']:mode,...authorization}=headers;
            return (mode===undefined||mode==='full'||mode==='terse')&&(isDeepStrictEqual(authorization,{Authorization:(priorFields.headers as Record<string,unknown>).Authorization})||agent==='copilot'&&isDeepStrictEqual(authorization,{Authorization:'Bearer ${env:'+credentialEnvironment(prior)+'}'}));
        }};
    let result=agent==='codex'?consolidateToml(source,options):consolidateJson(source,agent==='copilot'?'servers':'mcpServers',options);
    if(agent==='copilot')result=syncVscodeHttpInput(result,info);
    if(Buffer.byteLength(result)>1_048_576)throw Error('Updated configuration would exceed 1 MiB.');return result;
}

/** A manually configured VS Code connection owns its disabled/tool policies. */
export function hasUnifiedVscodeConfig(spec: LaunchSpec, storage: string, source: string): boolean {
    if (!parse(source)?.servers?.airk) { return false; }
    try {
        migrateAgentConfig('copilot',spec,storage,source);
        return true;
    } catch { return false; }
}

/** Local workspace files only; editor save owns unsaved-edit/version/conflict checks. */
export async function migrateLocalAgents(spec: LaunchSpec, storage: string, check: () => void,
    save: (destination: string, before: string, after: string) => Promise<void>, includeAssistance = true, remove = false, previous?:LaunchSpec, resolve?: (agent:LocalAgent,before:string)=>Promise<{spec:LaunchSpec;previous?:LaunchSpec;skip?:boolean}>,observe?:(destination:string,source:string)=>void): Promise<string[]> {
    const messages: string[] = [];
    for (const agent of Object.keys(locations) as LocalAgent[]) {
        check();
        const destination = path.join(spec.root, ...locations[agent]);
        const validate = async () => {
            if (!same(await fs.realpath(spec.root), spec.root)) { throw new Error('Workspace root redirected.'); }
            const directory = path.dirname(destination);
            const info = await fs.lstat(directory);
            if (!info.isDirectory() || info.isSymbolicLink() || !same(await fs.realpath(directory), directory)) {
                throw new Error('Configuration directory redirected.');
            }
            return readJsonConfig(destination);
        };
        try {
            const before = await validate();
            if (before === undefined) { continue; }
            const resolved=resolve&&!remove?await resolve(agent,before):{spec,previous};check();
            if(resolved.skip){observe?.(destination,before);continue;}
            const after = migrateAgentConfig(agent, resolved.spec, storage, before, includeAssistance,
                warning => messages.push(`Updated ${agent} AIRK tool policy: ${warning} (${destination})`),false,remove,resolved.previous);
            if (after === before) { observe?.(destination,before);continue; }
            check();
            if (await validate() !== before) { throw new Error('Configuration changed during migration.'); }
            check(); await save(destination, before, after); check();
            const saved=await validate();
            if(saved===undefined||saved.replace(/\r\n/g,'\n')!==after.replace(/\r\n/g,'\n'))throw new Error('Agent configuration save is unconfirmed; launcher cleanup is blocked.');
            observe?.(destination,saved);
            messages.push(`${remove?'Removed':'Updated'} ${agent} AIRK connection: ${destination}. Restart the agent to reload MCP.`);
        } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { messages.push(`Could not ${remove?'remove':'update'} ${agent} AIRK connection: ${String(error)} (${destination})`); }
        }
    }
    return messages;
}

export interface CleanBreakClientFile { agent: LocalAgent; destination: string; source: string }
interface CleanBreakRemoval { serverKey: string; containerPath: string[]; fields: Record<string, unknown>; http?: OwnedPredecessorHttpProof }
interface CleanBreakClientEdit { agent: LocalAgent; destination: string; beforeSha256: string; afterSha256: string; removals: CleanBreakRemoval[] }
export interface CleanBreakClientPlan {
    schema: 1; installDir: string; files: CleanBreakClientEdit[];
    blockers: { destination: string; serverKey?: string; code: string }[];
}
const clientHash = (source: string) => createHash('sha256').update(source, 'utf8').digest('hex');
const clientFields = (value: Record<string, unknown>): Record<string, unknown> => JSON.parse(JSON.stringify(value));
const inventoryAgent = (agent: LocalAgent) => agent === 'codex' ? 'codex' : agent === 'copilot' ? 'copilot' : 'claude';

export function validateCleanBreakClientPlan(value: unknown, installDir: string): CleanBreakClientPlan {
    const plan = value as CleanBreakClientPlan;
    if (!plan || plan.schema !== 1 || typeof plan.installDir !== 'string' || !path.isAbsolute(plan.installDir) || !same(plan.installDir, installDir)
        || !Array.isArray(plan.files) || plan.files.length > 64 || !Array.isArray(plan.blockers) || plan.blockers.length > 512
        || Buffer.byteLength(JSON.stringify(plan)) > 8 * 1_048_576) { throw new Error('invalid_clean_break_client_plan'); }
    const destinations = new Set<string>(); let removals = 0;
    const destination = (name: unknown): name is string => typeof name === 'string' && path.isAbsolute(name) && name.length <= 4096 && !/[\x00-\x1f]/.test(name);
    for (const file of plan.files) {
        if (!file || !Object.hasOwn(locations, file.agent) || !destination(file.destination) || destinations.has(file.destination)
            || !/^[a-f0-9]{64}$/.test(file.beforeSha256) || !/^[a-f0-9]{64}$/.test(file.afterSha256)
            || !Array.isArray(file.removals) || !file.removals.length || (removals += file.removals.length) > 512) { throw new Error('invalid_clean_break_client_plan'); }
        destinations.add(file.destination);
        for (const removal of file.removals) {
            if (!removal || typeof removal.serverKey !== 'string' || !removal.serverKey || removal.serverKey.length > 256
                || !Array.isArray(removal.containerPath) || !removal.containerPath.length || removal.containerPath.length > 3
                || removal.containerPath.some(part => typeof part !== 'string' || !part || part.length > 4096)
                || !removal.fields || typeof removal.fields !== 'object' || Array.isArray(removal.fields)) { throw new Error('invalid_clean_break_client_plan'); }
            if (removal.http && !ownedHttpRemoval(file.agent, removal.fields, validateOwnedPredecessorHttpProof(removal.http))) { throw new Error('invalid_clean_break_client_http_proof'); }
        }
    }
    if (plan.blockers.some(blocker => !blocker || !destination(blocker.destination) || typeof blocker.code !== 'string' || blocker.code.length > 256)) { throw new Error('invalid_clean_break_client_plan'); }
    return plan;
}

function ownedLegacyRemoval(spec: LaunchSpec, storage: string, fields: Record<string, unknown>): boolean {
    // Cleanup requires the exact command supplied by the native inventory.
    // Migration's derived launcher/cache paths confer no removal authority.
    if (!spec.service || typeof fields.command !== 'string' || !path.isAbsolute(fields.command) || !same(fields.command, spec.command)) { return false; }
    if (['url', 'serverUrl', 'headers', 'http_headers', 'env_http_headers', 'bearer_token_env_var'].some(key => key in fields)) { return false; }
    if (recognizedAssistance(spec, storage, fields.command, fields.args)) { return true; }
    if (!strings(fields.args)) { return false; }
    const args = [...fields.args], at = args.indexOf('--explorer-descriptor');
    if (at >= 0) {
        if (at !== args.length - 2 || !same(args[at + 1], launchDescriptorPath(storage))) { return false; }
        args.splice(at, 2);
    }
    const selected = forwarded(spec, args);
    if (!selected) { return false; }
    for (const [flag, expected] of [['--data-dir', spec.dataDir], ['--service-project', spec.service.project]] as const) {
        const at = selected.indexOf(flag);
        if (at >= 0 && (selected.lastIndexOf(flag) !== at || at + 1 >= selected.length
            || (flag === '--data-dir' ? !path.isAbsolute(selected[at + 1]) || !same(selected[at + 1], expected) : selected[at + 1] !== expected))) { return false; }
    }
    return recognized(spec, storage, fields.command, args);
}

function ownedHttpRemoval(agent: LocalAgent, fields: Record<string, unknown>, info: OwnedPredecessorHttpProof): boolean {
    if (httpConfigurationConnectionId(agent, fields) !== info.connection_id || !['codex', 'claude', 'copilot'].includes(agent)) { return false; }
    const environment='AIRK_MCP_TOKEN_'+info.connection_id.toUpperCase();
    const expected={type:agent==='codex'?undefined:'http',url:info.endpoint.url,bearer_token_env_var:environment,headers:{Authorization:agent==='copilot'?`Bearer \${input:airkToken_${info.connection_id}}`:`Bearer \${${environment}}`}};
    if ('command' in fields || 'args' in fields || 'serverUrl' in fields || fields.type !== expected.type || fields.url !== expected.url) { return false; }
    if (agent === 'codex') {
        const headers = fields.http_headers as Record<string, unknown> | undefined;
        return fields.bearer_token_env_var === expected.bearer_token_env_var && fields.headers === undefined && fields.env_http_headers === undefined
            && (!headers || typeof headers === 'object' && !Array.isArray(headers) && Object.keys(headers).every(key => key === 'X-AIRK-Response-Mode')
                && ['terse', 'full'].includes(String(headers['X-AIRK-Response-Mode'])));
    }
    const headers = fields.headers;
    if (!headers || typeof headers !== 'object' || Array.isArray(headers)) { return false; }
    const { ['X-AIRK-Response-Mode']: mode, ...actual } = headers as Record<string, unknown>;
    return (mode === undefined || mode === 'terse' || mode === 'full') && (isDeepStrictEqual(actual, { Authorization: (expected.headers as Record<string, unknown>).Authorization })
        || agent === 'copilot' && isDeepStrictEqual(actual, { Authorization: 'Bearer ${env:' + environment + '}' }));
}

/** Remove only exact captured declarations. It never creates a replacement server or credential. */
function applyCleanBreakRemovals(agent: LocalAgent, source: string, removals: readonly CleanBreakRemoval[]): string {
    let current = source;
    for (const removal of removals) {
        const mapped = inventoryAgent(agent), declarations = globalConfigDeclarations(mapped, current);
        const row = declarations.find(entry => entry.serverKey === removal.serverKey && isDeepStrictEqual(entry.containerPath, removal.containerPath));
        if (!row || !isDeepStrictEqual(clientFields(row.fields), clientFields(removal.fields))) { throw new Error('clean_break_client_declaration_changed'); }
        const file: GlobalConfigFile = { agent: mapped, destination: '', source: current, sha256: clientHash(current), declarations };
        current = transformGlobalContainer(file, row.containerPath, wrapped => {
            const options = { retrievalKey: row.serverKey, targetKey: row.serverKey, onlyKey: true, selectedFamily: 'unified' as const,
                entry: { key: row.serverKey, label: 'AIRK', command: '', args: [] }, remove: true,removeHttp:!!removal.http, warn: () => undefined,
                recognizes: (_family: unknown, fields: Record<string, unknown>) => isDeepStrictEqual(clientFields(fields), clientFields(removal.fields)) };
            return agent === 'codex' ? consolidateToml(wrapped, options) : consolidateJson(wrapped, agent === 'copilot' ? 'servers' : 'mcpServers', options);
        });
        if (agent === 'copilot' && removal.http) { current = removeVscodeHttpInputForId(current, removal.http.connection_id); }
    }
    return current;
}

/** Capture while the original native credential owner still exists. Serialized
 * plans contain only removed declarations and hashes, never unrelated config bytes. */
export async function captureCleanBreakClientCleanup(installDir: string, storage: string, files: readonly CleanBreakClientFile[], specs: readonly LaunchSpec[],
    check: () => void, lookupHttp?: (agent: LocalAgent, connectionId: string) => Promise<OwnedPredecessorHttpProof | undefined>, verifiedLaunchers: readonly string[] = []): Promise<CleanBreakClientPlan> {
    if (!path.isAbsolute(installDir) || files.length > 64 || specs.length > 1024 || verifiedLaunchers.length > 4) { throw new Error('clean_break_client_inventory_bound'); }
    if (specs.some(spec => !spec.service || !same(spec.service.installDir, installDir)) || verifiedLaunchers.some(command => !path.isAbsolute(command))) { throw new Error('clean_break_client_installation_changed'); }
    const candidates = specs.flatMap(spec => [spec, ...verifiedLaunchers.map(command => ({ ...spec, command }))]);
    const plan: CleanBreakClientPlan = { schema: 1, installDir, files: [], blockers: [] };
    let bytes = 0, count = 0;
    const destinations = new Set<string>();
    for (const file of files) {
        check(); bytes += Buffer.byteLength(file.source);
        if (!path.isAbsolute(file.destination) || destinations.has(file.destination) || bytes > 8 * 1_048_576) { throw new Error('clean_break_client_inventory_bound'); }
        destinations.add(file.destination);
        const removals: CleanBreakRemoval[] = [], rows = globalConfigDeclarations(inventoryAgent(file.agent), file.source);
        count += rows.length; if (count > 512) { throw new Error('clean_break_client_declaration_bound'); }
        for (const row of rows) {
            let http: OwnedPredecessorHttpProof | undefined;
            const id = httpConfigurationConnectionId(file.agent, row.fields);
            if (id && lookupHttp) { const value = await lookupHttp(file.agent, id); check(); if (value) { http = validateOwnedPredecessorHttpProof(value); } }
            const httpOwned = http && ownedHttpRemoval(file.agent, row.fields, http);
            if (candidates.some(spec => ownedLegacyRemoval(spec, storage, row.fields)) || httpOwned) {
                removals.push({ serverKey: row.serverKey, containerPath: [...row.containerPath], fields: clientFields(row.fields), ...(httpOwned ? { http } : {}) });
            } else if (/^airk(?:$|[-_])/.test(row.serverKey) || id || typeof row.fields.command === 'string' && candidates.some(spec => ownLauncher(spec, row.fields.command))) {
                plan.blockers.push({ destination: file.destination, serverKey: row.serverKey, code: 'clean_break_client_ownership_unconfirmed' });
            }
        }
        if (!removals.length) { continue; }
        try {
            const after = applyCleanBreakRemovals(file.agent, file.source, removals);
            plan.files.push({ agent: file.agent, destination: file.destination, beforeSha256: clientHash(file.source), afterSha256: clientHash(after), removals });
        } catch { plan.blockers.push({ destination: file.destination, code: 'clean_break_client_configuration_requires_review' }); }
    }
    check(); if (Buffer.byteLength(JSON.stringify(plan)) > 8 * 1_048_576) { throw new Error('clean_break_client_plan_bound'); }
    return plan;
}

/** Host-owned inventory restricts destinations; save must implement the editor's
 * unsaved-edit/CAS guards. Repeated calls verify already saved exact results. */
export async function applyCleanBreakClientCleanup(plan: CleanBreakClientPlan, installDir: string, allowedDestinations: ReadonlySet<string>, check: () => void,
    read: (destination: string) => Promise<string | undefined>, save: (destination: string, before: string, after: string) => Promise<void>): Promise<{ complete: boolean; removed: string[]; blockers: CleanBreakClientPlan['blockers'] }> {
    validateCleanBreakClientPlan(plan, installDir);
    const removed: string[] = [], blockers = [...plan.blockers];
    for (const file of plan.files) {
        check();
        if (!allowedDestinations.has(file.destination) || !/^[a-f0-9]{64}$/.test(file.beforeSha256) || !/^[a-f0-9]{64}$/.test(file.afterSha256)) { throw new Error('clean_break_client_destination_unconfirmed'); }
        try {
            const before = await read(file.destination); check();
            if (before !== undefined && clientHash(before) === file.afterSha256) { removed.push(file.destination); continue; }
            const declarations = before === undefined ? [] : globalConfigDeclarations(inventoryAgent(file.agent), before);
            if (file.removals.every(removal => !declarations.some(row => row.serverKey === removal.serverKey && isDeepStrictEqual(row.containerPath, removal.containerPath)))) {
                removed.push(file.destination); continue;
            }
            if (before === undefined || clientHash(before) !== file.beforeSha256) { throw new Error('clean_break_client_configuration_changed'); }
            const after = applyCleanBreakRemovals(file.agent, before, file.removals);
            if (clientHash(after) !== file.afterSha256) { throw new Error('clean_break_client_plan_changed'); }
            check(); await save(file.destination, before, after); check();
            const confirmed = await read(file.destination); check();
            if (confirmed === undefined || clientHash(confirmed) !== file.afterSha256) { throw new Error('clean_break_client_save_unconfirmed'); }
            removed.push(file.destination);
        } catch (error) { blockers.push({ destination: file.destination, code: error instanceof Error ? error.message : 'clean_break_client_cleanup_failed' }); }
    }
    return { complete: blockers.length === 0, removed, blockers };
}
