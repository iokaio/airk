// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import type { HelpCatalog } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';
import { ROUTE_KEYS } from '../explorerRoutes';
import { ABOUT_LINKS } from '../explorerContracts';
import { HelpIndex } from '../explorerAssistance/help';

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

test('offline help includes every native engine topic, four role journeys, indexing/format guidance and public About links', () => {
    const catalog = build.helpCatalog(), index = new HelpIndex(catalog);
    for (const id of build.engineTopics()) {
        const topic = catalog.topics.find(topic => topic.id === `engine.${id}`); assert.ok(topic, `offline engine guide ${id}`);
        assert.equal(topic.source, 'crates/airk-mcp/src/help/topics.rs'); assert.equal(topic.applicability, 'implemented'); assert.ok(topic.body.length > 40);
    }
    for (const role of ['developer', 'tester', 'architect', 'writer']) {
        const topic = index.topic(`role-${role}`)!; assert.ok(topic); assert.match(topic.body, /## Steps/); assert.ok(topic.relatedAirkHelp.includes('indexing-choices'));
        assert.ok(index.search(role).some(entry => entry.topic.id === topic.id));
    }
    const choices = index.topic('indexing-choices')!.body;
    for (const label of ['essential', 'agent_lean', 'balanced', 'detailed', 'Vectors', 'configured', 'publication']) { assert.ok(choices.toLowerCase().includes(label.toLowerCase()), label); }
    const formats = index.topic('document-types')!.body;
    for (const format of ['Markdown', 'PDF', 'DOCX', 'XLSX', 'PPTX', 'OCR', 'cached', 'recalculate']) { assert.ok(formats.includes(format), format); }
    const about = index.topic('about-airk')!.body;
    for (const key of ['website', 'repository', 'discord', 'issues', 'contribute'] as const) { assert.ok(about.includes(ABOUT_LINKS[key].url)); }
    assert.match(about, /Ioka LLC/); assert.match(about, /Munarium/); assert.match(about, /independently implemented/);
    assert.match(index.topic('explorer.help')!.body, /Next page/);
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
