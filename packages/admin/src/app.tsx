// SPDX-License-Identifier: Apache-2.0
// The admin shell: session gate, navigation and the end-of-session screens.
import { h, type ComponentChildren, type JSX } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { ParsedResponse } from './envelope';
import { isDigest } from './operations';
import { IndexLibrary } from './indexLibrary';
import { Activity, type LogTarget } from './activity';
import { Agents } from './agents';
import { Help } from './help';
import { Logs, Overview, Recovery, Search, ServiceWorkspace, Storage } from './pages';
import { Text } from './render';
import { currentSession, logout, onSessionChange, takeInitialRoute, type Session } from './session';
import { Usage } from './usage';
import { BundledHelp } from './bundledHelp';
import { browserDestination, editorDestination, DESTINATION_ICONS, MAIN_DESTINATIONS, evidenceToolsPresentation, type EvidenceToolsNode } from '../../ui/src/navigation';
import {BROWSER_EVIDENCE_ROUTES,isBrowserEvidenceRoute} from './browserEvidenceRoutes';
import { PreparedActions } from './preparedActions';
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
/** Browser hub is inert: unavailable guided editor operations stay explicitly limited. */
export function SearchWorkspace(props:{route:Route;onView(tools:boolean,tool?:string):void}):JSX.Element {
  const tools=props.route.params.get('view')==='tools',guided=props.route.params.get('tool'),route=isBrowserEvidenceRoute(guided)?guided:undefined,queryMounted=useRef(false);const lastTool=useRef<string>();if(route)lastTool.current=route;
  if(!tools||route)queryMounted.current=true;
  const show=(value:EvidenceToolsNode|string):ComponentChildren=>typeof value==='string'?value:h(value.tag,value.href?{href:value.href}:value.route?{onClick:()=>props.onView(true,value.route)}:{},...value.children.map(show));
  const destination=route?editorDestination(route):'search',title=MAIN_DESTINATIONS.find(entry=>entry.id===destination)?.title??'Search';
  return <section aria-label={`${title} workspace`}><h1>{title}</h1><div class="actions" aria-label="Search views">
    <button type="button" class={tools?'secondary':''} aria-current={!tools?'page':undefined} onClick={()=>props.onView(false)}>Query</button>
    <button type="button" class={tools?'':'secondary'} aria-current={tools?'page':undefined} onClick={()=>props.onView(true)}>Evidence tools</button>
    </div>{queryMounted.current?<section hidden={tools&&!route}><Search guidedRoute={route} onGuidedRoute={route=>props.onView(true,route)}/></section>:null}{tools&&!route?<section class="evidence-tools" aria-label="Evidence tools hub">{lastTool.current?<button onClick={()=>props.onView(true,lastTool.current)}>Continue last guided tool</button>:null}{show(evidenceToolsPresentation('browser',undefined,BROWSER_EVIDENCE_ROUTES))}</section>:null}</section>;
}


