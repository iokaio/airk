// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { consolidateJson, consolidateToml } from './unifiedAgentConfig';
import { LaunchSpec, unifiedEntry } from './model';
import { launchDescriptorPath } from './explorerAssistanceLaunch';
import { readJsonConfig } from './agentConfig';
import { parse } from 'jsonc-parser';

export type LocalAgent = 'codex' | 'claude' | 'copilot' | 'gemini' | 'antigravity' | 'cursor' | 'roo' | 'junie';
const locations: Record<LocalAgent, string[]> = { codex: ['.codex', 'config.toml'], claude: ['.mcp.json'], copilot: ['.vscode', 'mcp.json'],
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
function recognized(spec: LaunchSpec, storage: string, command: unknown, args: unknown, rebind = false): boolean {
    if (!spec.service || typeof command !== 'string' || !path.isAbsolute(command) || !strings(args)) { return false; }
    const relative = path.relative(storage, command).split(path.sep);
    const legacyCommand = relative.join('/') === `cli/bin/${process.platform === 'win32' ? 'airk.exe' : 'airk'}`
        || relative.length === 3 && relative[0] === 'helpers' && /^[A-Za-z0-9._-]+$/.test(relative[1])
            && ['airk', 'airk.exe'].includes(relative[2]);
    if (legacyCommand && args.length === 7 && args[0] === '--root' && same(args[1], spec.root)
        && args[2] === '--data-dir' && path.isAbsolute(args[3]) && args[4] === '--collection'
        && args[5] === spec.collection && args[6] === 'mcp') { return true; }
    // Already migrated entries may need the current data location or launcher, but must retain the same native binding.
    const rest = ownLauncher(spec, command) ? forwarded(spec, args) : undefined;
    return !!rest && rest.length === 9 && rest[0] === '--root' && same(rest[1], spec.root)
        && rest[2] === '--data-dir' && path.isAbsolute(rest[3]) && rest[4] === '--collection' && rest[5] === spec.collection
        && rest[6] === '--service-project' && (rest[7] === spec.service.project
            || rebind && /^[a-f0-9]{64}$/.test(rest[7]) && !same(rest[3],spec.dataDir)) && rest[8] === 'mcp';
}

/** An Explorer Assistance entry this extension wrote: this installation's launcher and its launch descriptor. */
function recognizedAssistance(spec: LaunchSpec, storage: string, command: unknown, args: unknown): boolean {
    if (!spec.service || !ownLauncher(spec, command) || !strings(args)) { return false; }
    const rest = forwarded(spec, args);
    return !!rest && rest.length === 3 && rest[0] === 'explorer-assistance' && rest[1] === '--descriptor' && same(rest[2], launchDescriptorPath(storage));
}

/** Consolidate only recognized connections in one guarded file save. */
export function migrateAgentConfig(agent: LocalAgent, spec: LaunchSpec, storage: string, source: string, includeAssistance = true, warn: (message: string) => void = () => undefined, rebind = false): string {
    if (!spec.service) { throw new Error('Registered service binding required.'); }
    if (Buffer.byteLength(source) > 1_048_576) { throw new Error('Configuration exceeds 1 MiB.'); }
    const descriptor=launchDescriptorPath(storage);
    const options={retrievalKey:`airk-${spec.collection}`,entry:unifiedEntry(spec,includeAssistance?descriptor:undefined),warn,
        recognizes:(family:'retrieval'|'explorer'|'unified',fields:Record<string,unknown>)=>{
            if(family==='explorer')return recognizedAssistance(spec,storage,fields.command,fields.args);
            if(family==='retrieval')return recognized(spec,storage,fields.command,fields.args,rebind);
            if(!strings(fields.args))return false;
            const args=[...fields.args],at=args.indexOf('--explorer-descriptor');
            if(at>=0){if(at!==args.length-2||!same(args[at+1],descriptor))return false;args.splice(at,2);}
            return recognized(spec,storage,fields.command,args,rebind);
        }};
    const result=agent==='codex'?consolidateToml(source,options):consolidateJson(source,agent==='copilot'?'servers':'mcpServers',options);
    if(Buffer.byteLength(result)>1_048_576){throw new Error('Updated configuration would exceed 1 MiB; existing file preserved.');}
    return result;
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
    save: (destination: string, before: string, after: string) => Promise<void>, includeAssistance = true): Promise<string[]> {
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
            const after = migrateAgentConfig(agent, spec, storage, before, includeAssistance,
                warning => messages.push(`Updated ${agent} AIRK tool policy: ${warning} (${destination})`));
            if (after === before) { continue; }
            check();
            if (await validate() !== before) { throw new Error('Configuration changed during migration.'); }
            check(); await save(destination, before, after); check();
            messages.push(`Updated ${agent} AIRK connection: ${destination}. Restart the agent to reload MCP.`);
        } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { messages.push(`Could not update ${agent} AIRK connection: ${String(error)} (${destination})`); }
        }
    }
    return messages;
}
