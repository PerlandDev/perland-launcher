import { expect, it, vi } from 'vitest';

vi.mock('virtual:release-notes', () => ({ default: { versions: 'not a list' } }));

it('starts with no notes when the bundled file is not what it expects, so the launcher still opens', async () => {
  const { releaseNotesData } = await import('./notes');
  expect(releaseNotesData).toEqual({ versions: [], groups: [] });
});
