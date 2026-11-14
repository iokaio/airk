// SPDX-License-Identifier: Apache-2.0
// Launch-code exchange and anti-forgery token custody (contracts §4.5, §4.6 as
// amended by §4.11 and security-review-i3.md M1/M2/M5).
//
// M1: /admin/launch serves this same document. The shell reads and clears `#c=`
// with history.replaceState before any other action, exchanges the code and keeps
// the returned token in memory without navigating. The token may be mirrored in
// sessionStorage under a key that includes service_instance so a reload of the
// same tab keeps working; it is removed at logout and on any 401. It is never put
// in a URL, cookie, localStorage, IndexedDB or the DOM. A new tab has no token.
import type { ExchangeResponse } from '../../contracts-gen/i3';
import { endsSession, parseEnvelope, type ParsedResponse } from './envelope';

const STORAGE_PREFIX = 'airk-admin-csrf:';
const SHELL_PATH = '/admin/';
const EXCHANGE_PATH = '/admin/api/v1/session';
const LOGOUT_PATH = '/admin/api/v1/session/logout';

export type Session = {
  readonly csrf: string;
  readonly serviceInstance: string;
  /** Wall-clock deadlines computed from the exchange reply; informational only. */
  readonly idleExpiresInMs: number | null;
  readonly absoluteDeadline: number | null;
};

let current: Session | null = null;
const listeners = new Set<(session: Session | null) => void>();

export function currentSession(): Session | null {
  return current;
}

export function onSessionChange(listener: (session: Session | null) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function publish(session: Session | null): void {
  current = session;
  for (const listener of listeners) listener(session);
}

/**
 * Reads the launch code from the fragment and removes it from the address bar and
 * the current history entry. Must run before anything else on the page.
 */
export function takeLaunchCode(): string | null {
  const hash = window.location.hash;
  let code = '';
  let route: string | null = null;
  if (hash.startsWith('#c=')) {
    // `#c=<code>[&route=<encoded page route>]`; the code alphabet is base64url.
    const [first, ...rest] = hash.slice(3).split('&');
    code = first ?? '';
    for (const part of rest) {
      if (part.startsWith('route=')) {
        try {
          route = decodeURIComponent(part.slice(6));
        } catch {
          route = null;
        }
      }
    }
  } else if (hash.length > 1) {
    route = hash.slice(1);
  }
  initialRoute = route !== null && PAGE_ROUTE.test(route) ? route : null;
  const keep = initialRoute === null ? SHELL_PATH : `${SHELL_PATH}#${initialRoute}`;
  if (
    hash !== (initialRoute === null ? '' : `#${initialRoute}`) ||
    window.location.pathname !== SHELL_PATH ||
    window.location.search !== ''
  ) {
    window.history.replaceState(null, '', keep);
  }
  return code.length > 0 ? code : null;
}

/** A page route such as `index-library?root=…`: a page id and an optional query. */
const PAGE_ROUTE = /^[a-z][a-z-]{0,31}(\?[^#\s]*)?$/;
let initialRoute: string | null = null;

/** The page route the document was opened with (after any launch code), or null. */
export function takeInitialRoute(): string | null {
  const route = initialRoute;
  initialRoute = null;
  return route;
}

function storageKeys(): string[] {
  try {
    const keys: string[] = [];
    for (let i = 0; i < window.sessionStorage.length; i += 1) {
      const key = window.sessionStorage.key(i);
      if (key !== null && key.startsWith(STORAGE_PREFIX)) keys.push(key);
    }
    return keys;
  } catch {
    return [];
  }
}

function forgetMirror(): void {
  for (const key of storageKeys()) {
    try {
      window.sessionStorage.removeItem(key);
    } catch {
      // Storage unavailable: nothing was mirrored.
    }
  }
}

function mirror(session: Session): void {
  forgetMirror();
  try {
    window.sessionStorage.setItem(`${STORAGE_PREFIX}${session.serviceInstance}`, session.csrf);
  } catch {
    // Without sessionStorage a reload needs a new `airkctl admin open`.
  }
}

/** Restores this tab's token after a reload (M1); a new tab finds nothing. */
export function restoreSession(): Session | null {
  const keys = storageKeys();
  if (keys.length !== 1) {
    forgetMirror();
    return null;
  }
  const key = keys[0]!;
  let csrf: string | null = null;
  try {
    csrf = window.sessionStorage.getItem(key);
  } catch {
    csrf = null;
  }
  if (csrf === null || csrf.length === 0) {
    forgetMirror();
    return null;
  }
  const session: Session = {
    csrf,
    serviceInstance: key.slice(STORAGE_PREFIX.length),
    idleExpiresInMs: null,
    absoluteDeadline: null,
  };
  publish(session);
  return session;
}

/** Ends the local session state (logout, any 401, stop or restart). */
export function dropSession(): void {
  forgetMirror();
  publish(null);
}

function isExchangeResponse(value: unknown): value is ExchangeResponse {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.csrf === 'string' &&
    v.csrf.length > 0 &&
    typeof v.service_instance === 'string' &&
    typeof v.idle_expires_in_ms === 'number' &&
    typeof v.absolute_expires_in_ms === 'number'
  );
}

export type ExchangeOutcome =
  | { kind: 'ok'; session: Session }
  | { kind: 'failed'; response: ParsedResponse };

export async function exchange(code: string): Promise<ExchangeOutcome> {
  const originalSession = current;
  let response: Response;
  try {
    response = await fetch(EXCHANGE_PATH, {
      method: 'POST',
      credentials: 'same-origin',
      cache: 'no-store',
      redirect: 'error',
      referrerPolicy: 'no-referrer',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
    });
  } catch {
    return { kind: 'failed', response: { kind: 'transport', status: null,
      message: 'The service did not answer the session request. Run airkctl admin open for a fresh session.' } };
  }
  if (response.status === 401 && current === originalSession) dropSession();
  let text: string;
  try {
    text = await response.text();
  } catch {
    return { kind: 'failed', response: { kind: 'transport', status: response.status,
      message: `HTTP ${response.status}: the session response body could not be read. Open a fresh Admin session with airkctl admin open.` } };
  }
  const parsed = parseEnvelope(response.status, text);
  if (endsSession(parsed) && current === originalSession) dropSession();
  if (parsed.kind !== 'ok' || !isExchangeResponse(parsed.envelope.result)) {
    return {
      kind: 'failed',
      response:
        parsed.kind === 'ok'
          ? { kind: 'transport', status: response.status, message: 'The exchange reply has no session.' }
          : parsed,
    };
  }
  const result = parsed.envelope.result;
  const session: Session = {
    csrf: result.csrf,
    serviceInstance: result.service_instance,
    idleExpiresInMs: result.idle_expires_in_ms,
    absoluteDeadline: Date.now() + result.absolute_expires_in_ms,
  };
  mirror(session);
  publish(session);
  return { kind: 'ok', session };
}

export async function logout(): Promise<void> {
  const session = current;
  dropSession();
  if (session === null) return;
  try {
    await fetch(LOGOUT_PATH, {
      method: 'POST',
      credentials: 'same-origin',
      cache: 'no-store',
      redirect: 'error',
      referrerPolicy: 'no-referrer',
      headers: { 'Content-Type': 'application/json', 'X-AIRK-CSRF': session.csrf },
      body: '{}',
    });
  } catch {
    // The local state is already gone; the server session expires on its own.
  }
}
