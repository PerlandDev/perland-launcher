import { QueryClientProvider } from '@tanstack/react-query';
import { act, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import type { ReleaseNotes } from '../../ipc/schema';
import { navigate } from '../../state/route';
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

it('waits for the next launch when a link opened another screen', async () => {
  const { calls } = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, updated);
  // What a perland:// link does on launch: a screen, not a dialog.
  act(() => navigate({ name: 'joinServer', invite: 'asalith' }));
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'whats_new_mark')).toHaveLength(0);
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

// The note that tells players about anonymous data shipped in 0.0.9, which this player's marker says they have seen.
const withPrivacy: ReleaseNotes = {
  versions: ['0.0.8', '0.0.9', '0.1.0'],
  groups: [
    notes.groups[0]!,
    {
      base: '0.0.9',
      date: '2026-11-12',
      notes: [
        { id: 'anonymous-data', kind: 'new', since: '0.0.9', en: 'The launcher now sends anonymous error reports.', es: 'El launcher ahora envía informes anónimos.' },
        { id: 'other', kind: 'fixed', since: '0.0.9', en: 'An old fix.', es: 'Un arreglo viejo.' },
      ],
    },
  ],
};

it('shows the anonymous data note until the player has seen it, whatever the marker says', async () => {
  // Up to date and nothing new: only the notice opens the dialog.
  const { calls, backend } = renderApp(<WhatsNewOnLaunch firstRunDone notes={withPrivacy} settleMs={SETTLE} />, {
    release: true,
    whatsNewSeen: '0.1.0',
    telemetryNoticeDue: true,
  });
  expect(await screen.findByRole('dialog', { name: "What's new" })).toBeInTheDocument();
  expect(screen.getByText('The launcher now sends anonymous error reports.')).toBeInTheDocument();
  expect(screen.queryByText('An old fix.')).not.toBeInTheDocument();
  expect(sent(calls, 'telemetry_notice_shown')).toHaveLength(1);
  expect(await backend.handle('telemetry_notice_due')).toBe(false);
  // The marker was already there: it is not moved again.
  expect(sent(calls, 'whats_new_mark')).toHaveLength(0);
});

it('says the notice was shown when the usual rule shows the note, and adds it to other news when it would not', async () => {
  const usual = renderApp(<WhatsNewOnLaunch firstRunDone notes={withPrivacy} settleMs={SETTLE} />, {
    release: true,
    whatsNewSeen: '0.0.8',
    telemetryNoticeDue: true,
  });
  expect(await screen.findByText('The launcher now sends anonymous error reports.')).toBeInTheDocument();
  expect(screen.getByText('An old fix.')).toBeInTheDocument();
  expect(sent(usual.calls, 'telemetry_notice_shown')).toHaveLength(1);
  usual.unmount();

  const added = renderApp(<WhatsNewOnLaunch firstRunDone notes={withPrivacy} settleMs={SETTLE} />, { ...updated, telemetryNoticeDue: true });
  expect(await screen.findByText('You can draw your own skin.')).toBeInTheDocument();
  expect(screen.getByText('The launcher now sends anonymous error reports.')).toBeInTheDocument();
  expect(sent(added.calls, 'telemetry_notice_shown')).toHaveLength(1);
  expect(sent(added.calls, 'whats_new_mark')).toHaveLength(1);
});

it('does not say the notice was shown when nothing opened or the note was not in it', async () => {
  // Another screen took the launch: nothing opens, so the notice is still due at the next start.
  const blocked = renderApp(<WhatsNewOnLaunch firstRunDone notes={withPrivacy} settleMs={SETTLE} />, {
    release: true,
    whatsNewSeen: '0.1.0',
    telemetryNoticeDue: true,
  });
  act(() => navigate({ name: 'joinServer', invite: 'asalith' }));
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(blocked.calls, 'telemetry_notice_shown')).toHaveLength(0);
  blocked.unmount();
  act(() => navigate({ name: 'home', id: null }));

  // Not due (already shown, or sharing is off): the usual dialog, and nothing is said.
  const usual = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, updated);
  expect(await screen.findByText('You can draw your own skin.')).toBeInTheDocument();
  expect(sent(usual.calls, 'telemetry_notice_due')).toHaveLength(1);
  expect(sent(usual.calls, 'telemetry_notice_shown')).toHaveLength(0);
  usual.unmount();

  // A build whose notes don't carry the note as released: nothing is forced.
  const without = renderApp(<WhatsNewOnLaunch firstRunDone notes={notes} settleMs={SETTLE} />, {
    release: true,
    whatsNewSeen: '0.1.0',
    telemetryNoticeDue: true,
  });
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(without.calls, 'telemetry_notice_shown')).toHaveLength(0);
});

it('asks nothing about the notice on a fresh install or in a dev build', async () => {
  const fresh = renderApp(<WhatsNewOnLaunch firstRunDone={false} notes={withPrivacy} settleMs={SETTLE} />, {
    release: true,
    whatsNewSeen: null,
    telemetryNoticeDue: true,
  });
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(fresh.calls, 'telemetry_notice_due')).toHaveLength(0);
  fresh.unmount();
  const dev = renderApp(<WhatsNewOnLaunch firstRunDone notes={withPrivacy} settleMs={SETTLE} />, { whatsNewSeen: '0.0.9', telemetryNoticeDue: true });
  await settled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(dev.calls, 'telemetry_notice_due')).toHaveLength(0);
});
