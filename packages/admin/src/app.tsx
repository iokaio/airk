// SPDX-License-Identifier: Apache-2.0
// The admin shell: session gate, navigation and the end-of-session screens.
import type { ComponentChildren, JSX } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { ParsedResponse } from './envelope';
import { IndexLibrary } from './indexLibrary';
import { Activity } from './activity';
import { Agents } from './agents';
import { Help } from './help';
import { Logs, Overview, Recovery, Search, Service, Storage } from './pages';
import { Text } from './render';
import { currentSession, logout, onSessionChange, takeInitialRoute, type Session } from './session';
import { Usage } from './usage';
import { BundledHelp } from './bundledHelp';
import { browserDestination, MAIN_DESTINATIONS } from '../../ui/src/navigation';
import { PreparedActions } from './preparedActions';
import { Arbiter } from './arbiter';
import { onPreparationChange } from './api';
import { PageVisible } from './pageVisibility';

const PAGES = [
  ['overview', 'Overview'],
  ['index-library', 'Index library'],
  ['activity', 'Activity'],
  ['agents', 'Agents'],
  ['storage', 'Storage'],
  ['recovery', 'Recovery'],
  ['service', 'Service'],
  ['logs', 'Logs'],
  ['usage', 'Usage'],
  ['search', 'Search'],
  ['help', 'Help'],
] as const;

export type PageId = (typeof PAGES)[number][0];

/**
 * A page route from the fragment, `#<page>[?<query>]`, for example
 * `#index-library?root=<folder>` (U97). A route only selects a page and
 * prefills fields; it never starts an action. `seq` changes on every route so
 * a page can apply the same prefill again.
 */
export type Route = { page: PageId; params: URLSearchParams; seq: number };

function parseRoute(text: string, seq: number): Route | null {
  const fragment = text.startsWith('#') ? text.slice(1) : text;
  const mark = fragment.indexOf('?');
  const id = mark < 0 ? fragment : fragment.slice(0, mark);
  const entry = PAGES.find(([page]) => page === id);
  if (entry === undefined) return null;
  return { page: entry[0], params: new URLSearchParams(mark < 0 ? '' : fragment.slice(mark + 1)), seq };
}

/** Why the shell has no session. */
export type Gate =
  | { kind: 'none' }
  | { kind: 'exchange_failed'; response: ParsedResponse }
  | { kind: 'expired' }
  | { kind: 'logged_out' }
  | { kind: 'stopped' }
  | { kind: 'restarted' };

function OpenHint(): JSX.Element {
  return (
    <p>
      Open with <code>airkctl admin open</code>. AIRK pages never ask you to paste a code, URL or token.
    </p>
  );
}

function Signed(props: { gate: Gate }): JSX.Element {
  const g = props.gate;
  switch (g.kind) {
    case 'none':
      return <OpenHint />;
    case 'logged_out':
      return (
        <>
          <p>Signed out.</p>
          <OpenHint />
        </>
      );
    case 'expired':
      return (
        <>
          <p class="error">
            Session expired; run <code>airkctl admin open</code>.
          </p>
        </>
      );
    case 'stopped':
      return (
        <>
          <p>The service is stopping and this page has no server any more.</p>
          <p>
            Start it again with <code>airk</code>, <code>airkctl service start</code>, or Start in the editor
            extension. Then run <code>airkctl admin open</code> for a new session.
          </p>
        </>
      );
    case 'restarted':
      return (
        <>
          <p>The service is restarting. A restart ends every admin session.</p>
          <p>
            When it is back, run <code>airkctl admin open</code>. If it does not come back, start it with{' '}
            <code>airk</code> or <code>airkctl service start</code>.
          </p>
        </>
      );
    case 'exchange_failed': {
      const r = g.response;
      if (r.kind === 'failed' && r.envelope.error.code === 'admin_launch_code_invalid') {
        return (
          <>
            <p class="error">This sign-in link was already used or has expired.</p>
            <OpenHint />
          </>
        );
      }
      if (r.kind === 'failed') {
        return (
          <>
            <p class="error">
              Sign-in was refused: <code><Text value={r.envelope.error.code} /></code> ({r.envelope.error.class}).
            </p>
            <OpenHint />
          </>
        );
      }
      return (
        <>
          <p class="error">
            Sign-in failed: <Text value={r.kind === 'transport' ? r.message : 'unexpected reply'} />
          </p>
          <OpenHint />
        </>
      );
    }
  }
}

