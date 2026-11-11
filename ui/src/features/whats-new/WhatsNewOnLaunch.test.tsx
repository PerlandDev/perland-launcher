import { QueryClientProvider } from '@tanstack/react-query';
import { act, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import type { ReleaseNotes } from '../../ipc/schema';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { WhatsNewOnLaunch } from './WhatsNewOnLaunch';

// The mock backend runs version 0.1.0 (contract/app_info.json).
const notes: ReleaseNotes = {
  versions: ['0.0.9', '0.1.0'],
  groups: [{ base: '0.1.0', date: '2026-11-20', notes: [{ id: 'c', kind: 'new', since: '0.1.0', en: 'You can draw your own skin.', es: 'Puedes dibujar tu propia skin.' }] }],
};
const updated = { release: true, whatsNewSeen: '0.0.9' } as const;
const SETTLE = 20;
/** Long enough for the check and its settle wait to have run. */
const settled = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, SETTLE + 80)));

it('opens once the first screen has settled, and marks the notes seen', async () => {
  const { user, calls, backend } = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, updated);
  expect(await screen.findByRole('dialog', { name: "What's new" })).toBeInTheDocument();
  expect(screen.getByText('You can draw your own skin.')).toBeInTheDocument();
  expect(sent(calls, 'whats_new_mark')).toHaveLength(1);
  expect(await backend.handle('whats_new_seen')).toBe('0.1.0');
  await user.click(screen.getByRole('button', { name: 'Got it' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('waits for the next launch when another dialog is open', async () => {
  const other = document.createElement('dialog');
  other.setAttribute('open', '');
  document.body.append(other);
  try {
    const { calls, backend } = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, updated);
    await settled();
    expect(screen.queryByRole('dialog', { name: "What's new" })).not.toBeInTheDocument();
    expect(sent(calls, 'whats_new_mark')).toHaveLength(0);
    expect(await backend.handle('whats_new_seen')).toBe('0.0.9');
  } finally {
    other.remove();
  }
});

it('waits for the next launch when a game is starting', async () => {
  const { calls } = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, updated);
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'whats_new_mark')).toHaveLength(0);
});

it('counts a fresh install as having seen the running version, and opens nothing', async () => {
  const { calls } = renderApp(<WhatsNewOnLaunch firstRunDone={false} notes={notes} settleMs={SETTLE} />, { ...updated, whatsNewSeen: null });
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'whats_new_mark')).toHaveLength(1);
});

it('does nothing in a dev build', async () => {
  const { calls } = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, { whatsNewSeen: '0.0.9' });
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'whats_new_mark')).toHaveLength(0);
});

it('checks once per launch', async () => {
  const { user, calls, rerender, qc } = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, updated);
  await user.click(await screen.findByRole('button', { name: 'Got it' }));
  rerender(
    <QueryClientProvider client={qc}>
      <WhatsNewOnLaunch firstRunDone notes={{ ...notes }} settleMs={SETTLE} />
    </QueryClientProvider>,
  );
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'whats_new_mark')).toHaveLength(1);
});
