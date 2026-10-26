// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import type { HelpCatalog } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';
import { ROUTE_KEYS } from '../explorerRoutes';

// out/test → ../../scripts/build-explorer.cjs (the build step exports its validator).
// eslint-disable-next-line @typescript-eslint/no-require-imports
const build = require(path.resolve(__dirname, '..', '..', 'scripts', 'build-explorer.cjs')) as { helpCatalog(directory?: string): HelpCatalog; engineTopics(): Set<string> };

test('every route has exactly one help topic and applicability follows the feature manifest', () => {
    const catalog = build.helpCatalog();
    assert.equal(catalog.catalogVersion, 1);
    const byRoute = new Map<string, number>();
    for (const topic of catalog.topics) {
        if (topic.route) { byRoute.set(topic.route, (byRoute.get(topic.route) ?? 0) + 1); assert.equal(topic.applicability, FEATURES[topic.route]); }
        for (const id of topic.relatedAirkHelp) { assert.ok(build.engineTopics().has(id), id); }
        assert.doesNotMatch(topic.body, /\]\(|<[a-z]/i);
    }
    for (const key of ROUTE_KEYS) { assert.equal(byRoute.get(key), 1, `help topic for ${key}`); }
});

test('the catalog build rejects unknown routes, unknown engine topics, links and stale planned text', () => {
    const cases: [string, RegExp][] = [
        ['---\nid: bad.route\ntitle: T\nroute: nowhere/x\narea: explorer\naliases: a\nrelated_airk_help: search\n---\nBody', /not in the route manifest/],
        ['---\nid: bad.engine\ntitle: T\nroute: search/query\narea: explorer\naliases: a\nrelated_airk_help: not-a-topic\n---\nBody', /unknown airk_help topic/],
        ['---\nid: bad.link\ntitle: T\nroute: search/query\narea: explorer\naliases: a\nrelated_airk_help: search\n---\nSee [x](../y.md)', /without links/],
        ['---\nid: bad.planned\ntitle: T\nroute: search/query\narea: explorer\naliases: a\nrelated_airk_help: search\n---\nUntil this screen ships, wait.', /replace the planned text/],
        ['---\nid: bad.key\ntitle: T\nroute: search/query\narea: explorer\naliases: a\nrelated_airk_help: search\nextra: x\n---\nBody', /invalid front matter/],
    ];
    for (const [text, message] of cases) {
        const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-help-'));
        try {
            fs.writeFileSync(path.join(directory, 'topic.md'), text);
            assert.throws(() => build.helpCatalog(directory), message);
        } finally { fs.rmSync(directory, { recursive: true, force: true }); }
    }
});
