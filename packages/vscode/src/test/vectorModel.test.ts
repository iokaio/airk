// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { mock, test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { installModelAssets, ModelAsset, modelProfileFromCatalog, prepareVectorModel, writeModelAsset } from '../vectorModel';

const bytes = Buffer.from('original model fixture');
const asset: ModelAsset = { name: 'model.onnx', url: 'https://example.invalid/fixture', bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
async function* stream(value = bytes) { yield value.subarray(0, 5); yield value.subarray(5); }

async function catalogReport() {
    const catalog = JSON.parse(await fs.readFile(path.resolve(__dirname, '../../../../crates/airk-retrieval/src/model_catalog.json'), 'utf8'));
    return { schema: 1, automatic_int8: false, catalog,
        capabilities: { runtime: catalog.profiles[0].runtime }, selection: { profile_id: catalog.default_profile } };
}

test('native catalog selection binds download URLs, hashes, precision and asset inventory', async () => {
    const report = await catalogReport();
    const profile = modelProfileFromCatalog(report);
    assert.equal(profile.precision, 'fp32');
    assert.equal(profile.manifest.model_sha256, '6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452');
    const mutations = [
        (value: any) => { value.selection.profile_id = 'unapproved'; },
        (value: any) => { value.catalog.profiles[0].precision = 'int8'; },
        (value: any) => { value.catalog.profiles[0].assets[0].sha256 = '0'.repeat(64); },
        (value: any) => { value.catalog.profiles[0].assets[0].url = 'https://example.invalid/unpinned'; },
        (value: any) => { value.catalog.profiles[0].assets[0].bytes = 268435457; },
        (value: any) => { value.catalog.profiles[0].assets[1] = value.catalog.profiles[0].assets[0]; },
        (value: any) => { value.automatic_int8 = true; }
    ];
    for (const mutate of mutations) {
        const invalid = structuredClone(report);
        mutate(invalid);
        assert.throws(() => modelProfileFromCatalog(invalid), /unsupported|invalid/);
    }
});

test('model setup preserves existing custom assets without capability selection or download', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-catalog-existing-'));
    const destination = path.join(directory, 'models', 'local');
    try {
        await fs.mkdir(destination, { recursive: true });
        const manifest = '{"original":"custom model fixture"}\n';
        await fs.writeFile(path.join(destination, 'model.json'), manifest);
        await prepareVectorModel(directory, () => { assert.fail('must not change an existing model'); },
            async () => { assert.fail('must not install'); }, async () => { assert.fail('must not reselect'); });
        assert.equal(await fs.readFile(path.join(destination, 'model.json'), 'utf8'), manifest);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('catalog selection failures include retry guidance and permit a subsequent retry', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-catalog-retry-'));
    let selections = 0;
    try {
        for (let attempt = 0; attempt < 2; attempt++) {
            await assert.rejects(prepareVectorModel(directory, () => assert.fail('must not download'),
                async () => assert.fail('must not install'), async () => { selections++; throw new Error('catalog unavailable'); }),
            /Vector model setup failed: catalog unavailable.*Retry Index now, or turn Vector search off/);
        }
        assert.equal(selections, 2);
        assert.deepEqual(await fs.readdir(directory), []);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('new setup downloads the selected catalog assets once and cleans its temporary source', async t => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-catalog-setup-'));
    const report = await catalogReport(), profile = report.catalog.profiles[0];
    const contents = new Map<string, Buffer>();
    for (const entry of profile.assets) {
        const content = Buffer.from(`original selected fixture ${entry.name}`);
        entry.bytes = content.length;
        entry.sha256 = createHash('sha256').update(content).digest('hex');
        const field = entry.name === 'model.onnx' ? 'model_sha256' : entry.name === 'tokenizer.json' ? 'tokenizer_sha256' : 'license_sha256';
        profile.manifest[field] = entry.sha256;
        contents.set(entry.url, content);
    }
    let selections = 0, installations = 0, downloads = 0, temporarySource = '';
    const fetch = mock.method(globalThis, 'fetch', async (input: string | URL | Request) => {
        const content = contents.get(String(input));
        assert.ok(content, 'only selected pinned URLs may be downloaded');
        assert.equal(selections, 1);
        downloads++;
        return new Response(new Uint8Array(content));
    });
    t.after(() => fetch.mock.restore());
    try {
        const select = async () => { selections++; return report; };
        const install = async (source: string) => {
            installations++;
            temporarySource = source;
            assert.deepEqual(JSON.parse(await fs.readFile(path.join(source, 'model.json'), 'utf8')), profile.manifest);
            for (const entry of profile.assets) {
                assert.deepEqual(await fs.readFile(path.join(source, entry.name)), contents.get(entry.url));
            }
        };
        await Promise.all([prepareVectorModel(directory, () => {}, install, select), prepareVectorModel(directory, () => {}, install, select)]);
        assert.equal(selections, 1);
        assert.equal(installations, 1);
        assert.equal(downloads, 3);
        await assert.rejects(fs.stat(temporarySource), /ENOENT/);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('model download enforces exact size and digest, including streamed overflow', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-vector-test-'));
    try {
        const target = path.join(directory, 'model.onnx');
        await writeModelAsset(target, asset, stream());
        assert.deepEqual(await fs.readFile(target), bytes);
        await assert.rejects(writeModelAsset(target, asset, stream()), /EEXIST/);
        for (const [name, value] of [['overflow', Buffer.concat([bytes, bytes])], ['short', bytes.subarray(0, 4)], ['wrong', Buffer.alloc(bytes.length)]] as const) {
            await assert.rejects(writeModelAsset(path.join(directory, name), asset, stream(value)), /pinned size|digest mismatch/);
        }
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('model setup publishes complete assets atomically, cleans failures and preserves custom models', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-vector-test-'));
    const destination = path.join(directory, 'models', 'local');
    let calls = 0;
    try {
        await assert.rejects(installModelAssets(directory, [asset], {}, async function* () {
            await assert.rejects(fs.stat(destination), /ENOENT/);
            throw new Error('offline fixture');
        }, () => {}), /offline fixture/);
        assert.deepEqual(await fs.readdir(path.dirname(destination)), []);
        await installModelAssets(directory, [asset], { fixture: true }, async function* () { calls++; yield* stream(); }, () => {});
        assert.equal(calls, 1);
        assert.deepEqual(await fs.readFile(path.join(destination, 'model.onnx')), bytes);
        assert.deepEqual(JSON.parse(await fs.readFile(path.join(destination, 'model.json'), 'utf8')), { fixture: true });
        await installModelAssets(directory, [asset], {}, async function* () { throw new Error('must not download'); }, () => {});
        assert.deepEqual(JSON.parse(await fs.readFile(path.join(destination, 'model.json'), 'utf8')), { fixture: true });
        await fs.unlink(path.join(destination, 'model.json'));
        await assert.rejects(installModelAssets(directory, [asset], {}, () => stream(), () => {}), /incomplete/);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});
