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
const source = 'https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2';
const revision = '10dbd2f06a8baf40ad285b037edda610c1b9a57c';
export const modelAssets: readonly ModelAsset[] = [
    { name: 'model.onnx', url: `${source}/resolve/${revision}/onnx/model.onnx`, bytes: 90405214, sha256: '6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452' },
    { name: 'tokenizer.json', url: `${source}/resolve/${revision}/tokenizer.json`, bytes: 466247, sha256: 'be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037' },
    { name: 'LICENSE', url: 'https://www.apache.org/licenses/LICENSE-2.0.txt', bytes: 11358, sha256: 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30' }
];
export const modelManifest = {
    schema: 2, name: 'sentence-transformers/all-MiniLM-L6-v2', license: 'Apache-2.0', source, revision,
    model_sha256: modelAssets[0].sha256, tokenizer_sha256: modelAssets[1].sha256, license_sha256: modelAssets[2].sha256,
    dimensions: 384, max_tokens: 256, pooling: 'attention-mask-mean-l2', input_ids: 'input_ids',
    attention_mask: 'attention_mask', token_type_ids: 'token_type_ids', output: 'last_hidden_state'
};

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
export function prepareVectorModel(dataDir: string, progress: (message: string) => void, install: (source: string) => Promise<void>): Promise<void> {
    const key = path.resolve(dataDir);
    const previous = pending.get(key);
    if (previous) { return previous; }
    const job = (async () => {
        if (await existingModel(path.join(key, 'models', 'local'))) { return; }
        const scratch = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-vector-download-'));
        const source = path.join(scratch, 'models', 'local');
        try {
            await installModelAssets(scratch, modelAssets, modelManifest, download, progress);
            progress('Installing verified model files');
            await install(source);
        } catch (error) {
            throw new Error(`Vector model setup failed: ${error instanceof Error ? error.message : String(error)}. Retry Index now, or turn Vector search off in the panel.`);
        } finally {
            for (const name of [...modelAssets.map(asset => asset.name), 'model.json']) {
                await fs.unlink(path.join(source, name)).catch(error => { if (error.code !== 'ENOENT') { throw error; } });
            }
            for (const directory of [source, path.dirname(source), scratch]) {
                await fs.rmdir(directory).catch(error => { if (error.code !== 'ENOENT') { throw error; } });
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
            progress(`Preparing local vector model: ${asset.name} (about 91 MB total)`);
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
