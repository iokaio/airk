// SPDX-License-Identifier: Apache-2.0
// Entry point for both /admin/ and /admin/launch (one document, review M1).
import { render } from 'preact';
import { App, type Gate } from './app';
import { exchange, restoreSession, takeLaunchCode } from './session';

async function start(): Promise<void> {
  // Before any other action: read and clear `#c=` (review M1, I3).
  const code = takeLaunchCode();
  let gate: Gate = { kind: 'none' };
  if (code !== null) {
    const outcome = await exchange(code);
    if (outcome.kind === 'failed') gate = { kind: 'exchange_failed', response: outcome.response };
  } else {
    // Same tab reload only; a new tab has no token and makes no data call.
    restoreSession();
  }
  const root = document.getElementById('app');
  if (root === null) return;
  root.className = '';
  root.replaceChildren();
  render(<App initialGate={gate} />, root);
}

void start().catch(() => {
  const root = document.getElementById('app');
  if (root === null) return;
  root.className = 'gate';
  root.textContent = 'AIRK Admin could not start. Run airkctl admin open to open a fresh session.';
});
