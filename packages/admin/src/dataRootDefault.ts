// SPDX-License-Identifier: Apache-2.0
// The data root a new index setup uses. An empty field always means "the
// default": a folder named after the collection inside the installation's
// data location (the native starting list's `airk_location: "data"` entry).
// The review never waits on the field being pre-filled.
import { joinPath } from './folderPicker';

export type DataRootChoice =
  /** Send this data root. */
  | { root: string; isDefault: boolean }
  /** The default needs the installation's data location; look it up first. */
  | { pending: true }
  /** Nothing can be sent yet; show why. */
  | { missing: string };

export function dataRootChoice(input: {
  typed: string;
  dataBase: string | null;
  lookupError: string | null;
  collection: string;
  collectionValid: boolean;
}): DataRootChoice {
  const typed = input.typed.trim();
  if (typed !== '') return { root: typed, isDefault: false };
  if (!input.collectionValid) {
    return { missing: 'Enter a valid collection name; the default data root is a folder named after it.' };
  }
  if (input.dataBase !== null) return { root: joinPath(input.dataBase, input.collection), isDefault: true };
  if (input.lookupError !== null) {
    return {
      missing: `The installation's data location could not be read (${input.lookupError}). Try again, or choose a data root.`,
    };
  }
  return { pending: true };
}

/** The installation's data location in a native starting list, if listed. */
export function dataLocationOf(listing: { entries: { path: string; airk_location?: string }[] }): string | null {
  return listing.entries.find((entry) => entry.airk_location === 'data')?.path ?? null;
}
