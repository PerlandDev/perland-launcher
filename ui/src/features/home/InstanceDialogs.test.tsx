import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { inviteOf } from '../../ipc/mockServers';
import type { Instance } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { DeleteDialog } from './InstanceDialogs';

/** The mock's first instance, made the one that joins the invited server Friends (invite `I`). */
function setup(options: MockOptions, invite = inviteOf('I')) {
  const backend = createMockBackend(options);
  const base = backend.state.listing.instances[0]!;
  const instance: Instance = { ...base, source: { kind: 'perland', invite, engine: 'https://api.example.com/' } };
  backend.state.listing.instances = backend.state.listing.instances.map((i) => (i.id === base.id ? instance : i));
  const calls: [string, unknown][] = [];
  const onClose = vi.fn();
  const result = renderWithProviders(<DeleteDialog instance={instance} onClose={onClose} />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  return { ...result, calls, instance, onClose };
}

const dialog = () => screen.findByRole('dialog');
const ALSO = 'Also remove it from Your servers';

describe('DeleteDialog', () => {
  it('offers to remove an invited server too, on by default, and does both', async () => {
    const { user, calls, instance, onClose } = setup({ perland: 'signedIn' });
    const toggle = await within(await dialog()).findByRole('switch', { name: ALSO });
    expect(toggle).toBeChecked();
    await user.click(within(await dialog()).getByRole('button', { name: 'Delete instance' }));
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(sent(calls, 'instance_delete')).toEqual([{ id: instance.id }]);
    expect(sent(calls, 'your_servers_forget')).toEqual([{ invite: inviteOf('I') }]);
  });

  it('turned off, only the instance goes', async () => {
    const { user, calls, onClose } = setup({ perland: 'signedIn' });
    await user.click(await within(await dialog()).findByRole('switch', { name: ALSO }));
    await user.click(within(await dialog()).getByRole('button', { name: 'Delete instance' }));
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(sent(calls, 'your_servers_forget')).toEqual([]);
  });

  it('never offers it for an owned or followed server, or signed out', async () => {
    for (const [options, invite] of [
      [{ perland: 'signedIn' }, inviteOf('O')],
      [{ perland: 'signedIn' }, inviteOf('K')],
      [{ perland: 'signedOut' }, inviteOf('I')],
    ] as const) {
      const { calls, unmount } = setup(options, invite);
      await within(await dialog()).findByRole('button', { name: 'Delete instance' });
      await vi.waitFor(() => expect(sent(calls, 'perland_status').length).toBeGreaterThan(0));
      await new Promise((r) => setTimeout(r, 20));
      expect(screen.queryByRole('switch', { name: ALSO })).toBeNull();
      unmount();
    }
  });
});
