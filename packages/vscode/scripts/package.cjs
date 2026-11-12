// SPDX-License-Identifier: Apache-2.0
// The Python driver constructs/verifies the stage; VSCE alone creates the universal,
// UI-only VSIX (split plan U29). No target platform is ever passed.
const { createVSIX } = require('@vscode/vsce');
const [stage, output, channel = 'stable', ...extra] = process.argv.slice(2);
if (!stage || !output || extra.length) { throw new Error('Expected stage, output path and optional channel.'); }
if (!['preview', 'stable'].includes(channel)) { throw new Error('Expected preview or stable channel.'); }
// The stage contains only explicitly allowlisted runtime dependencies. VSCE must
// enumerate them or it will omit node_modules even though the files are staged.
createVSIX({ cwd: stage, packagePath: output, dependencies: true, useYarn: false,
    followSymlinks: false, rewriteRelativeLinks: false, preRelease: channel === 'preview' })
    .catch(error => { console.error(error); process.exitCode = 1; });
