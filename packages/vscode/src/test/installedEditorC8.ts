// SPDX-License-Identifier: Apache-2.0
/** Installed-driver acceptance; callbacks use the real selected HTTPS and Management UI. */
import {strict as assert} from 'node:assert';
import * as net from 'node:net';
import {ServiceError} from '../serviceClient';
import type {HttpConnectionInfo} from '../serviceHttp';
import {installedHttpIdentity} from './installedEditorHttp';

type Row = Record<string, any>;
export interface InstalledEditorC8Host {
    /** Original shared scenario deadline; this helper never extends it. */
    deadline: number;
    /** Existing driver ui mailbox, handled by actual editor-ui clicks/confirmation. */
    ui(action: string, input?: Row, route?: string): Promise<Row>;
    /** RuntimeHost.client(binding).rpc, bound to the exact private installation. */
    rpc(method: unknown, expected?: string): Promise<Row>;
    /** Existing Mcp.call on the selected actual extension definition, with its verified CA. */
    call(name: string, args: Row): Promise<Row>;
    /** Separate installed Assistance descriptor; combined diagnostics stays unmodified. */
    uiStatus(): Promise<Row>;
    /** Existing selected-authority/citation assertions, with no inference requirement. */
    lexical(): Promise<void>;
    mark(name: string, value: unknown): Promise<unknown>;
}

export interface InstalledEditorC8Definition {
    uri: {toString(): string}; headers: Record<string, string>; version?: string;
    ca: Buffer; nativeConnection: HttpConnectionInfo;
}
export interface InstalledEditorC8EndpointHost {
    deadline: number;
    ui(action: string, input?: Row, route?: string): Promise<Row>;
    /** Existing definitions() validates actual extension API output against native connection material. */
    definitions(): Promise<InstalledEditorC8Definition[]>;
    /** Exact-owned installation identity required by existing serviceStop UI action. */
    installation: {installDir: string; runtimeSha256: string};
    /** A NEW Mcp using this definition and its CA: initialize, native status/scope checks, close. */
    verifyFresh(definition: InstalledEditorC8Definition): Promise<void>;
    /** Private fixture configs with distinct native-issued client credentials. */
    prepareOwnedConfigurations():Promise<void>;
    /** Observe automatic extension refresh; never invoke Repair or rewrite here. */
    verifyOwnedConfigurations():Promise<void>;
    mark(name: string, value: unknown): Promise<unknown>;
}

