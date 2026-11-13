// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as path from 'node:path';
import type { VectorStatus } from './vectorModel';
import { validateCatalogSession, validateCombinedCatalogSession, type CatalogSessionSelector, type CombinedCatalogSessionSelector } from './catalogSession';
import { HttpConnectionInfo, credentialEnvironment, validateHttpConnection, vscodeCredentialInput, vscodeCredentialInputId } from './serviceHttp';

export interface LaunchSpec { command: string; root: string; dataDir: string; collection: string; name: string;
    /** Protected native enrollment; command remains the interactive CLI image only. */
    http?:HttpConnectionInfo;
    /** Explicit native-reviewed selection; display origins remain native Index metadata. */
    catalogSession?: CatalogSessionSelector;
    combinedSession?: CombinedCatalogSessionSelector;
    /** The window's granted search scope (W4): reads name no selection and run
     * under the window scope (contracts §3.7). `key` is a stable 64-hex key of
     * the selection (Workbench history); `service.project` carries the same key. */
    windowScope?: { readonly key: string; readonly label: string };
    /** (U105) Explicitly named indexes and index sets (ids or names), read through the
     * service's open read path (`tools call --index … --set …`). `key` is a stable 64-hex key
     * of the selection (Workbench history); `service.project` carries the same key and is never sent. */
    openSelection?: { readonly key: string; readonly label: string; readonly indexes: readonly string[]; readonly sets: readonly string[] };
    /** `universal`: `command` is the version-free universal launcher, which finds the install root itself. */
    service?: { installDir: string; project: string; universal?: boolean; direct?: boolean } }
export interface Usage { collection: string; disk_bytes: number; indexed_files: number | null; generation: number | null; status: string }
export interface IndexReport { generation: number; indexed: number; reused: number; removed: number; source_bytes?: number; skipped_truncated?: number;
    repaired_retained_artifacts?: number; retained_repair_retries?: number; skipped: { path: string; reason: string }[];
    path_scoped?: boolean; yield_to_source?: boolean; scan_extract_us?: number; publish_us?: number; first_ready_us?: number | null; fully_ready_us?: number | null;
    embedding_model_load_us?: number; embedding_inference_us?: number; vector_graph_insert_us?: number;
    indexed_vectors?: number; semantic_readiness?: VectorStatus['semantic_readiness']; discovery_readiness?: VectorStatus['discovery_readiness'];
    maintenance_error?: string | null }
export type IndexMode = 'manual' | 'onSave' | 'debounced' | 'idle';
/** Result of the extension's own MCP handshake check for one collection (not proof that an agent connected). */
export type McpVerification = import('./explorerContracts').ConnectionCheck;
export const clients = ['vscode', 'claude', 'codex', 'cursor', 'gemini', 'antigravity', 'cline', 'roo', 'junie', 'copilot-cli', 'opencode', 'opencode-v2', 'kilo', 'amazon-q', 'windsurf'] as const;
export type Client = typeof clients[number];

export function collectionId(canonicalRoot: string, platform = process.platform): string {
    const identity = platform === 'win32' ? canonicalRoot.toLowerCase() : canonicalRoot;
    return 'project-' + createHash('sha256').update(identity).digest('hex').slice(0, 24);
}

