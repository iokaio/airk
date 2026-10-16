// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';

// Test the pinned build tool itself: 4.0.0 sent an obsolete exchange request.
const { getOIDCCredential } = require('@vscode/vsce/out/oidc');
const environment = {
    GITHUB_ACTIONS: 'true',
    ACTIONS_ID_TOKEN_REQUEST_URL: 'https://pipelines.actions.githubusercontent.com/fixture?api-version=2',
    ACTIONS_ID_TOKEN_REQUEST_TOKEN: 'fixture-request-token'
};
type Request = { method: string; headers: Record<string, string>; body?: string };
const response = (body: object, statusCode = 200) => ({
    statusCode, statusMessage: statusCode === 200 ? 'OK' : 'Forbidden',
    readBody: async () => JSON.stringify(body)
});

test('pinned VSCE uses the final Marketplace OIDC exchange contract', async () => {
    const calls: { url: string; request: Request }[] = [];
    const credential = await getOIDCCredential('iokaio', {
        environment,
        request: async (url: string, request: Request) => {
            calls.push({ url, request });
            return calls.length === 1
                ? response({ value: 'fixture-workflow-identity' })
                : response({ credential: 'fixture-short-lived-session' });
        }
    });
    assert.equal(credential, 'fixture-short-lived-session');
    assert.equal(calls.length, 2);
    const issuer = new URL(calls[0].url);
    assert.equal(issuer.searchParams.get('audience'), 'marketplace.visualstudio.com');
    assert.equal(issuer.searchParams.get('api-version'), '2');
    assert.equal(calls[0].request.method, 'GET');
    assert.equal(calls[0].request.headers.Authorization, 'Bearer fixture-request-token');
    assert.equal(calls[1].url, 'https://marketplace.visualstudio.com/_apis/gallery/token?api-version=7.2-preview.1');
    assert.equal(calls[1].request.method, 'POST');
    assert.equal(calls[1].request.headers.Authorization, 'FederatedToken fixture-workflow-identity');
    assert.deepEqual(JSON.parse(calls[1].request.body!), { publisherName: 'iokaio' });
});

test('pinned VSCE reports a rejected Marketplace identity without returning a credential', async () => {
    let calls = 0;
    await assert.rejects(getOIDCCredential('iokaio', {
        environment,
        request: async () => ++calls === 1
            ? response({ value: 'fixture-workflow-identity' })
            : response({ message: 'Fixture trust policy rejects this identity' }, 403)
    }), /Marketplace OIDC token exchange failed with 403 Forbidden/);
    assert.equal(calls, 2);
});
