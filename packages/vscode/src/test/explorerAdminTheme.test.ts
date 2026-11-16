// SPDX-License-Identifier: Apache-2.0
// The embedded management panel (Service, Activity, Storage, ...) takes its
// colours and fonts from the VS Code theme, never the browser Admin's own palette.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import test from 'node:test';

const { embeddedAdminCss } = require(path.resolve(__dirname, '../../scripts/build-explorer.cjs')) as { embeddedAdminCss(source: string): string };
const adminCss = fs.readFileSync(path.resolve(__dirname, '../../../admin/src/app.css'), 'utf8');

test('the embedded Admin CSS keeps no fixed page palette', () => {
    const css = embeddedAdminCss(adminCss);
    assert.doesNotMatch(css, /#[0-9a-fA-F]{3,8}\b/);
    assert.doesNotMatch(css, /prefers-color-scheme/);
    assert.doesNotMatch(css, /(^|[\s,{]):root\b/m);
    assert.doesNotMatch(css, /(^|[\s,}])body\b/m);
});

test('every Admin token the stylesheet uses is mapped to a VS Code theme variable', () => {
    const css = embeddedAdminCss(adminCss);
    const used = new Set([...adminCss.matchAll(/var\((--airk-[a-z-]+)/g)].map(match => match[1]));
    assert.ok(used.size > 0);
    const block = css.slice(0, css.indexOf('}') + 1);
    for (const token of used) {
        assert.match(block, new RegExp(`${token}: var\\(--vscode-`), `${token} must come from the VS Code theme`);
    }
    // The panel's own text and font follow the editor.
    assert.match(block, /color: var\(--airk-text\)/);
    assert.match(block, /font: inherit/);
});

test('a reintroduced fixed palette fails the build', () => {
    assert.throws(() => embeddedAdminCss(`${adminCss}\n.extra { color: #ffffff; }`), /page palette/);
    assert.throws(() => embeddedAdminCss(`${adminCss}\n@media (prefers-color-scheme: light) { .x { color: red; } }`), /page palette/);
});