export function App(props: { initialGate: Gate; embedded?: boolean; active?: boolean; initialPage?: PageId; onPageChange?: (page: PageId) => void }): JSX.Element {
  const [session, setSession] = useState<Session | null>(currentSession());
  const sessionOwner = useRef({ session, generation: 0 });
  if (sessionOwner.current.session !== session) sessionOwner.current = { session, generation: sessionOwner.current.generation + 1 };
  const [, refreshPrepared] = useState(0);
  useEffect(() => onPreparationChange(() => refreshPrepared(value => value + 1)), [session]);
  const [gate, setGate] = useState<Gate>(props.initialGate);
  const [route, setRoute] = useState<Route>(() => {
    if (props.embedded) return { page: props.initialPage ?? 'overview', params: new URLSearchParams(), seq: 0 };
    const initial = takeInitialRoute();
    return (initial === null ? null : parseRoute(initial, 1)) ?? { page: 'overview', params: new URLSearchParams(), seq: 0 };
  });
  const page = route.page;
  const retained = useRef({ generation: sessionOwner.current.generation, routes: new Map<PageId, Route>() });
  if (retained.current.generation !== sessionOwner.current.generation) retained.current = { generation: sessionOwner.current.generation, routes: new Map() };
  if (session !== null && props.embedded) retained.current.routes.set(page, route);
  const pageRoute = (id: PageId): Route => retained.current.routes.get(id) ?? route;
  const view = (id: PageId, children: ComponentChildren): ComponentChildren => page === id || (props.embedded && retained.current.routes.has(id))
    ? <section hidden={page !== id}><PageVisible.Provider value={page === id && props.active !== false}>{children}</PageVisible.Provider></section> : null;
  // The Explorer owns navigation in the embedded host. Synchronize without
  // remounting the exact-session draft/result owners or sending navigation back.
  useLayoutEffect(() => {
    if (props.embedded && props.initialPage !== undefined) setRoute(current => current.page === props.initialPage ? current
      : { page: props.initialPage!, params: new URLSearchParams(), seq: current.seq + 1 });
  }, [props.embedded, props.initialPage]);
  const setPage = (id: PageId): void => {
    setRoute((current) => ({ page: id, params: new URLSearchParams(), seq: current.seq + 1 }));
    props.onPageChange?.(id);
    // Keeps a same-tab reload on this page; replaceState fires no hashchange.
    if (!props.embedded) window.history.replaceState(null, '', `#${id}`);
  };
  const navigateHelp = (params: URLSearchParams, replace: boolean): void => {
    setRoute(current => ({ page: 'help', params, seq: current.seq + 1 }));
    const destination = `#help${params.size ? `?${params}` : ''}`;
    if (!props.embedded) {
      if (replace) window.history.replaceState(null, '', destination);
      else window.history.pushState(null, '', destination);
    }
  };

  useEffect(() => {
    if (props.embedded) return;
    const onHash = (): void => {
      const hash = window.location.hash;
      // A launch code never stays in the address bar (review M1); it is used once, on load.
      if (hash.startsWith('#c=')) {
        window.history.replaceState(null, '', window.location.pathname);
        return;
      }
      setRoute((current) => parseRoute(hash, current.seq + 1) ?? current);
    };
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, [props.embedded]);

  // Observe session revocation before mounted controls can issue a request.
  // A passive listener can miss an immediate native auth-ended notification.
  useLayoutEffect(
    () =>
      onSessionChange((next) => {
        setSession(next);
        // A session that disappears without a local reason was refused by the service (401).
        if (next === null) setGate((g) => (g.kind === 'none' || g.kind === 'exchange_failed' ? { kind: 'expired' } : g));
      }),
    [],
  );

  const endWith = (next: Gate): void => {
    setGate(next);
    void logout();
  };

  if (session === null) {
    return (
      <div class="gate">
        <h1>AIRK Admin</h1>
        {props.embedded ? <p>The captured native management session ended. Reopen management after verifying the service connection. No original action will be replayed.</p> : <Signed gate={gate} />}
        {page === 'help' ? <BundledHelp params={route.params} seq={route.seq} onNavigate={navigateHelp} />
          : <button type="button" class="secondary" onClick={() => setPage('help')}>Read offline Help</button>}
      </div>
    );
  }

  return (
    <div class="layout">
      {!props.embedded ? <>
      <header>
        <span class="brand">AIRK Admin</span>
        <span class="muted instance" title="service instance">
          {session.serviceInstance.slice(0, 8)}
        </span>
        <button type="button" class="small secondary" onClick={() => endWith({ kind: 'logged_out' })}>
          Sign out
        </button>
      </header>
      <nav aria-label="AIRK main navigation">
        {MAIN_DESTINATIONS.map(({ id, title, browser: target }) => (
          <button
            key={id}
            type="button"
            class={id === browserDestination(page) ? 'nav current' : 'nav'}
            aria-current={id === browserDestination(page) ? 'page' : undefined}
            onClick={() => setPage(target)}
          >
            {title}
          </button>
        ))}
      </nav>
      </> : null}
      <main key={sessionOwner.current.generation}>
        <PreparedActions />
        {browserDestination(page) === 'activity' ? <div class="actions" aria-label="Activity views">{(['activity', 'logs', 'usage'] as const).map(id =>
          <button type="button" class={page === id ? '' : 'secondary'} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{PAGES.find(entry => entry[0] === id)![1]}</button>)}</div> : null}
        {browserDestination(page) === 'service' ? <div class="actions" aria-label="Service views">{(['service', 'storage', 'recovery'] as const).map(id =>
          <button type="button" class={page === id ? '' : 'secondary'} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{PAGES.find(entry => entry[0] === id)![1]}</button>)}</div> : null}
        {view('overview', <Overview />)}
        {view('index-library', <IndexLibrary params={pageRoute('index-library').params} seq={pageRoute('index-library').seq} onActivity={() => setPage('activity')} />)}
        {view('activity', <Activity />)}
        {view('agents', <Agents />)}
        {view('storage', <Storage />)}
        {view('recovery', <Recovery />)}
        {view('service',
          <><Service onLifecycle={(action) => endWith({ kind: action === 'stop' ? 'stopped' : 'restarted' })} />
            <details class="panel"><summary>Optional local Arbiter</summary><Arbiter /></details></>)}
        {view('logs', <Logs />)}
        {view('usage', <Usage />)}
        {view('search', <Search />)}
        {view('help', <Help params={pageRoute('help').params} seq={pageRoute('help').seq} onNavigate={navigateHelp} />)}
      </main>
    </div>
  );
}