export function launchArgs(spec: LaunchSpec, command = 'mcp', pinProject = false): string[] {
    if(spec.combinedSession!==undefined){
        if(spec.catalogSession!==undefined)throw new Error('Cannot mix catalog session selectors.');
        if(!spec.service||!path.isAbsolute(spec.service.installDir))throw new Error('Invalid trusted combined catalog launch binding.');
        const selected=['--combined-session',JSON.stringify(validateCombinedCatalogSession(spec.combinedSession)),command];
        return spec.service.direct&&command!=='mcp'?['--service-install-dir',spec.service.installDir,...selected]:spec.service.universal?selected:['runtime','launch','--install-dir',spec.service.installDir,'--',...selected];
    }
    if(spec.catalogSession!==undefined){
        if(!spec.service||!path.isAbsolute(spec.service.installDir)||!/^[a-f0-9]{64}$/.test(spec.service.project))throw new Error('Invalid trusted catalog launch binding.');
        const selected=['--catalog-session',JSON.stringify(validateCatalogSession(spec.catalogSession)),command];
        return spec.service.direct&&command!=='mcp'?['--service-install-dir',spec.service.installDir,...selected]:spec.service.universal?selected:['runtime','launch','--install-dir',spec.service.installDir,'--',...selected];
    }
    if(spec.service){
        if(!path.isAbsolute(spec.service.installDir)||!/^[a-f0-9]{64}$/.test(spec.service.project))throw new Error('Invalid trusted service launch binding.');
        const scoped=command==='mcp'&&!pinProject?['--root',spec.root,'mcp']
            :['--root',spec.root,'--data-dir',spec.dataDir,'--collection',spec.collection,'--service-project',spec.service.project,command];
        return spec.service.direct&&command!=='mcp'?['--service-install-dir',spec.service.installDir,...scoped]:spec.service.universal?scoped:['runtime','launch','--install-dir',spec.service.installDir,'--',...scoped];
    }
    return ['--root', spec.root, '--data-dir', spec.dataDir, '--collection', spec.collection, command];
}

export function eligibleEvent(file: string, root: string, dataDir: string): boolean {
    const relative = path.relative(root, file);
    if (!relative || relative.startsWith('..' + path.sep) || relative === '..' || path.isAbsolute(relative)) { return false; }
    const inData = path.relative(dataDir, file);
    if (!inData.startsWith('..' + path.sep) && inData !== '..' && !path.isAbsolute(inData)) { return false; }
    return !relative.split(path.sep).some(part => {
        const name = part.toLowerCase();
        return ['.git', '.airk', 'node_modules', 'target', '.venv', '__pycache__', '.aws', '.ssh', '.env'].includes(name)
            || name.startsWith('.env.') || name.endsWith('.pem') || name.endsWith('.key');
    });
}

/** One MCP server in an agent configuration. Legacy entries exist only for migration. */
export interface ServerEntry { readonly key: string; readonly command: string; readonly args: readonly string[]; readonly label: string; readonly http?:HttpConnectionInfo;
    /** HTTPS only: per-connection credential command for clients outside VS Code. */
    readonly helper?:HttpCredentialHelper }
export const ASSISTANCE_KEY = 'airk_explorer';
export const UNIFIED_KEY = 'airk';
export const RETRIEVAL_TOOLS = ['capabilities','changes','code_facts','context','document_facts','file_info','help','imports','list_files','outline','query_scope','read_file','references','relationships','search','status','diagnostics','symbols'] as const;
export const UNIFIED_TOOLS = [...RETRIEVAL_TOOLS, 'parse', 'projects', 'draft_search', 'draft_strategy', 'draft_plan', 'suggest_view', 'list_templates', 'get_template', 'validate_draft', 'proposal_status', 'get_handoff'] as const;
export const EXPLORER_TOOL_RENAMES: Readonly<Record<string,string>> = {
    airk_explorer_help_search:'help', airk_explorer_help_get:'help', airk_explorer_ui_status:'status',
    airk_explorer_suggest_search:'draft_search', airk_explorer_suggest_strategy:'draft_strategy', airk_explorer_suggest_plan:'draft_plan',
    airk_explorer_suggest_navigation:'suggest_view', airk_explorer_suggest_review:'suggest_view', airk_explorer_suggest_management:'suggest_view',
    airk_explorer_templates_list:'list_templates', airk_explorer_template_get:'get_template',
    airk_explorer_validate_draft:'validate_draft', airk_explorer_proposal_status:'proposal_status', airk_explorer_handoff_get:'get_handoff',
};

