# Actual Explorer hot-update UI gate

Run this opt-in gate in a graphical Code session with two independently packaged,
sealed AIRK VSIXes and their reviewed SHA256 values:

```text
node scripts/check-explorer-hot-update.cjs <Code> <cli.js> <baseline.vsix> <baseline-sha256> <candidate.vsix> <candidate-sha256>
```

Both packages must actually contain the stable update loader and complete update
inventories. Capability is checked from sealed bytes and the installed baseline,
without a version whitelist. Their inventory identities must differ. Genuine
modern release packages are preferred. Separately canonical-packaged same-version
fixtures are acceptable if their identities and fixture purpose are recorded;
never modify archive bytes while describing them as the shipping candidate.

The harness creates its own Code profile, extensions directory, workspace and
client configuration home. Native execution and automatic runtime setup stay
disabled, so this gate performs no native installation, shell/PATH change or OS
trust mutation. It does not attach to an existing user's Code window.
On macOS the profile uses a unique canonical mode-0700 `/tmp/airk-hot-*`
directory, keeping Code IPC socket paths short even when artifacts are stored
under a long campaign path. Windows retains its artifact-local profile. The
artifact ownership receipt binds the exact profile directory identity and
private marker. After confirmed graceful owned Code exit, macOS logs are copied
with bounded traversal before that exact profile is removed. Unknown exit or
changed ownership retains the profile and fails cleanup.

The real Code CLI installs the baseline. An independent development driver calls
the public commands of the installed AIRK extension. Actual webview automation
dismisses the native-execution-disabled notice and waits for the saved dismissal.
The CLI installs the candidate while that Code window and Explorer remain open.
The gate requires the candidate's sealed asset inventory in the actual new
webview, the same extension-host PID and random boot identity, one Explorer tab,
the preserved route, and continued dismissal across routes and close/reopen.
Only the harness-owned Code process is asked to quit during cleanup.
Route checks read the actual selected workspace and tab, rather than an unused
webview URL hash. Request and reply files publish by atomic rename. Cleanup has
its own bounded graceful-quit budget after the test deadline; an unacknowledged
quit or unobserved owned process exit leaves a failing process status and an
explicit cleanup receipt. No machine-wide process census is claimed.
Success requires a quit acknowledgment, observed exit code zero with no signal,
and no forced fallback. A successful result is published only after that cleanup;
forced, failed or uncertain termination preserves the macOS profile.

Receipts, installation output and failure diagnostics remain under
`artifacts/editor-qualification/hot-explorer-*`. A result qualifies this isolated
UI behavior only. Native upgrade, legacy removal, runtime readiness, MCP parity,
and platform release qualification require their separate actual gates. The
historical `check-vsix-upgrade-editor.cjs` intentionally restarts its old Code
process; its result does not substitute for this same-window gate.

Harness contract tests:

```text
node --test scripts/tests/check-explorer-hot-update.test.cjs
node --test scripts/tests/editor-owned-profile.test.cjs
```
