// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';

export interface VectorStatus {
    enabled: boolean; published: boolean; indexed_vectors: number; pending_reindex: boolean;
    default_search_mode: 'hybrid' | 'lexical' | 'requires_complete';
    semantic_readiness?: {
        state: 'disabled' | 'pending' | 'partial' | 'ready' | 'failed';
        selected_artifacts: number; completed_artifacts: number;
        selected_units: number | null; completed_units: number;
        failure: string | null;
    };
    discovery_readiness?: {
        state: 'pending' | 'partial' | 'ready' | 'failed';
        processed_artifacts: number; committed_artifacts: number;
        total_artifacts: number | null; failure: string | null;
    };
}
export interface ModelAsset { name: string; url: string; bytes: number; sha256: string }
interface CatalogProfile { id: string; precision: 'fp32'; assets: ModelAsset[]; manifest: Record<string, unknown> }

/** Fail before downloading if the verified helper's catalog protocol is incompatible. */
export function modelProfileFromCatalog(value: unknown): CatalogProfile {
    const object = (item: unknown): Record<string, unknown> => item !== null && typeof item === 'object' && !Array.isArray(item) ? item as Record<string, unknown> : {};
    const report = object(value), catalog = object(report.catalog), selection = object(report.selection);
    const profiles = catalog.profiles;
    if (report.schema !== 1 || catalog.schema !== 1 || report.automatic_int8 !== false
        || !Array.isArray(profiles) || profiles.length !== 1 || typeof selection.profile_id !== 'string'
        || selection.profile_id !== catalog.default_profile) { throw new Error('AIRK helper returned an unsupported model catalog.'); }
    const profile = object(profiles[0]), manifest = object(profile.manifest);
    if (profile.id !== selection.profile_id || profile.precision !== 'fp32'
        || typeof profile.runtime !== 'string' || profile.runtime !== object(report.capabilities).runtime
        || manifest.schema !== 2 || manifest.dimensions !== 384 || manifest.max_tokens !== 256
        || manifest.source !== 'https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2'
        || typeof manifest.revision !== 'string' || !/^[0-9a-f]{40}$/.test(manifest.revision)
        || !Array.isArray(profile.assets) || profile.assets.length !== 3) { throw new Error('AIRK helper selected an unsupported model profile.'); }
    const names = new Set<string>();
    const assets = profile.assets.map(item => {
        const asset = object(item), name = asset.name;
        const field = name === 'model.onnx' ? 'model_sha256' : name === 'tokenizer.json' ? 'tokenizer_sha256' : name === 'LICENSE' ? 'license_sha256' : undefined;
        const maximum = name === 'model.onnx' ? 268435456 : name === 'tokenizer.json' ? 4194304 : 262144;
        const url = name === 'LICENSE' ? 'https://www.apache.org/licenses/LICENSE-2.0.txt'
            : `${manifest.source}/resolve/${manifest.revision}/${name === 'model.onnx' ? 'onnx/model.onnx' : 'tokenizer.json'}`;
        if (typeof name !== 'string' || !field || names.has(name) || !Number.isSafeInteger(asset.bytes)
            || Number(asset.bytes) <= 0 || Number(asset.bytes) > maximum || asset.url !== url
            || typeof asset.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(asset.sha256)
            || asset.sha256 !== manifest[field]) { throw new Error('AIRK helper returned an invalid model asset binding.'); }
        names.add(name);
        return asset as unknown as ModelAsset;
    });
    return { id: String(profile.id), precision: 'fp32', assets, manifest };
}

/** A bounded stream keeps downloads out of memory; hashes cover the exact file bytes. */
export async function writeModelAsset(file: string, asset: ModelAsset, chunks: AsyncIterable<Uint8Array>): Promise<void> {
    const output = await fs.open(file, 'wx', 0o600);
    const digest = createHash('sha256');
    let count = 0;
    try {
        for await (const chunk of chunks) {
            count += chunk.byteLength;
            if (count > asset.bytes) { throw new Error(`Model download exceeds pinned size: ${asset.name}`); }
            digest.update(chunk);
            await output.writeFile(chunk);
        }
        if (count !== asset.bytes || digest.digest('hex') !== asset.sha256) { throw new Error(`Model download size or digest mismatch: ${asset.name}`); }
        await output.sync();
    } finally { await output.close(); }
}

async function existingModel(directory: string): Promise<boolean> {
    try {
        const stat = await fs.lstat(directory);
        if (!stat.isDirectory() || stat.isSymbolicLink()) { throw new Error('Local model path must be a regular directory.'); }
        const manifest = await fs.lstat(path.join(directory, 'model.json'));
        if (!manifest.isFile() || manifest.isSymbolicLink() || manifest.size > 16384) { throw new Error('Invalid existing model manifest.'); }
        // The native helper validates the full model. Never replace user-provisioned assets.
        return true;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; }
        try { await fs.lstat(directory); } catch (missing) {
            if ((missing as NodeJS.ErrnoException).code === 'ENOENT') { return false; }
            throw missing;
        }
        throw new Error('Existing local model is incomplete. Repair it or move it before retrying.');
    }
}

