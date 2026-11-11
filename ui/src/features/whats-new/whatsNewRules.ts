import type { NoteGroup, ReleaseNotes } from '../../ipc/schema';

/** When the "last seen" marker moves to the running version: at once, only when the dialog opens, or not at all. */
export type Mark = 'now' | 'on-open' | 'never';

const baseOf = (version: string) => version.replace(/-beta\.\d+$/, '');

/** What a launch shows of the release notes, and what happens to the marker (design: What's new, "The rule").
 *  Versions compare by their place in `notes.versions`, which lists every release in order. */
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
  // Notes that don't know this version, or a downgrade: nothing here is news.
  if (runningAt === -1 || seenAt > runningAt) return { groups: [], mark: 'now' };
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
