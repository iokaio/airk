// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { identifier, MutationIntent, record, RequestKey, ServiceClient, ServiceError, validateMutationIntent, count } from './serviceClient';

export interface IntentStorage {
    keys(): readonly string[];
    get<T>(key: string): T | undefined;
    update(key: string, value: unknown): Thenable<void>;
}
export interface LifecycleIntent { lifecycle:{caller:string;key:RequestKey;action:'stop'|'restart';expected_instance:string;runtime:string} }
export type ConsoleIntent=MutationIntent|LifecycleIntent;
export interface SavedIntent { schema: 1; scope: string; title: string; intent: ConsoleIntent }
function validate(input:unknown):ConsoleIntent {
    const object=record(input);
    if(!('lifecycle' in object))return validateMutationIntent(input);
    if(Object.keys(object).join(',')!=='lifecycle')throw new ServiceError('invalid_lifecycle_intent');
    const value=record(object.lifecycle),key=record(value.key);
    if(Object.keys(value).sort().join(',')!=='action,caller,expected_instance,key,runtime'
        ||!['stop','restart'].includes(String(value.action))||Object.keys(key).sort().join(',')!=='epoch,namespace,nonce')throw new ServiceError('invalid_lifecycle_intent');
    identifier(value.caller,16);identifier(value.expected_instance,16);identifier(value.runtime,32);
    identifier(key.namespace,16);identifier(key.nonce,16);count(key.epoch);
    return input as LifecycleIntent;
}
function request(intent:ConsoleIntent){
    return 'enqueue' in intent?intent.enqueue:'retry' in intent?intent.retry:'plan_create' in intent?intent.plan_create:intent.lifecycle;
}
const PREFIX = 'airk.service.intent.1.';
const LIMIT = 64;
const MAX_BYTES = 32768;
/** Each immutable request has its own storage key: windows never rewrite a shared list. */
export class IntentJournal {
    readonly scope: string;
    private tail: Promise<unknown> = Promise.resolve();
    constructor(private readonly storage: IntentStorage, control: string, private readonly namespace: string) {
        identifier(namespace,16);
        this.scope=createHash('sha256').update(JSON.stringify([control,namespace])).digest('hex');
    }
    private key(id:string):string { return `${PREFIX}${this.scope}.${identifier(id,16)}`; }
    list(): {id:string;value:SavedIntent}[] {
        const keys=this.storage.keys().filter(key=>key.startsWith(PREFIX));
        // Memento does not provide a cross-window count transaction. Racing
        // windows can exceed the admission count; page those records instead
        // of poisoning the journal or discarding an unknown outcome.
        return keys.filter(key=>key.startsWith(`${PREFIX}${this.scope}.`)).sort().slice(0,LIMIT).map(key=>{
            const value=record(this.storage.get(key));
            if(Buffer.byteLength(JSON.stringify(value))>MAX_BYTES||value.schema!==1||value.scope!==this.scope
                ||typeof value.title!=='string'||value.title.length>160)throw new ServiceError('local_intent_record_invalid');
            const intent=validate(value.intent);
            const requestKey=request(intent).key;
            if(requestKey.namespace!==this.namespace)throw new ServiceError('local_intent_scope_mismatch');
            const id=identifier(key.slice(key.lastIndexOf('.')+1),16);
            if(requestKey.nonce!==id)throw new ServiceError('local_intent_identity_mismatch');
            return {id,value:value as unknown as SavedIntent};
        });
    }
    private serialize<T>(action:()=>Promise<T>):Promise<T> {
        const next=this.tail.then(action);this.tail=next.catch(()=>undefined);return next;
    }
    prepare(title:string,intent:ConsoleIntent):Promise<string> {
        // Copy before any asynchronous work; the exact persisted bytes are submitted.
        const copied=validate(JSON.parse(JSON.stringify(intent)));
        const key=request(copied).key;
        const id=identifier(key.nonce,16);
        if(key.namespace!==this.namespace)throw new ServiceError('local_intent_scope_mismatch');
        const value:SavedIntent={schema:1,scope:this.scope,title,intent:copied};
        return this.serialize(async()=>{
            if(!title||title.length>160||Buffer.byteLength(JSON.stringify(value))>MAX_BYTES)throw new ServiceError('local_intent_record_oversize');
            this.list();
            const existing=this.storage.get(this.key(id));
            if(existing!==undefined){
                const saved=record(existing);
                if(saved.schema!==1||saved.scope!==this.scope||!isDeepStrictEqual(saved.intent,copied))throw new ServiceError('local_intent_key_conflict');
                return id;
            }
            if(this.storage.keys().filter(key=>key.startsWith(PREFIX)).length>=LIMIT)throw new ServiceError('local_intent_capacity_exceeded');
            await this.storage.update(this.key(id),value);
            return id;
        });
    }
    async submit(client:ServiceClient|undefined,id:string,onReceipt?:(response:Record<string,unknown>)=>Promise<void>,
        lifecycle?:(intent:LifecycleIntent)=>Promise<Record<string,unknown>>):Promise<Record<string,unknown>> {
        const saved=this.list().find(entry=>entry.id===id);
        if(!saved)throw new ServiceError('local_intent_not_found');
        // Errors, including a lost acknowledgement, preserve the immutable record.
        const intent=saved.value.intent;
        if('lifecycle' in intent&&!lifecycle)throw new ServiceError('native_lifecycle_transport_required');
        if(!('lifecycle' in intent)&&!client)throw new ServiceError('console_not_connected');
        const response='lifecycle' in intent?await lifecycle!(intent):await client!.submitMutation(intent);
        await onReceipt?.(response);
        await this.forget(id);
        return response;
    }
    forget(id:string):Promise<void> {
        return this.serialize(async()=>{await this.storage.update(this.key(id),undefined);});
    }
}