async function* download(url: string): AsyncGenerator<Uint8Array> {
    let current = url;
    const signal = AbortSignal.timeout(120000);
    for (let redirects = 0; redirects <= 5; redirects++) {
        if (new URL(current).protocol !== 'https:') { throw new Error('Model downloads require HTTPS.'); }
        const response = await fetch(current, { redirect: 'manual', signal });
        if ([301, 302, 303, 307, 308].includes(response.status)) {
            const location = response.headers.get('location');
            await response.body?.cancel();
            if (!location) { throw new Error('Model download redirect has no location.'); }
            current = new URL(location, current).href;
            continue;
        }
        if (!response.ok || !response.body) { await response.body?.cancel(); throw new Error(`Model download failed (${response.status}). Retry Index now when online, or turn Vector search off in the panel.`); }
        yield* response.body;
        return;
    }
    throw new Error('Too many model download redirects.');
}

const pending = new Map<string, Promise<void>>();
/** Trusted extension setup only. MCP/native reads never download or run inference remotely. */
export function prepareVectorModel(dataDir: string, progress: (message: string) => void, install: (source: string) => Promise<void>,
    select: () => Promise<unknown>): Promise<void> {
    const key = path.resolve(dataDir);
    const previous = pending.get(key);
    if (previous) { return previous; }
    const job = (async () => {
        let scratch: string | undefined, source: string | undefined;
        let assets: ModelAsset[] = [];
        try {
            if (await existingModel(path.join(key, 'models', 'local'))) { return; }
            const profile = modelProfileFromCatalog(await select());
            assets = profile.assets;
            progress(`Selected local model: ${profile.manifest.name} (${profile.precision.toUpperCase()})`);
            scratch = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-vector-download-'));
            source = path.join(scratch, 'models', 'local');
            await installModelAssets(scratch, profile.assets, profile.manifest, download, progress);
            progress('Installing verified model files');
            await install(source);
        } catch (error) {
            throw new Error(`Vector model setup failed: ${error instanceof Error ? error.message : String(error)}. Retry Index now, or turn Vector search off in the panel.`);
        } finally {
            if (source && scratch) {
                for (const name of [...assets.map(asset => asset.name), 'model.json']) {
                    await fs.unlink(path.join(source, name)).catch(error => { if (error.code !== 'ENOENT') { throw error; } });
                }
                for (const directory of [source, path.dirname(source), scratch]) {
                    await fs.rmdir(directory).catch(error => { if (error.code !== 'ENOENT') { throw error; } });
                }
            }
        }
    })().finally(() => { pending.delete(key); });
    pending.set(key, job);
    return job;
}

export async function installModelAssets(dataDir: string, assets: readonly ModelAsset[], manifest: object, fetchAsset: (url: string) => AsyncIterable<Uint8Array>, progress: (message: string) => void): Promise<void> {
    if (assets.some(asset => !['model.onnx', 'tokenizer.json', 'LICENSE'].includes(asset.name))) { throw new Error('Invalid model asset name.'); }
    const parent = path.join(dataDir, 'models'), destination = path.join(parent, 'local');
    // Match native model validation: redirected ancestors must not receive assets.
    for (let current = destination; ; current = path.dirname(current)) {
        try { if ((await fs.lstat(current)).isSymbolicLink()) { throw new Error('Model storage cannot use symbolic links.'); } }
        catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
        if (path.dirname(current) === current) { break; }
    }
    if (await existingModel(destination)) { return; }
    await fs.mkdir(parent, { recursive: true });
    const staging = await fs.mkdtemp(path.join(parent, '.minilm-'));
    try {
        for (const asset of assets) {
            const megabytes = Math.ceil(assets.reduce((total, item) => total + item.bytes, 0) / 1000000);
            progress(`Preparing local vector model: ${asset.name} (about ${megabytes} MB total)`);
            await writeModelAsset(path.join(staging, asset.name), asset, fetchAsset(asset.url));
        }
        await fs.writeFile(path.join(staging, 'model.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
        // Atomic directory publication: an interrupted download never appears ready.
        try { await fs.rename(staging, destination); }
        catch (error) { if (!(await existingModel(destination))) { throw error; } }
    } finally {
        // Only our unique staging directory, never existing models or collection state.
        for (const name of [...assets.map(asset => asset.name), 'model.json']) { await fs.unlink(path.join(staging, name)).catch(error => { if (error.code !== 'ENOENT') { throw error; } }); }
        await fs.rmdir(staging).catch(error => { if (error.code !== 'ENOENT') { throw error; } });
    }
}
