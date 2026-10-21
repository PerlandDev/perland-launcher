import { act, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { runStore } from '../../state/runStore';
import { toastsSnapshot } from '../../state/toasts';
import { createMockBackend } from '../../ipc/mock';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Home } from '../home/Home';

const openShare = async (user: ReturnType<typeof renderApp>['user'], name = 'Creative Flat') => {
  await screen.findByRole('heading', { name });
  await user.click(screen.getByRole('button', { name: 'More options' }));
  await user.click(screen.getByRole('menuitem', { name: 'Share…' }));
  return screen.getByRole('dialog', { name: `Share ${name}` });
};

const toggle = (dialog: HTMLElement, name: string) => within(dialog).getByRole('switch', { name });

describe('ShareDialog', () => {
  it('lists what goes in, with worlds off', async () => {
    const { user } = renderApp(<Home id="creative-flat" />);
    const dialog = await openShare(user);
    expect(await within(dialog).findByText('14 mods')).toBeInTheDocument();
    expect(within(dialog).getByText('Game and mod settings')).toBeInTheDocument();
    expect(within(dialog).getByText('2 resource packs')).toBeInTheDocument();
    expect(within(dialog).getByText('1 shader pack')).toBeInTheDocument();
    expect(within(dialog).getByText('2 worlds, 340 MB')).toBeInTheDocument();
    expect(toggle(dialog, 'Mods')).toHaveAttribute('aria-checked', 'true');
    expect(toggle(dialog, 'Settings')).toHaveAttribute('aria-checked', 'true');
    expect(toggle(dialog, 'Worlds')).toHaveAttribute('aria-checked', 'false');
  });

  it('says how many mods go inside the file before saving', async () => {
    const { user } = renderApp(<Home id="creative-flat" />);
    const dialog = await openShare(user);
    expect(await within(dialog).findByText("12 mods download from Modrinth. 2 aren't on Modrinth, so they go inside the file.")).toBeInTheDocument();
    // Embedded mods, settings and one resource pack, without worlds.
    expect(within(dialog).getByText('About 9.9 MB.')).toBeInTheDocument();
    await user.click(toggle(dialog, 'Worlds'));
    expect(within(dialog).getByText('About 350 MB.')).toBeInTheDocument();
    await user.click(toggle(dialog, 'Mods'));
    expect(within(dialog).queryByText(/download from Modrinth/)).toBeNull();
  });

  it('says so when every mod is on Modrinth', async () => {
    const { user } = renderApp(<Home id="creative-flat" />, { share: 'all' });
    const dialog = await openShare(user);
    expect(await within(dialog).findByText('All mods download from Modrinth, so the file stays small.')).toBeInTheDocument();
  });

  it('saves the file with the choice and offers Show in folder', async () => {
    const { user, calls } = renderApp(<Home id="creative-flat" />);
    const dialog = await openShare(user);
    await within(dialog).findByText('14 mods');
    await user.click(toggle(dialog, 'Shader packs'));
    await user.click(within(dialog).getByRole('button', { name: 'Save file…' }));
    await vi.waitFor(() =>
      expect(sent(calls, 'share_export')).toEqual([
        {
          id: 'creative-flat',
          choice: { mods: true, settings: true, resourcePacks: true, shaderPacks: false, worlds: false },
          path: 'C:/Users/player/Downloads/Creative Flat.mrpack',
        },
      ]),
    );
    await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    const notice = toastsSnapshot().at(-1)?.notice;
    expect(notice?.title).toBe('Saved. Your friends open it in Perland Launcher.');
    act(() => notice?.action?.run());
    await vi.waitFor(() => expect(sent(calls, 'share_reveal')).toHaveLength(1));
  });

  it('writes nothing when the save dialog is cancelled', async () => {
    const { user, calls } = renderApp(<Home id="creative-flat" />, { savePath: null });
    const dialog = await openShare(user);
    await within(dialog).findByText('14 mods');
    await user.click(within(dialog).getByRole('button', { name: 'Save file…' }));
    await vi.waitFor(() => expect(sent(calls, 'plugin:dialog|save')).toHaveLength(1));
    expect(sent(calls, 'share_export')).toEqual([]);
    expect(screen.getByRole('dialog', { name: 'Share Creative Flat' })).toBeInTheDocument();
  });

  it('needs at least one thing turned on', async () => {
    const { user } = renderApp(<Home id="creative-flat" />);
    const dialog = await openShare(user);
    await within(dialog).findByText('14 mods');
    for (const name of ['Mods', 'Settings', 'Resource packs', 'Shader packs']) await user.click(toggle(dialog, name));
    expect(within(dialog).getByText('Turn on at least one thing to share.')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Save file…' })).toHaveAttribute('aria-disabled', 'true');
  });

  it('shows a failed lookup with Try again and no save', async () => {
    const { user, calls } = renderApp(<Home id="creative-flat" />, { share: 'error' });
    const dialog = await openShare(user);
    expect(await within(dialog).findByRole('alert')).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: 'Save file…' })).toBeNull();
    await user.click(within(dialog).getByRole('button', { name: 'Try again' }));
    await vi.waitFor(() => expect(sent(calls, 'share_preview')).toHaveLength(2));
  });

  it('is not offered for a server instance', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Survival' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.queryByRole('menuitem', { name: 'Share…' })).toBeNull();
  });

  it('is off while the game runs', async () => {
    const { user } = renderApp(<Home id="creative-flat" />);
    await screen.findByRole('heading', { name: 'Creative Flat' });
    act(() => runStore.apply({ type: 'run_state', instance: 'creative-flat', state: 'running', exit: null, update: null }));
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.getByRole('menuitem', { name: 'Share…' })).toBeDisabled();
  });

  it('refuses more than one file can hold before the save dialog', async () => {
    const backend = createMockBackend();
    const { user } = renderWithProviders(<Home id="creative-flat" />, {
      ipc: async (cmd, args) => {
        if (cmd !== 'share_preview') return backend.handle(cmd, args);
        const preview = (await backend.handle(cmd, args)) as { counts: { category: string; embeddedBytes: number }[] };
        for (const c of preview.counts) if (c.category === 'worlds') c.embeddedBytes = 9 * 1024 ** 3;
        return preview;
      },
    });
    const dialog = await openShare(user);
    await within(dialog).findByText('14 mods');
    await user.click(toggle(dialog, 'Worlds'));
    expect(within(dialog).getByText('This is too big to share as one file. Leave out worlds.')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Save file…' })).toHaveAttribute('aria-disabled', 'true');
  });

  it('stays open while the file is written', async () => {
    const backend = createMockBackend();
    const { user } = renderWithProviders(<Home id="creative-flat" />, {
      ipc: (cmd, args) => (cmd === 'share_export' ? new Promise(() => {}) : backend.handle(cmd, args)),
    });
    const dialog = await openShare(user);
    await within(dialog).findByText('14 mods');
    await user.click(within(dialog).getByRole('button', { name: 'Save file…' }));
    expect(await within(dialog).findByRole('button', { name: 'Saving…' })).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await user.keyboard('{Escape}');
    expect(screen.getByRole('dialog', { name: 'Share Creative Flat' })).toBeInTheDocument();
  });
});
