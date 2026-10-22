// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';

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

/** Fail before installing if the verified helper's catalog protocol is incompatible. */
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

const pending = new Map<string, Promise<void>>();
/** Trusted setup installs the helper's embedded assets. It never uses the network. */
export function prepareVectorModel(dataDir: string, progress: (message: string) => void, install: () => Promise<void>,
    select: () => Promise<unknown>): Promise<void> {
    const key = path.resolve(dataDir);
    const previous = pending.get(key);
    if (previous) { return previous; }
    const job = (async () => {
        try {
            if (await existingModel(path.join(key, 'models', 'local'))) { return; }
            const report = await select();
            const profile = modelProfileFromCatalog(report);
            const bundled = (report as { bundled_model?: { manifest?: unknown } }).bundled_model?.manifest;
            // Compare values independently of JSON property order.
            const expected = profile.manifest;
            if (!bundled || typeof bundled !== 'object' || Array.isArray(bundled)
                || Object.keys(bundled).length !== Object.keys(expected).length
                || Object.entries(expected).some(([name, value]) => (bundled as Record<string, unknown>)[name] !== value)) {
                throw new Error('Embedded model manifest differs from the helper catalog. Reinstall the AIRK extension.');
            }
            progress(`Installing bundled local model: ${profile.manifest.name} (${profile.precision.toUpperCase()})`);
            await install();
        } catch (error) {
            throw new Error(`Vector model setup failed: ${error instanceof Error ? error.message : String(error)}. Retry Index now, or turn Vector search off in the panel.`);
        }
    })().finally(() => { pending.delete(key); });
    pending.set(key, job);
    return job;
}