function parseRoute(text: string, seq: number): Route | null {
  const fragment = text.startsWith('#') ? text.slice(1) : text;
  const mark = fragment.indexOf('?');
  const id = mark < 0 ? fragment : fragment.slice(0, mark);
  if (isBrowserEvidenceRoute(id)) return { page: 'search', params: new URLSearchParams({ view: 'tools', tool: id }), seq };
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
          <p>The service accepted stop for restart. This ends every admin session.</p>
          <p>
            After it stops, run <code>airkctl service start</code> locally, then run{' '}
            <code>airkctl admin open</code> for a new session. This page does not start a replacement service.
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

export function App(props: { initialGate: Gate }): JSX.Element {
  const [session, setSession] = useState<Session | null>(currentSession());
  const sessionOwner = useRef({ session, generation: 0 });
  if (sessionOwner.current.session !== session) sessionOwner.current = { session, generation: sessionOwner.current.generation + 1 };
  const [, refreshPrepared] = useState(0);
  useEffect(() => onPreparationChange(() => refreshPrepared(value => value + 1)), [session]);
  const [gate, setGate] = useState<Gate>(props.initialGate);
  const [menuOpen, setMenuOpen] = useState(false);
  const content = useRef<HTMLElement>(null);
  const [route, setRoute] = useState<Route>(() => {
    const initial = takeInitialRoute();
    return (initial === null ? null : parseRoute(initial, 1)) ?? { page: 'overview', params: new URLSearchParams(), seq: 0 };
  });
  const page = route.page;
  const pageTitle = page === 'search' ? null : PAGES.find(([id]) => id === page)?.[1];
  const retained = useRef({ generation: sessionOwner.current.generation, routes: new Map<PageId, Route>() });
  if (retained.current.generation !== sessionOwner.current.generation) retained.current = { generation: sessionOwner.current.generation, routes: new Map() };
  if (session !== null) retained.current.routes.set(page, route);
  const pageRoute = (id: PageId): Route => retained.current.routes.get(id) ?? route;
  const logId = (key: 'job' | 'project'): string | undefined => {
    const value = pageRoute('logs').params.get(key);
    return value !== null && isDigest(value) ? value : undefined;
  };
  const view = (id: PageId, children: ComponentChildren): ComponentChildren => retained.current.routes.has(id)
    ? <section hidden={page !== id}><PageVisible.Provider value={page === id}>{children}</PageVisible.Provider></section> : null;
  const setPage = (id: PageId): void => {
    setRoute((current) => ({ page: id, params: new URLSearchParams(), seq: current.seq + 1 }));
    if (id !== page) window.history.pushState(null, '', `#${id}`);
  };
  const navigateSearch = (tools:boolean,tool?:string):void => {
    const params=new URLSearchParams(tools?{view:'tools',...(isBrowserEvidenceRoute(tool)?{tool}:{})}:{});
    setRoute(current=>({page:'search',params,seq:current.seq+1}));
    const destination = isBrowserEvidenceRoute(tool) && tools ? `#${tool}` : `#search${params.size ? '?' + params : ''}`;
    if (window.location.hash !== destination) window.history.pushState(null, '', destination);
  };
  const navigateLogs = (target: LogTarget): void => {
    const params = new URLSearchParams();
    if (target.job) params.set('job', target.job);
    if (target.project) params.set('project', target.project);
    setRoute(current => ({ page: 'logs', params, seq: current.seq + 1 }));
    window.history.pushState(null, '', `#logs?${params}`);
  };
  const navigateCatalogItem = (selection?: { index?: string; set?: string }): void => {
    const params = new URLSearchParams(selection?.index ? { index: selection.index } : selection?.set ? { set: selection.set } : {});
    setRoute(current => ({ page: 'index-library', params, seq: current.seq + 1 }));
    const destination = `#index-library${params.size ? '?' + params : ''}`;
    if (window.location.hash !== destination) window.history.pushState(null, '', destination);
  };
  const navigateHelp = (params: URLSearchParams, replace: boolean): void => {
    setRoute(current => ({ page: 'help', params, seq: current.seq + 1 }));
    const destination = `#help${params.size ? `?${params}` : ''}`;
    if (replace) window.history.replaceState(null, '', destination);
    else window.history.pushState(null, '', destination);
  };

  useEffect(() => {
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
  }, []);

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
        <Signed gate={gate} />
        {page === 'help' ? <BundledHelp params={route.params} seq={route.seq} onNavigate={navigateHelp} />
          : <button type="button" class="secondary" onClick={() => setPage('help')}>Read offline Help</button>}
      </div>
    );
  }

  return (
    <div class="layout admin-console">
      <header>
        <span class="brand">AIRK Admin</span>
        <span class="muted instance">Local service · Signed in</span>
        <button class="secondary console-menu" type="button" aria-controls="console-navigation" aria-expanded={menuOpen} onClick={() => setMenuOpen(open => !open)}>Menu</button>
        <button type="button" class="small secondary" onClick={() => endWith({ kind: 'logged_out' })}>
          Sign out
        </button>
      </header>
      <nav id="console-navigation" class={menuOpen ? 'menu-open' : ''} aria-label="AIRK main navigation">
        {[
          { title: 'Manage', destinations: ['overview', 'indexes', 'activity', 'agents', 'service'] },
          { title: 'Investigate', destinations: ['search', 'code', 'docs', 'evidence', 'review', 'strategies'] },
          { title: 'Help', destinations: ['help'] },
        ].map(group => <section class="console-nav-group" key={group.title} aria-label={group.title}>
          <h2>{group.title}</h2>
          {MAIN_DESTINATIONS.filter(entry => group.destinations.includes(entry.id)).map(({ id, title, browser: target }) => (
          <button
            key={id}
            type="button"
            title={title} aria-label={title}
            class={`nav${id === (page === 'search' && route.params.get('tool') ? editorDestination(route.params.get('tool')!) : browserDestination(page)) ? ' current' : ''}${id === 'code' ? ' nav-divider' : id === 'help' ? ' nav-help' : ''}`}
            aria-current={id === (page === 'search' && route.params.get('tool') ? editorDestination(route.params.get('tool')!) : browserDestination(page)) ? 'page' : undefined}
            onClick={() => {
              if (['code', 'docs', 'evidence', 'review', 'strategies'].includes(id)) navigateSearch(true, MAIN_DESTINATIONS.find(entry => entry.id === id)!.editor);
              else setPage(target);
              setMenuOpen(false);
              content.current?.focus();
            }}
          >
            <svg class="destination-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={DESTINATION_ICONS[id]} /></svg><span class="nav-label">{title}</span>
          </button>
          ))}
        </section>)}
      </nav>
      <main ref={content} tabIndex={-1} aria-label="Admin workspace" key={sessionOwner.current.generation}>
        {pageTitle ? <h1>{pageTitle}</h1> : null}
        <PreparedActions />
        {browserDestination(page) === 'activity' ? <div class="actions" aria-label="Activity views">{(['activity', 'logs', 'usage'] as const).map(id =>
          <button type="button" class={page === id ? '' : 'secondary'} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{PAGES.find(entry => entry[0] === id)![1]}</button>)}</div> : null}
        {browserDestination(page) === 'service' ? <div class="actions" aria-label="Service views">{(['service', 'storage', 'recovery'] as const).map(id =>
          <button type="button" class={page === id ? '' : 'secondary'} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{PAGES.find(entry => entry[0] === id)![1]}</button>)}</div> : null}
        {view('overview', <Overview />)}
        {view('index-library', <IndexLibrary params={pageRoute('index-library').params} seq={pageRoute('index-library').seq}
          onActivity={() => setPage('activity')} onOpenItem={navigateCatalogItem} />)}
        {view('activity', <Activity onLogs={navigateLogs} />)}
        {view('agents', <Agents />)}
        {view('storage', <Storage />)}
        {view('recovery', <Recovery />)}
        {view('service', <ServiceWorkspace onLifecycle={action => endWith({ kind: action === 'stop' ? 'stopped' : 'restarted' })} />)}
        {view('logs', <Logs target={{ job: logId('job'), project: logId('project') }} />)}
        {view('usage', <Usage />)}
        {view('search', <SearchWorkspace route={pageRoute('search')} onView={navigateSearch} />)}
        {view('help', <Help params={pageRoute('help').params} seq={pageRoute('help').seq} onNavigate={navigateHelp} />)}
      </main>
    </div>
  );
}
