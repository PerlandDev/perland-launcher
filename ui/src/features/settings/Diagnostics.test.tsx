import { screen, waitFor } from '@testing-library/react';
import { expect, it } from 'vitest';
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
