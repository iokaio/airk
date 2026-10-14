// SPDX-License-Identifier: Apache-2.0
// Isolated profile/fixture only; never reuse the user's normal editor state.
const { spawn } = require('node:child_process');
const path = require('node:path');
const [editor, extension, profile, extensions, workspace] = process.argv.slice(2).map(value => path.resolve(value));
if (!workspace) { throw new Error('Expected editor, verified extension, isolated profile/extensions, and fixture workspace.'); }
const run = path.dirname(workspace);
const artifactRoot = path.resolve(__dirname, '../../../artifacts/local');
const relative = path.relative(artifactRoot, run);
if (!relative || relative.startsWith('..') || path.isAbsolute(relative) || path.basename(workspace) !== 'review-workspace'
    || [profile, extensions, extension].some(value => path.relative(run, value).startsWith('..') || path.isAbsolute(path.relative(run, value)))) {
    throw new Error('Editor tests require a generated artifact directory with private profile, extensions and fixture paths.');
}
const env = { ...process.env, AIRK_EDITOR_FIXTURE: workspace };
delete env.ELECTRON_RUN_AS_NODE;
const child = spawn(editor, [workspace, `--extensionDevelopmentPath=${extension}`,
    `--extensionTestsPath=${path.resolve(__dirname, '../out/test/editor.js')}`,
    '--user-data-dir', profile, '--extensions-dir', extensions,
    '--disable-workspace-trust', '--skip-welcome', '--skip-release-notes', '--disable-updates',
    '--disable-telemetry', '--disable-gpu', '--disable-extensions'],
    { cwd: path.dirname(workspace), windowsHide: true, stdio: 'inherit', env });
const timer = setTimeout(() => { child.kill(); process.exitCode = 1; }, 120000);
child.on('error', error => { clearTimeout(timer); console.error(error); process.exitCode = 1; });
child.on('exit', code => { clearTimeout(timer); process.exitCode = code === 0 ? 0 : 1; });
