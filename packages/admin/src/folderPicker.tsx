// SPDX-License-Identifier: Apache-2.0
// The Browse dialog of the path fields (M24, `files.browse`).
//
// A browser page cannot learn an absolute path from a folder picker, so the
// service lists folders (never files) for it and the dialog only shows what it
// answers. Links and junctions are shown and never entered. Nothing is created
// here: a folder that does not exist yet is chosen by name, and the setup
// review says it will be created.
import type { JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { call, type CallResult } from './api';
import { body, OPS } from './operations';

export type FolderEntry = {
  name: string;
  path: string;
  kind: 'folder' | 'link';
  hidden?: boolean;
  start?: 'drive' | 'home' | 'airk_location';
  airk_location?: string;
  indexes?: { index: string; use: 'indexed_root' | 'index_data' }[];
};

export type FolderListing = {
  schema: number;
  path?: string;
  parent?: string;
  breadcrumb: { name: string; path: string }[];
  airk_location?: string;
  indexes?: { index: string; use: 'indexed_root' | 'index_data' }[];
  entries: FolderEntry[];
  continuation?: string;
  truncated: boolean;
  marks_complete: boolean;
};

const FAILURES: Record<string, string> = {
  folder_not_found: 'That folder does not exist.',
  folder_not_a_folder: 'That path is a file, not a folder.',
  folder_link_not_followed: 'That folder is a link or junction; open the folder it points to instead.',
  folder_not_readable: 'AIRK cannot read that folder with your account.',
  folder_path_not_absolute: 'Type a full path, such as D:\\code.',
};

/** The listing in a call result, or the failure text to show. */
export function listingOf(result: CallResult): { listing: FolderListing } | { error: string } {
  if (result.kind === 'ok') {
    const value = result.envelope.result as { listing?: FolderListing } | null;
    if (value !== null && typeof value === 'object' && value.listing !== undefined) return { listing: value.listing };
    return { error: 'The service answered without a folder listing.' };
  }
  if (result.kind === 'failed') {
    const code = result.envelope.error.code;
    return { error: FAILURES[code] ?? `The service refused the listing: ${code}.` };
  }
  if (result.kind === 'no_session') return { error: 'Sign in again to browse folders.' };
  return { error: 'The service did not answer the listing.' };
}

export async function browse(path: string | null, showHidden: boolean, after: string | null) {
  return listingOf(await call(OPS.filesBrowse, body.folderBrowse(path, showHidden, after)));
}

/** `parent` + `name` with the parent's own separator. */
export function joinPath(parent: string, name: string): string {
  if (parent.endsWith('\\') || parent.endsWith('/')) return parent + name;
  return parent + (parent.includes('\\') || !parent.startsWith('/') ? '\\' : '/') + name;
}

function marks(entry: { airk_location?: string; indexes?: FolderEntry['indexes'] }): string {
  const parts: string[] = [];
  if (entry.airk_location !== undefined) parts.push(`AIRK ${entry.airk_location} location`);
  for (const mark of entry.indexes ?? []) {
    parts.push(mark.use === 'indexed_root' ? `indexed: ${mark.index}` : `index data: ${mark.index}`);
  }
  return parts.join(' · ');
}

const START_LABEL: Record<string, string> = { drive: 'Drive', home: 'Home', airk_location: 'AIRK data' };

export function FolderPicker(props: {
  title: string;
  /** The folder to open at; empty or missing opens the starting list. */
  initial: string;
  /** Whether a new subfolder may be named (index storage that setup creates). */
  allowNew?: boolean;
  onPick: (path: string) => void;
  onClose: () => void;
}): JSX.Element {
  const [starts, setStarts] = useState<FolderEntry[]>([]);
  const [listing, setListing] = useState<FolderListing | null>(null);
  const [typed, setTyped] = useState(props.initial);
  const [hidden, setHidden] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [newName, setNewName] = useState('');

  const open = async (path: string | null, after: string | null = null, showHidden = hidden): Promise<void> => {
    setBusy(true);
    const answer = await browse(path, showHidden, after);
    setBusy(false);
    if ('error' in answer) {
      setError(answer.error);
      return;
    }
    setError(null);
    const next = answer.listing;
    if (after !== null && listing !== null && next.path === listing.path) {
      setListing({ ...next, entries: [...listing.entries, ...next.entries] });
    } else {
      setListing(next);
      setNewName('');
    }
    if (next.path === undefined) setStarts(next.entries);
    setTyped(next.path ?? '');
  };

  useEffect(() => {
    void (async () => {
      const first = await browse(null, false, null);
      if ('listing' in first) setStarts(first.listing.entries);
      const initial = props.initial.trim();
      if (initial === '') {
        if ('listing' in first) setListing(first.listing);
        else setError(first.error);
        return;
      }
      const opened = await browse(initial, false, null);
      if ('listing' in opened) {
        setListing(opened.listing);
        return;
      }
      // A folder that does not exist yet: show its nearest listed parent.
      const parent = initial.replace(/[\\/][^\\/]*[\\/]?$/, '');
      const up = parent !== '' && parent !== initial ? await browse(parent, false, null) : opened;
      if ('listing' in up) {
        setListing(up.listing);
        if (props.allowNew) setNewName(initial.slice(parent.length).replace(/^[\\/]+|[\\/]+$/g, ''));
      } else if ('listing' in first) {
        setListing(first.listing);
      }
      setError(opened.error);
    })();
  }, []);

  const here = listing?.path ?? null;
  const cleanNew = newName.trim();
  const newValid = cleanNew === '' || /^[^\\/:*?"<>|]{1,128}$/.test(cleanNew);
  const selected = here === null ? null : cleanNew !== '' && newValid ? joinPath(here, cleanNew) : here;

  return (
    <div class="overlay" role="dialog" aria-modal="true" aria-label={props.title}>
      <div class="picker">
        <h3>{props.title}</h3>
        <div class="actions">
          <span class="muted">Start at:</span>
          {starts.map((entry) => (
            <button
              type="button"
              class="small"
              key={entry.path}
              title={entry.path}
              onClick={() => void open(entry.path)}
            >
              {START_LABEL[entry.start ?? ''] ?? 'Folder'}: {entry.name}
            </button>
          ))}
        </div>
        <form
          class="row"
          onSubmit={(event) => {
            event.preventDefault();
            void open(typed.trim() === '' ? null : typed.trim());
          }}
        >
          <label class="field wide">
            Path
            <input
              type="text"
              autocomplete="off"
              spellcheck={false}
              value={typed}
              onInput={(event) => setTyped(event.currentTarget.value)}
            />
          </label>
          <button type="submit" disabled={busy}>
            Go
          </button>
        </form>
        {listing !== null && listing.path !== undefined ? (
          <div class="actions crumbs">
            {listing.breadcrumb.map((crumb, index) => (
              <span key={crumb.path}>
                {index > 0 ? ' › ' : ''}
                <button type="button" class="link" onClick={() => void open(crumb.path)}>
                  {crumb.name}
                </button>
              </span>
            ))}
            <button type="button" class="small" onClick={() => void open(listing.parent ?? null)}>
              ↑ Parent
            </button>
          </div>
        ) : null}
        {error !== null ? <p class="error">{error}</p> : null}
        {listing !== null && marks(listing) !== '' ? <p class="muted">This folder: {marks(listing)}</p> : null}
        <ul class="folders">
          {(listing?.entries ?? []).map((entry) => (
            <li key={entry.path}>
              {entry.kind === 'link' ? (
                <span class="muted" title={entry.path}>
                  ⤷ {entry.name} (link: not followed)
                </span>
              ) : (
                <button type="button" class="link" title={entry.path} onClick={() => void open(entry.path)}>
                  📁 {entry.name}
                </button>
              )}
              {entry.hidden ? <span class="muted"> (hidden)</span> : null}
              {marks(entry) !== '' ? <span class="muted"> — {marks(entry)}</span> : null}
            </li>
          ))}
          {listing !== null && listing.entries.length === 0 ? <li class="muted">No folders here.</li> : null}
        </ul>
        <div class="actions">
          {listing?.continuation !== undefined && here !== null ? (
            <button type="button" class="small" disabled={busy} onClick={() => void open(here, listing.continuation ?? null)}>
              Show more
            </button>
          ) : null}
          {here !== null ? (
            <label class="check">
              <input
                type="checkbox"
                checked={hidden}
                onChange={(event) => {
                  setHidden(event.currentTarget.checked);
                  void open(here, null, event.currentTarget.checked);
                }}
              />
              Show hidden folders
            </label>
          ) : null}
        </div>
        {listing?.truncated ? <p class="muted">This folder has very many entries; some folders may not be listed.</p> : null}
        {listing !== null && !listing.marks_complete ? (
          <p class="muted">Index marks could not all be read; a folder without a mark may still be indexed.</p>
        ) : null}
        {props.allowNew && here !== null ? (
          <label class="field wide">
            New folder inside this one (optional; the setup creates it)
            <input
              type="text"
              autocomplete="off"
              spellcheck={false}
              class={newValid ? '' : 'invalid'}
              value={newName}
              onInput={(event) => setNewName(event.currentTarget.value)}
            />
          </label>
        ) : null}
        <p>
          Selected: <code>{selected ?? 'choose a folder'}</code>
        </p>
        <div class="actions">
          <button type="button" class="secondary" onClick={props.onClose}>
            Cancel
          </button>
          <button
            type="button"
            disabled={selected === null || !newValid}
            onClick={() => {
              if (selected !== null) props.onPick(selected);
            }}
          >
            Use this folder
          </button>
        </div>
      </div>
    </div>
  );
}