/** One public endpoint; editor attachment never chooses the retrieval binding. */
export function unifiedEntry(spec: LaunchSpec, descriptor?: string, additional: readonly LaunchSpec[] = []): ServerEntry {
    if(spec.http){if(additional.length)throw new Error('Explicit combined HTTP selection required.');return {key:'airk',command:spec.command,args:[],label:'AIRK',http:validateHttpConnection(spec.http)};}
    if(additional.length&&(spec.catalogSession!==undefined||spec.combinedSession!==undefined||additional.some(value=>value.catalogSession!==undefined||value.combinedSession!==undefined)))throw new Error('Catalog sessions support one explicit native selection; a legacy catalog session supports one selected Index.');
    const args = launchArgs(spec);
    if (descriptor) {
        if (!path.isAbsolute(descriptor)) { throw new Error('Invalid Explorer descriptor.'); }
        args.push('--explorer-descriptor', descriptor);
    }
    for (const other of additional) {
        if (!other.service || !spec.service || other.service.installDir !== spec.service.installDir || other.service.project === spec.service.project) {
            throw new Error('Additional projects must be distinct bindings of the same service.');
        }
        args.push('--additional-root', other.root);
    }
    if (additional.length) { args.push('--no-default-project'); }
    return { key: UNIFIED_KEY, command: spec.command, args, label: 'AIRK (indexed evidence, help and Explorer drafts)' };
}

/** AIRK Retrieval: the native read-only evidence tools for one collection. */
export function retrievalEntry(spec: LaunchSpec): ServerEntry {
    return { key: 'airk-' + spec.collection, command: spec.command, args: launchArgs(spec), label: 'AIRK Retrieval (evidence)' };
}

/**
 * AIRK Explorer Assistance for external agents: the stable launcher forwards to the runtime's
 * descriptor-checked `explorer-assistance` subcommand, which starts the extension's bundled server.
 */
export function assistanceEntry(spec: LaunchSpec, descriptor: string): ServerEntry | undefined {
    if (!spec.service || !path.isAbsolute(descriptor)) { return undefined; }
    const forwarded = ['explorer-assistance', '--descriptor', descriptor];
    return { key: ASSISTANCE_KEY, command: spec.command, label: 'AIRK Explorer Assistance (help and UI status)',
        args: spec.service.universal ? forwarded : ['runtime', 'launch', '--install-dir', spec.service.installDir, '--', ...forwarded] };
}

/** A one-shot native credential command that a client runs per connection.
 * It prints `{"Authorization":"Bearer …"}` and exits; no secret is stored in
 * the configuration file or the user environment. */
export interface HttpCredentialHelper { readonly executable: string; readonly installDir: string }
/** Characters a client shell (cmd, sh/bash) passes through unquoted. */
const SHELL_PLAIN = /^[A-Za-z0-9_.:\\/+-]+$/;
function helperPath(value: string): string {
    // Reject anything a shell could reinterpret inside double quotes, and
    // namespaced/UNC spellings whose doubled backslashes a POSIX shell collapses.
    if (!path.isAbsolute(value) || value.length > 1024 || /["'`$%!\r\n\x00-\x1f]/.test(value) || /^\\\\/.test(value)) { throw new Error('http_credential_helper_path_unsupported'); }
    return SHELL_PLAIN.test(value) ? value : `"${value}"`;
}
export function credentialHelperCommand(helper: HttpCredentialHelper, connectionId: string): string {
    if (!/^[a-f0-9]{32}$/.test(connectionId)) { throw new Error('invalid_http_connection_id'); }
    return `${helperPath(helper.executable)} runtime http-credential --install-dir ${helperPath(helper.installDir)} --connection-id ${connectionId} --format header-json`;
}
/** The program a client shell command starts: its first token, quoted or plain. */
export function commandExecutable(command: unknown): string | undefined {
    if (typeof command !== 'string' || command.length > 4096) { return undefined; }
    const match = /^\s*(?:"([^"]+)"|([^\s"]+))/.exec(command);
    return match ? match[1] ?? match[2] : undefined;
}
/** The connection ID of an AIRK credential helper command, or undefined for any other command. */
export function credentialHelperConnectionId(command: unknown): string | undefined {
    if (typeof command !== 'string' || command.length > 4096) { return undefined; }
    const match = /^(?:"[^"]+"|[^\s"]+) runtime http-credential --install-dir (?:"[^"]+"|[^\s"]+) --connection-id ([a-f0-9]{32}) --format header-json$/.exec(command);
    return match?.[1];
}
/** Serialize entries in the selected client's configuration format. A helper,
 * when supplied, delivers the credential to clients started outside VS Code:
 * Claude Code `headersHelper` overrides the static Authorization header, and
 * Codex `http_headers_helper` replaces `bearer_token_env_var` (an explicit
 * bearer variable would take precedence over the helper and fail when unset). */
