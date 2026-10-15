import { screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { Settings } from './Settings';

it('saves diagnostics where the player picks and offers Show in folder', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true });
  await user.click(await screen.findByRole('button', { name: 'Export diagnostics' }));
  await waitFor(() => expect(sent(calls, 'diagnostics_export')).toHaveLength(1));
  expect(sent(calls, 'diagnostics_export')[0]).toMatchObject({ path: expect.stringMatching(/\.zip$/) });
  await waitFor(() => expect(toastsSnapshot().some((t) => t.notice?.title === 'Diagnostics saved')).toBe(true));
  toastsSnapshot().find((t) => t.notice?.title === 'Diagnostics saved')!.notice!.action!.run();
  expect(sent(calls, 'diagnostics_reveal')).toHaveLength(1);
});

it('does nothing when the save dialog is cancelled', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, savePath: null });
  await user.click(await screen.findByRole('button', { name: 'Export diagnostics' }));
  await new Promise((r) => setTimeout(r, 20));
  expect(sent(calls, 'diagnostics_export')).toHaveLength(0);
});

it("names the file with the player's local date, not the UTC one", async () => {
  // Just after local midnight: west of UTC it is still the 14th there; east of it, already the 15th.
  // Either way the UTC date differs from the local one at one end of the day, so try both ends.
  for (const hour of [0, 23]) {
    vi.useFakeTimers({ toFake: ['Date'] });
    try {
      vi.setSystemTime(new Date(2026, 9, 15, hour, 30));
      const { user, calls, unmount } = renderApp(<Settings />, { beta: true });
      await user.click(await screen.findByRole('button', { name: 'Export diagnostics' }));
      await waitFor(() => expect(sent(calls, 'plugin:dialog|save')).toHaveLength(1));
      expect(JSON.stringify(sent(calls, 'plugin:dialog|save')[0])).toContain('perland-diagnostics-2026-10-15.zip');
      unmount();
    } finally {
      vi.useRealTimers();
    }
  }
});
