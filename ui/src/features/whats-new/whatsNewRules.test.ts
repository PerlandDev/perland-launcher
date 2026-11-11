import { describe, expect, it } from 'vitest';
import type { ReleaseNote, ReleaseNotes } from '../../ipc/schema';
import { whatsNew } from './whatsNewRules';

const note = (id: string, since: string | null, kind: ReleaseNote['kind'] = 'new'): ReleaseNote => ({ id, kind, since, en: `${id} en`, es: `${id} es` });

const notes: ReleaseNotes = {
  versions: ['2026.11.4', '2026.11.5-beta.1', '2026.11.5', '2026.11.6'],
  groups: [
    { base: '2026.11.6', date: '2026-11-20', notes: [note('c', '2026.11.6')] },
    { base: '2026.11.5', date: '2026-11-12', notes: [note('b', '2026.11.5'), note('a', '2026.11.5-beta.1', 'fixed')] },
  ],
};
/** The same history, had 2026.11.5 itself added nothing after its beta. */
const withoutB: ReleaseNotes = { ...notes, groups: [notes.groups[0]!, { ...notes.groups[1]!, notes: [note('a', '2026.11.5-beta.1', 'fixed')] }] };

type Input = Parameters<typeof whatsNew>[0];
const run = (input: Partial<Input>) => {
  const { groups, mark } = whatsNew({ notes, running: '2026.11.6', seen: null, firstRunDone: true, dev: false, ...input });
  return { shown: groups.map((g) => [g.base, g.notes.map((n) => n.id)]), mark };
};

describe('whatsNew', () => {
  it('shows nothing in a dev build and leaves the marker alone', () => {
    expect(run({ running: '0.2.15', dev: true })).toEqual({ shown: [], mark: 'never' });
  });

  it('shows nothing during first run, and counts the running version as seen', () => {
    expect(run({ firstRunDone: false })).toEqual({ shown: [], mark: 'now' });
  });

  it('shows nothing once the running version was seen', () => {
    expect(run({ seen: '2026.11.6' })).toEqual({ shown: [], mark: 'never' });
  });

  it('shows the notes of one update', () => {
    expect(run({ seen: '2026.11.5' })).toEqual({ shown: [['2026.11.6', ['c']]], mark: 'on-open' });
  });

  it('shows every update the player skipped, newest first', () => {
    expect(run({ seen: '2026.11.4' })).toEqual({
      shown: [
        ['2026.11.6', ['c']],
        ['2026.11.5', ['b', 'a']],
      ],
      mark: 'on-open',
    });
  });

  it("shows only what the stable added to a beta player's own beta", () => {
    expect(run({ running: '2026.11.5', seen: '2026.11.5-beta.1' })).toEqual({ shown: [['2026.11.5', ['b']]], mark: 'on-open' });
  });

  it('never shows notes of a version newer than the one running', () => {
    expect(run({ running: '2026.11.5', seen: '2026.11.4' })).toEqual({ shown: [['2026.11.5', ['b', 'a']]], mark: 'on-open' });
  });

  it('opens nothing for an update with no notes, and counts it as seen', () => {
    expect(run({ notes: withoutB, running: '2026.11.5', seen: '2026.11.5-beta.1' })).toEqual({ shown: [], mark: 'now' });
  });

  it("shows the running version's notes to a player with no marker", () => {
    expect(run({ running: '2026.11.5', seen: null })).toEqual({ shown: [['2026.11.5', ['b', 'a']]], mark: 'on-open' });
  });

  it("shows the running version's notes when the marker is a version it does not know", () => {
    expect(run({ seen: '2026.11.5-beta.9' })).toEqual({ shown: [['2026.11.6', ['c']]], mark: 'on-open' });
  });

  it('shows nothing after a downgrade', () => {
    expect(run({ running: '2026.11.5', seen: '2026.11.6' })).toEqual({ shown: [], mark: 'now' });
  });

  it('shows nothing when the notes do not know the running version', () => {
    expect(run({ running: '2027.1.0', seen: '2026.11.6' })).toEqual({ shown: [], mark: 'now' });
  });

  it('never shows unreleased notes by itself', () => {
    const next: ReleaseNotes = { versions: notes.versions, groups: [{ base: null, date: '2026-11-21', notes: [note('d', null)] }, ...notes.groups] };
    expect(run({ notes: next, seen: '2026.11.5' })).toEqual({ shown: [['2026.11.6', ['c']]], mark: 'on-open' });
  });
});
