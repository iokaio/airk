// SPDX-License-Identifier: Apache-2.0
// The Python driver constructs/verifies the stage; VSCE alone creates the VSIX.
const { createVSIX } = require('@vscode/vsce');
const { execFileSync } = require('node:child_process');
const path = require('node:path');
const [stage, target, output, channel = 'preview'] = process.argv.slice(2);
if (!stage || !target || !output) { throw new Error('Expected stage, VSIX platform and output path.'); }
if (!['preview', 'stable'].includes(channel)) { throw new Error('Expected preview or stable channel.'); }
const root = path.resolve(__dirname, '../../..');
execFileSync(process.platform === 'win32' ? 'python' : 'python3',
    [path.join(root, 'scripts/check-storage-engine.py')], { cwd: root, stdio: 'inherit' });
// The stage contains only explicitly allowlisted runtime dependencies. VSCE must
// enumerate them or it will omit node_modules even though the files are staged.
createVSIX({ cwd: stage, target, packagePath: output, dependencies: true, useYarn: false,
    followSymlinks: false, rewriteRelativeLinks: false, preRelease: channel === 'preview' })
    .catch(error => { console.error(error); process.exitCode = 1; });
