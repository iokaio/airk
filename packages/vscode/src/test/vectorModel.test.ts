// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { modelProfileFromCatalog, prepareVectorModel } from '../vectorModel';

async function catalogReport() {
    const catalog = JSON.parse(await fs.readFile(path.resolve(__dirname, '../../../../crates/airk-retrieval/src/model_catalog.json'), 'utf8'));
    return { schema: 1, automatic_int8: true, catalog,
        bundled_model: { manifest: structuredClone(catalog.profiles[0].manifest) },
        capabilities: { runtime: catalog.profiles[0].runtime }, selection: { profile_id: catalog.default_profile } };
}

test('native catalog selection binds hashes, precision and inventory', async () => {
    const report = await catalogReport();
    assert.equal(modelProfileFromCatalog(report).precision, 'uint8');
    const signed = structuredClone(report);
    signed.selection.profile_id = signed.catalog.profiles[1].id;
    assert.equal(modelProfileFromCatalog(signed).precision, 'int8');
    for (const mutate of [
        (v: any) => { v.selection.profile_id = 'unapproved'; },
        (v: any) => { v.catalog.profiles[0].precision = 'int8'; },
        (v: any) => { v.catalog.profiles[0].assets[0].sha256 = '0'.repeat(64); },
        (v: any) => { v.catalog.profiles[0].assets[0].url = 'https://example.invalid/unpinned'; },
        (v: any) => { v.catalog.profiles[0].assets[0].bytes = 268435457; },
        (v: any) => { v.catalog.profiles[0].assets[1] = v.catalog.profiles[0].assets[0]; },
        (v: any) => { v.automatic_int8 = false; },
        (v: any) => { v.catalog.profiles[1].precision = 'fp32'; },
        (v: any) => { v.catalog.profiles.push(v.catalog.profiles[0]); }
    ]) {
        const invalid = structuredClone(report); mutate(invalid);
        assert.throws(() => modelProfileFromCatalog(invalid), /unsupported|invalid/);
    }
});

test('setup preserves custom models and rejects incomplete models without installing', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-model-existing-'));
    const destination = path.join(directory, 'models', 'local');
    try {
        await fs.mkdir(destination, { recursive: true });
        const manifest = '{"original":"custom model fixture"}';
        await fs.writeFile(path.join(destination, 'model.json'), manifest);
        const install = async () => assert.fail('must preserve custom model');
        const select = async () => assert.fail('must not reselect');
        await prepareVectorModel(directory, () => assert.fail('must not change'), install, select);
        assert.equal(await fs.readFile(path.join(destination, 'model.json'), 'utf8'), manifest);
        await fs.unlink(path.join(destination, 'model.json'));
        await assert.rejects(prepareVectorModel(directory, () => {}, install, select), /incomplete/);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('fresh setup is offline and joins simultaneous requests for the same data root', async t => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-model-offline-'));
    t.mock.method(globalThis, 'fetch', () => { assert.fail('runtime must never download'); });
    let selections = 0, installations = 0;
    try {
        const progress: string[] = [];
        const install = async () => { installations++; };
        const select = async () => { selections++; return catalogReport(); };
        await Promise.all([prepareVectorModel(directory, v => progress.push(v), install, select),
            prepareVectorModel(directory, v => progress.push(v), install, select)]);
        assert.equal(selections, 1); assert.equal(installations, 1);
        assert.match(progress[0], /Installing bundled local model/);
        assert.deepEqual(await fs.readdir(directory), []);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('missing or mismatched bundled manifests fail before installation; failures permit retry', async t => {
    t.mock.method(globalThis, 'fetch', () => { assert.fail('setup failures must never fall back to downloading'); });
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-model-retry-'));
    try {
        const report = await catalogReport();
        const missing: any = structuredClone(report); delete missing.bundled_model;
        const wrong = structuredClone(report); wrong.bundled_model.manifest.revision = '0'.repeat(40);
        for (const invalid of [missing, wrong]) {
            await assert.rejects(prepareVectorModel(directory, () => assert.fail('must not install'),
                async () => assert.fail('must not install'), async () => invalid), /manifest differs.*Retry Index now/);
        }
        let attempts = 0;
        for (let i = 0; i < 2; i++) {
            await assert.rejects(prepareVectorModel(directory, () => {}, async () => { attempts++; throw new Error('quota fixture'); },
                catalogReport), /quota fixture.*Retry Index now/);
        }
        assert.equal(attempts, 2);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});
