import type { NoteGroup, ReleaseNotes } from '../../ipc/schema';

/** When the "last seen" marker moves to the running version: at once, only when the dialog opens, or not at all. */
export type Mark = 'now' | 'on-open' | 'never';

const baseOf = (version: string) => version.replace(/-beta\.\d+$/, '');

/** `YYYY.M.MICRO` or `YYYY.M.MICRO-beta.N` as numbers that sort (a stable after its betas); null for anything else. */
function parts(version: string): number[] | null {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-beta\.(\d+))?$/.exec(version);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? Infinity : Number(m[4])] : null;
}

/** Whether `a` is a newer version than `b`; false when either is not a version. */
function newer(a: string, b: string): boolean {
  const x = parts(a);
  const y = parts(b);
  if (!x || !y) return false;
  for (let i = 0; i < x.length; i += 1) {
    if (x[i] !== y[i]) return x[i]! > y[i]!;
  }
  return false;
}

/** What a launch shows of the release notes, and what happens to the marker (design: What's new, "The rule").
 *  Versions compare by their place in `notes.versions`, which lists every release up to this build in order; only a
 *  marker those don't list is compared by its numbers, to tell a downgrade from a release these notes never heard of. */
export function whatsNew({
  notes,
  running,
  seen,
  firstRunDone,
  dev,
}: {
  notes: ReleaseNotes;
  /** The launcher's own version. */
  running: string;
  /** The last version whose notes this player was shown; null when none. */
  seen: string | null;
  firstRunDone: boolean;
  /** A dev build: nothing opens by itself. */
  dev: boolean;
}): { groups: NoteGroup[]; mark: Mark } {
  if (dev) return { groups: [], mark: 'never' };
  // A fresh install has lived through none of it.
  if (!firstRunDone) return { groups: [], mark: 'now' };
  if (seen === running) return { groups: [], mark: 'never' };
  const runningAt = notes.versions.indexOf(running);
  const seenAt = seen === null ? -1 : notes.versions.indexOf(seen);
  // Notes that don't know this version: nothing to show.
  if (runningAt === -1) return { groups: [], mark: 'now' };
  // A downgrade: nothing here is news, and the marker stays, so the way back up repeats nothing.
  if (seenAt > runningAt || (seen !== null && seenAt === -1 && newer(seen, running))) return { groups: [], mark: 'never' };
  const base = baseOf(running);
  const groups = notes.groups
    .map((group) => ({
      ...group,
      notes: group.notes.filter((note) => {
        const at = note.since === null ? -1 : notes.versions.indexOf(note.since);
        if (at === -1 || at > runningAt) return false;
        // No marker, or one from a release these notes never heard of: the running version's own notes.
        return seenAt === -1 ? group.base === base : at > seenAt;
      }),
    }))
    .filter((group) => group.notes.length > 0);
  return { groups, mark: groups.length > 0 ? 'on-open' : 'now' };
}

/** The release note that tells players the launcher shares anonymous data (release-notes/anonymous-data.json). */
export const PRIVACY_NOTE = 'anonymous-data';

/** Does what the dialog shows include the note `id`? */
export const showsNote = (groups: NoteGroup[], id: string) => groups.some((group) => group.notes.some((note) => note.id === id));

/** `groups` with the released note `id` in them, whatever the marker says: a notice the player must be shown before
 *  it takes effect is news until they have seen it. Unchanged when it is already there, or when this build's notes
 *  don't carry it as released at or before the running version. */
export function withNote(groups: NoteGroup[], notes: ReleaseNotes, running: string, id: string): NoteGroup[] {
  if (showsNote(groups, id)) return groups;
  const runningAt = notes.versions.indexOf(running);
  for (const group of notes.groups) {
    const note = group.notes.find((n) => n.id === id);
    if (!note) continue;
    const at = note.since === null ? -1 : notes.versions.indexOf(note.since);
    if (at === -1 || at > runningAt) return groups;
    return [{ ...group, notes: [note] }, ...groups];
  }
  return groups;
}