/** Installed relocation gate; the old-port listener never serves a protocol. */
export async function checkInstalledEditorC8Endpoint(host: InstalledEditorC8EndpointHost): Promise<Row> {
    const deadline = Math.min(host.deadline, Date.now() + 90000);
    const evidence: Row = {scope: 'installed extension definition refresh after owned-port collision',
        stop_dispatches: 0, start_dispatches: 0, occupant_connections: 0,
        occupant_contact_origin: 'unattributed', occupant_application_data: 'not_inspected'};
    const remaining = () => {const value = deadline - Date.now(); assert.ok(value > 0, 'Installed endpoint deadline'); return value;};
    const bounded = async <T>(operation: () => Promise<T>): Promise<T> => {
        const timeout=remaining();
        let timer: ReturnType<typeof setTimeout> | undefined;
        try {return await Promise.race([operation(), new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new Error('Installed endpoint original deadline exhausted')), timeout);
        })]);} finally {if (timer !== undefined) clearTimeout(timer);}
    };
    const scope = (connection: HttpConnectionInfo) => {
        const value = structuredClone(connection.selection);
        delete (value.selection as Row).expected_catalog_revision;
        return value;
    };
    const snapshot = (definition: InstalledEditorC8Definition) => ({
        credential_id: installedHttpIdentity(definition).credentialId,
        url: installedHttpIdentity(definition).url, version: definition.version,
        endpoint: definition.nativeConnection.endpoint,
    });
    let occupant: net.Server | undefined;
    try {
        const definitions = await bounded(() => host.definitions());
        assert.equal(definitions.length, 1, 'One actual applied extension definition');
        const before = definitions[0], old = before.nativeConnection.endpoint;
        const url = new URL(installedHttpIdentity(before).url);
        assert.equal(url.hostname, '127.0.0.1', 'Only exact private loopback endpoint may be occupied');
        assert.ok(Number(url.port) > 0);
        evidence.before = snapshot(before);
        await bounded(()=>host.prepareOwnedConfigurations());
        evidence.stop_dispatches++;
        const stopped = await bounded(() => host.ui('serviceStop', host.installation, '#manage/service'));
        assert.equal(stopped.capture?.response?.record?.phase, 'complete', 'Complete actual owned service Stop before collision');
        assert.equal(stopped.capture?.runtime?.installDir, host.installation.installDir);
        assert.equal(stopped.capture?.runtime?.runtimeSha256, host.installation.runtimeSha256);
        evidence.stop_complete = true;
        await host.mark('c8-endpoint-refresh', evidence);
        occupant = net.createServer(socket => {
            evidence.occupant_connections++;
            socket.destroy(); // No request, credentials or source bytes are read or retained.
        });
        await bounded(() => new Promise<void>((resolve, reject) => {
            occupant!.once('error', reject);
            occupant!.listen({host: '127.0.0.1', port: Number(url.port), exclusive: true}, () => {
                occupant!.removeListener('error', reject); resolve();
            });
        }));
        evidence.collision_port_owned = true;
        evidence.start_dispatches++;
        await bounded(() => host.ui('startService', host.installation, '#manage/service'));
        let after: InstalledEditorC8Definition | undefined;
        for (let attempt = 0; attempt < 128; attempt++) {
            remaining();
            const current = await bounded(() => host.definitions());
            assert.ok(current.length <= 1, 'Refresh cannot widen advertised selection');
            if (current[0] && current[0].nativeConnection.endpoint.service_instance !== old.service_instance) {
                after = current[0]; break;
            }
            await new Promise(resolve => setTimeout(resolve, Math.min(250, remaining())));
        }
        assert.ok(after, 'Actual extension definition refresh after restart');
        const next = after.nativeConnection.endpoint;
        assert.notEqual(next.url, old.url, 'Occupied saved port requires a new actual URL');
        assert.ok(next.generation > old.generation, 'Endpoint generation advances');
        assert.notEqual(after.version, before.version, 'Actual provider version refresh');
        assert.equal(after.nativeConnection.connection_id, before.nativeConnection.connection_id);
        assert.equal(installedHttpIdentity(after).credentialId, installedHttpIdentity(before).credentialId);
        assert.deepEqual(scope(after.nativeConnection), scope(before.nativeConnection));
        const beforeClock=row(before.nativeConnection.selection.selection).expected_catalog_revision,afterClock=row(after.nativeConnection.selection.selection).expected_catalog_revision;
        assert.ok(Number.isSafeInteger(beforeClock)&&Number.isSafeInteger(afterClock)&&afterClock>=beforeClock,'Endpoint refresh cannot regress the native metadata clock');
        assert.deepEqual(after.ca, before.ca, 'Port relocation retains normal CA trust');
        for (const field of ['installation', 'user', 'namespace', 'ca_sha256', 'executable_sha256'] as const) {
            assert.equal(next[field], old[field], 'Relocation retains endpoint ' + field);
        }
        await bounded(() => host.verifyFresh(after!));
        await bounded(()=>host.verifyOwnedConfigurations());
        // Other editor clients may still close or reconnect captured old URLs.
        // This aggregate listener cannot attribute their TCP contacts to native
        // discovery. Retain the count; qualify the fresh authenticated endpoint.
        evidence.after = snapshot(after); evidence.fresh_authenticated_request = true; evidence.status = 'passed';
    } catch (error) {
        evidence.status = 'failed';
        evidence.failure = error instanceof ServiceError ? error.code : 'installed_endpoint_assertion_or_transport_failure';
        throw error;
    } finally {
        if (occupant?.listening) await new Promise<void>((resolve, reject) => occupant!.close(error => error ? reject(error) : resolve()));
        evidence.occupant_closed = !occupant?.listening;
        await host.mark('c8-endpoint-refresh', evidence);
    }
    return evidence;
}

function row(value: unknown): Row {
    assert.ok(value && typeof value === 'object' && !Array.isArray(value));
    return value as Row;
}

