// SPDX-License-Identifier: Apache-2.0
// U59: the Marketplace README opens with "Requires AIRK Service" and carries the
// reviewed download links; the Download action uses the same link. The
// expectations below are reviewed literals, not read from the code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { DOWNLOAD_URL, RELEASES_URL } from '../airkctl';

const README = fs.readFileSync(path.resolve(__dirname, '..', '..', 'README.md'), 'utf8');
const REVIEWED = { heading: '## Requires AIRK Service', download: 'https://airk.ioka.io', releases: 'https://github.com/iokaio/airk/releases' };

test('the README opens with the Requires AIRK Service section', () => {
    const lines = README.split(/\r?\n/).filter(line => line.trim());
    assert.equal(lines[0], '# AIRK: Local Search for Coding Agents');
    assert.equal(lines[1], REVIEWED.heading, 'the first section after the title');
    const section = README.slice(README.indexOf(REVIEWED.heading), README.indexOf('\n## ', README.indexOf(REVIEWED.heading) + 1));
    assert.ok(section.includes(REVIEWED.download));
    assert.ok(section.includes(`(${REVIEWED.releases})`));
    for (const platform of ['**Windows:**', '**macOS:**', '**Linux:**']) { assert.ok(section.includes(platform), platform); }
    assert.ok(section.includes('airkctl service status'));
    assert.match(section, /never installs, updates, repairs or\s+removes AIRK Service/);
});

test('the Download action and the not-installed state use the reviewed links', () => {
    assert.equal(DOWNLOAD_URL, REVIEWED.download);
    assert.equal(RELEASES_URL, REVIEWED.releases);
});