export function httpServerFields(client:Client,info:HttpConnectionInfo,helper?:HttpCredentialHelper):Record<string,unknown>{
    info=validateHttpConnection(info);const environment=credentialEnvironment(info),url=info.endpoint.url;
    if(client==='codex')return helper?{url,http_headers_helper:credentialHelperCommand(helper,info.connection_id),http_headers:{'X-AIRK-Response-Mode':'terse'}}
        :{url,bearer_token_env_var:environment,http_headers:{'X-AIRK-Response-Mode':'terse'}};
    if(client==='vscode')return {type:'http',url,headers:{Authorization:`Bearer \${input:${vscodeCredentialInputId(info)}}`,'X-AIRK-Response-Mode':'terse'}};
    if(client==='claude')return {type:'http',url,headers:{Authorization:`Bearer \${${environment}}`,'X-AIRK-Response-Mode':'terse'},...(helper?{headersHelper:credentialHelperCommand(helper,info.connection_id)}:{})};
    throw new Error(`http_client_compatibility_unverified: ${client}`);
}
export function serializeServers(client: Client, entries: readonly ServerEntry[]): string {
    if(entries.some(entry=>entry.http)){
        if(entries.some(entry=>!entry.http)||entries.length!==1)throw new Error('One explicitly enrolled HTTP endpoint required.');
        const info=validateHttpConnection(entries[0].http),name=entries[0].key,fields=httpServerFields(client,info,entries[0].helper);
        // These formats have explicit URL + host-environment references. Other
        // clients remain compatibility blockers until their real schema is qualified.
        if(client==='codex')return `[mcp_servers.${name}]\n`+Object.entries(fields).filter(([,value])=>typeof value!=='object').map(([key,value])=>`${key} = ${JSON.stringify(value)}\n`).join('')+
            Object.entries(fields).filter(([,value])=>typeof value==='object').map(([key,value])=>`\n[mcp_servers.${name}.${key}]\n`+Object.entries(value as Record<string,unknown>).map(([name,value])=>`${JSON.stringify(name)} = ${JSON.stringify(value)}\n`).join('')).join('');
        return JSON.stringify({[client==='vscode'?'servers':'mcpServers']:{[name]:fields},...(client==='vscode'?{inputs:[vscodeCredentialInput(info)]}:{})},null,2)+'\n';
    }
    if (client === 'amazon-q' || client === 'windsurf') {
        // Connection details only: these clients' agent/version-specific schemas differ.
        const details = (entry: ServerEntry) => ({ transport: 'stdio', command: entry.command, args: entry.args });
        return JSON.stringify(entries.length === 1 ? details(entries[0]) : Object.fromEntries(entries.map(entry => [entry.key, details(entry)])), null, 2) + '\n';
    }
    if (client === 'codex') {
        return entries.map(entry => `[mcp_servers.${entry.key}]\ncommand = ${JSON.stringify(entry.command)}\nargs = ${JSON.stringify(entry.args)}\n`).join('\n');
    }
    const servers: Record<string, unknown> = {};
    for (const entry of entries) {
        const server: Record<string, unknown> = { command: entry.command, args: entry.args };
        if (client === 'vscode') { servers[entry.key] = { type: 'stdio', ...server }; }
        else if (client === 'opencode' || client === 'opencode-v2' || client === 'kilo') { servers[entry.key] = { type: 'local', command: [entry.command, ...entry.args] }; }
        else {
            if (client === 'cursor') { server.type = 'stdio'; }
            if (client === 'copilot-cli') { server.type = 'local'; server.tools = ['*']; }
            servers[entry.key] = server;
        }
    }
    const config = client === 'vscode' ? { servers } : client === 'opencode-v2' ? { mcp: { servers } }
        : client === 'opencode' || client === 'kilo' ? { mcp: servers } : { mcpServers: servers };
    return JSON.stringify(config, null, 2) + '\n';
}

/** One unified public connection. */
export function configFor(client: Client, spec: LaunchSpec): string {
    return serializeServers(client, [unifiedEntry(spec)]);
}