/** Call only before selected authority is deleted and while Explorer remains open. */
export async function checkInstalledEditorC8(host: InstalledEditorC8Host): Promise<Row> {
    const deadline = Math.min(host.deadline, Date.now() + 90000);
    const remaining = () => {
        const value = deadline - Date.now();
        assert.ok(value > 0, 'Installed Assistance/provider acceptance deadline');
        return value;
    };
    // A bounded outer wait does not authorize replaying an uncertain UI mutation.
    const bounded = async <T>(operation: () => Promise<T>): Promise<T> => {
        const timeout=remaining();
        let timer: ReturnType<typeof setTimeout> | undefined;
        try {
            return await Promise.race([operation(), new Promise<never>((_, reject) => {
                timer = setTimeout(() => reject(new Error('Installed C8 operation exceeded its original deadline')), timeout);
            })]);
        } finally { if (timer !== undefined) clearTimeout(timer); }
    };
    const evidence: Row = {scope: 'actual selected HTTPS Assistance and installed Management inference controls',
        start_dispatches: 0, stop_dispatches: 0, provider_reads:0, observations: []};
    const retain = () => host.mark('c8-assistance-provider', evidence);
    const observation = async (): Promise<Row> => {
        remaining();
        let value:Row;
        while(true){
            assert.ok(++evidence.provider_reads<=128,'Provider read budget exhausted');
            try{value=row(await bounded(()=>host.rpc('embedding_status','embedding_status')));break;}
            catch(error){
                if(!(error instanceof ServiceError)||error.code!=='embedding_observation_busy')throw error;
                await new Promise(resolve=>setTimeout(resolve,Math.min(250,remaining())));
            }
        }
        assert.equal(value.process_id, null, 'In-service inference must not acquire a helper process');
        assert.ok(evidence.observations.length < 128, 'Provider observation count bound');
        evidence.observations.push({worker: value.worker, available: value.available,
            process_id: value.process_id, provider_kind: value.provider?.kind ?? null});
        return value;
    };
    const waitProvider = async (ready: boolean, worker?: string): Promise<Row> => {
        for (let attempt = 0; attempt < 128; attempt++) {
            remaining();
            const value=await observation();
            if (ready && value.available === true) {
                assert.match(value.worker, /^[a-f0-9]{32}$/, 'Actual provider generation');
                assert.equal(row(value.provider).kind, 'in_process');
                return value;
            }
            if (!ready && value.worker === null) {
                assert.equal(value.available, false); assert.equal(value.provider ?? null, null);
                return value;
            }
            if (!ready) assert.equal(value.worker, worker, 'Accepted Stop cannot retire another generation');
            await new Promise(resolve => setTimeout(resolve, Math.min(250, remaining())));
        }
        throw new Error('Installed provider observation budget exhausted');
    };
    try {
        const help = row(await bounded(() => host.call('help', {query: 'repair'})));
        assert.ok(Array.isArray(help.results) && help.results.length > 0, 'Installed HTTPS Assistance help');
        const diagnostics = row(await bounded(() => host.call('diagnostics', {response_mode: 'full'})));
        assert.equal(diagnostics.schema,'airk.combined-read/1','Combined native diagnostics retains its complete canonical framing');
        const ui = row(await bounded(()=>host.uiStatus()));
        assert.equal(ui.attached, true, 'Actual installed standalone Assistance attaches Explorer');
        evidence.assistance = {help_results: help.results.length, attached: ui.attached,
            active_route: ui.active_route,ui_status_transport:'installed_standalone_assistance',combined_diagnostics_schema:diagnostics.schema};
        const before = row(row(await bounded(() => host.rpc('health', 'health'))).observation);
        assert.match(before.instance, /^[a-f0-9]{32}$/);
        evidence.service_before = {instance: before.instance, runtime_sha256: before.runtime_sha256,
            executable_sha256: before.executable_sha256};
        for (const field of ['runtime_sha256', 'executable_sha256']) assert.match(before[field], /^[a-f0-9]{64}$/);
        let provider = await observation();
        if (provider.available !== true) {
            evidence.start_dispatches++;
            evidence.start_ui = await bounded(() => host.ui('providerStart', {}, '#manage/service'));
            assert.equal(evidence.start_ui.mutation_dispatches, 1, 'One actual installed Start click');
            await retain();
            provider = await waitProvider(true);
        }
        assert.match(provider.worker, /^[a-f0-9]{32}$/);
        assert.equal(row(provider.provider).kind, 'in_process');
        evidence.stopped_worker = provider.worker;
        // Native acceptance can report unknown while the exact generation drains.
        // Never submit Stop twice; only observe that original generation below.
        evidence.stop_dispatches++;
        try {
            evidence.stop_ui = await bounded(() => host.ui('providerStop', {worker: provider.worker}, '#manage/service'));
            assert.equal(evidence.stop_ui.mutation_dispatches, 1, 'One actual installed Stop click');
            assert.equal(evidence.stop_ui.worker, provider.worker, 'Installed confirmation names exact provider');
        } catch (error) {
            if (!(error instanceof ServiceError) || error.code !== 'embedding_stop_outcome_unknown'
                || error.nativeResponse?.status !== 'failure' || error.nativeResponse.code !== error.code) throw error;
            evidence.stop_outcome = 'native_reported_unknown';
        }
        await retain();
        await waitProvider(false, provider.worker);
        const after = row(row(await bounded(() => host.rpc('health', 'health'))).observation);
        for (const field of ['instance', 'runtime_sha256', 'executable_sha256']) {
            assert.equal(after[field], before[field], 'Inference Stop preserves service ' + field);
        }
        await bounded(() => host.lexical());
        evidence.lexical_after_stop = true;
        evidence.status = 'passed';
        await retain();
        return evidence;
    } catch (error) {
        evidence.status = 'failed';
        // Retain no raw transport errors: they can contain headers or credentials.
        evidence.failure = error instanceof ServiceError ? error.code : 'installed_c8_assertion_or_transport_failure';
        await retain();
        throw error;
    }
}
