import { act, render, screen, within } from '@testing-library/react';
import { Marquee } from '../../art/Marquee';
import { describe, expect, it, vi } from 'vitest';
import packStatusFx from '../../ipc/contract/pack_status.json';
import { createMockBackend, exitOf } from '../../ipc/mock';
import type { PackStatus } from '../../ipc/schema';
import { lastInstance, navigate, routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import App from '../../App';
import { Home } from './Home';

describe('Home', () => {
  it('shows the selected instance with its versions and plays it', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    expect(await screen.findByRole('heading', { name: 'Asalith Fields' })).toBeInTheDocument();
    expect(screen.getByText('Minecraft 1.20.1 · Fabric 0.16.14')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(sent(calls, 'play')).toEqual([{ id: 'asalith-fields', join: true }]);
  });

  it('sends one play for a double click', async () => {
    const { user, calls, backend } = renderApp(<Home id="asalith-fields" />);
    const handle = backend.handle;
    let release = () => {};
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => {
      if (cmd !== 'play') return handle(cmd, args);
      return new Promise((resolve) => (release = () => resolve(handle(cmd, args))));
    });
    await user.dblClick(await screen.findByRole('button', { name: 'Play' }));
    expect(sent(calls, 'play')).toEqual([{ id: 'asalith-fields', join: true }]);
    await act(async () => release());
    expect(sent(calls, 'play')).toHaveLength(1);
  });

  it('shows progress while preparing and stops while running', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'preparing', exit: null, update: null });
      runStore.apply({
        type: 'progress',
        instance: 'asalith-fields',
        task: { key: 'install_minecraft', params: { version: '1.20.1' } },
        done: 3,
        total: 6,
        bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Installing Minecraft 1.20.1' })).toBeInTheDocument();
    act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(sent(calls, 'stop')).toEqual([{ id: 'asalith-fields' }]);
  });

  it('shows the empty state for a fresh install', async () => {
    renderApp(<Home id="creative-flat" />, { firstRun: true });
    expect(await screen.findByRole('heading', { name: 'No instances yet' })).toBeInTheDocument();
  });

  it('disables Play without an account and links to Accounts', async () => {
    const { backend, user } = renderApp(<Home id="creative-flat" />);
    backend.state.accounts.active = null;
    await screen.findByRole('heading', { name: 'Creative Flat' });
    expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
    await user.click(await screen.findByRole('button', { name: 'Add an account to play' }));
    expect(routeSnapshot()).toEqual({ name: 'accounts' });
  });

  it('offers to create the first instance when there are none', async () => {
    const { user } = renderApp(<Home id={null} />, { firstRun: true });
    await user.click(await screen.findByRole('button', { name: 'Create an instance' }));
    expect(routeSnapshot()).toEqual({ name: 'newInstance' });
  });

  it('explains a damaged instance and can delete it', async () => {
    const { user, calls } = renderApp(<Home id="old-pack" />);
    expect(await screen.findByText('The settings file of “old-pack” is damaged.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete “old-pack”?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete' }));
    expect(sent(calls, 'instance_delete')).toEqual([{ id: 'old-pack' }]);
  });

  it('renames from the menu', async () => {
    const { user, calls } = renderApp(<Home id="creative-flat" />);
    await screen.findByRole('heading', { name: 'Creative Flat' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(screen.getByRole('menuitem', { name: 'Rename' }));
    const input = screen.getByRole('textbox', { name: 'Name' });
    await user.clear(input);
    await user.type(input, 'Flat World');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(sent(calls, 'instance_rename')).toEqual([{ id: 'creative-flat', name: 'Flat World' }]);
  });

  it('follows a rename to the new folder id', async () => {
    const { user, calls } = renderApp(<App />);
    await screen.findByRole('navigation', { name: 'Instances' });
    act(() => navigate({ name: 'home', id: 'creative-flat' }));
    await screen.findByRole('heading', { name: 'Creative Flat' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(screen.getByRole('menuitem', { name: 'Rename' }));
    const input = screen.getByRole('textbox', { name: 'Name' });
    await user.clear(input);
    await user.type(input, 'Sky Base');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('heading', { name: 'Sky Base' })).toBeInTheDocument();
    expect(sent(calls, 'instance_rename')).toEqual([{ id: 'creative-flat', name: 'Sky Base' }]);
    expect(routeSnapshot()).toEqual({ name: 'home', id: 'sky-base' });
    expect(lastInstance()).toBe('sky-base');
  });

  const offer = (packStatusFx as PackStatus[])[0]!.update!;

  it('asks before updating and sends the answer', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'asking', exit: null, update: offer }));
    const dialog = screen.getByRole('dialog', { name: 'Update ready: 1.0.3 → 1.0.4' });
    expect(within(dialog).getByText('+3 added · 2 updated · 1 removed')).toBeInTheDocument();
    expect(within(dialog).getByText('Kept your version of 1 file')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Show what changed' }));
    expect(within(dialog).getByText("Just Enough Items, Create, Farmer's Delight")).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Update and play' }));
    expect(sent(calls, 'play_answer')).toEqual([{ id: 'sky-islands', update: true }]);
  });

  it('plays the installed version, or cancels with Escape', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'asking', exit: null, update: offer }));
    await user.click(screen.getByRole('button', { name: 'Play 1.0.3' }));
    expect(sent(calls, 'play_answer')).toEqual([{ id: 'sky-islands', update: false }]);
    // jsdom doesn't turn Escape into the dialog's cancel event; dispatch it as the browser would.
    act(() => {
      screen.getByRole('dialog').dispatchEvent(new Event('cancel', { cancelable: true }));
    });
    expect(sent(calls, 'stop')).toEqual([{ id: 'sky-islands' }]);
  });

  it('shows checking and installing on the Play button', async () => {
    renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'checking', exit: null, update: null }));
    expect(screen.getByRole('button', { name: 'Cancel: Checking for updates…' })).toBeInTheDocument();
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'syncing', exit: null, update: null });
      runStore.apply({
        type: 'progress', instance: 'sky-islands',
        task: { key: 'install_pack', params: { name: 'Sky Islands' } }, done: 2, total: 24, bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Installing Sky Islands' })).toBeInTheDocument();
  });

  it('says the pack files are being checked before an update starts', async () => {
    renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'syncing', exit: null, update: null });
      runStore.apply({
        type: 'progress', instance: 'sky-islands',
        task: { key: 'check_pack', params: { name: 'Sky Islands' } }, done: 3, total: 88, bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Checking Sky Islands files' })).toBeInTheDocument();
  });

  it('offers the update between plays and runs it on Update', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    expect(await screen.findByText('Update ready: 1.0.3 → 1.0.4')).toBeInTheDocument();
    expect(screen.getByText('+3 added · 2 updated · 1 removed')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Update' }));
    expect(sent(calls, 'pack_update')).toEqual([{ id: 'sky-islands' }]);
    expect(screen.getByRole('button', { name: 'Play' })).toHaveClass('play'); // Play stays the lit one
  });

  it('says so quietly when it could not check', async () => {
    const { backend } = renderApp(<Home id="sky-islands" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation(async (cmd, args) =>
      cmd === 'pack_status'
        ? { installedVersion: '1.0.3', update: null, checkError: { category: 'network', code: 'update_check_failed', message: 'x', fix: null, detail: null, params: {} } }
        : handle(cmd, args),
    );
    expect(await screen.findByText("Couldn't check for updates")).toBeInTheDocument();
  });

  it('tells what an update changed after the run', async () => {
    renderApp(<Home id="sky-islands" />, { noUpdate: true });
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() =>
      runStore.apply({
        type: 'run_state', instance: 'sky-islands', state: 'idle', update: null,
        exit: exitOf({ updateOnly: true, synced: { fromVersion: '1.0.3', toVersion: '1.0.4', added: 3, updated: 2, removed: 1, keptModified: ['options.txt', 'config/a.toml'] } }),
      }),
    );
    expect(screen.getByText('Updated to 1.0.4 · Kept your version of 2 files')).toBeInTheDocument();
  });

  it('never asks a vanilla instance about updates', async () => {
    const { calls } = renderApp(<Home id="creative-flat" />);
    await screen.findByRole('heading', { name: 'Creative Flat' });
    expect(sent(calls, 'pack_status')).toEqual([]);
  });

  it('checks for updates from the menu', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    await screen.findByText('Update ready: 1.0.3 → 1.0.4');
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(screen.getByRole('menuitem', { name: 'Check for updates' }));
    await vi.waitFor(() => expect(sent(calls, 'pack_status').length).toBe(2));
  });
});

it("shows the pack's own banner, and generated art otherwise", async () => {
  const { container } = renderApp(<Home id="sky-islands" />);
  await screen.findByRole('heading', { name: 'Sky Islands' });
  await vi.waitFor(() =>
    expect(container.querySelector('img.marquee-banner')?.getAttribute('src')).toMatch(/^data:image\/svg\+xml;base64,/),
  );
});

describe('server instances', () => {
  it('show the live status, the address and the invite-only line', async () => {
    renderApp(<Home id="survival" />, { server: 'whitelisted' });
    expect(await screen.findByRole('status', { name: 'Server status: Online · 3/20' })).toBeInTheDocument();
    expect(screen.getByText('play.example.com:25566')).toBeInTheDocument();
    expect(screen.getByText('Invite only: ask the owner to add Steve')).toBeInTheDocument();
  });

  it.each([
    ['vanilla', 'Sleeping · join to wake it'],
    ['gone', 'Unknown'],
  ] as const)('read %s as %s', async (server, word) => {
    renderApp(<Home id="survival" />, { server });
    expect(await screen.findByRole('status', { name: `Server status: ${word}` })).toBeInTheDocument();
  });

  it('copy the address', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Copy server address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25566');
  });

  it('say Copied only when the copy worked', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    const button = await screen.findByRole('button', { name: 'Copy server address' });
    expect(button).toHaveTextContent('Copy');
    await user.click(button);
    expect(await screen.findByText('Copied')).toBeInTheDocument();
  });

  it('name the button Copied and announce the copy', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Copy server address' }));
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeInTheDocument();
    expect(screen.getByText('Address copied')).toHaveAttribute('aria-live', 'polite');
  });

  it('read a starting server as Starting', async () => {
    renderApp(<Home id="survival" />, { server: 'starting' });
    expect(await screen.findByRole('status', { name: 'Server status: Starting' })).toBeInTheDocument();
  });

  it('keep the label as Copy and toast when the clipboard refuses', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    const failure = new Error('denied');
    const button = await screen.findByRole('button', { name: 'Copy server address' });
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValueOnce(failure);
    await user.click(button);
    await vi.waitFor(() => expect(toastsSnapshot().map((toast) => toast.error)).toEqual([failure]));
    expect(button).toHaveTextContent(/^Copy$/);
    expect(screen.queryByText('Copied')).not.toBeInTheDocument();
  });

  it('offer the server items in the menu', async () => {
    const { user, calls } = renderApp(<Home id="survival" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Survival' });
    const open = async () => user.click(screen.getByRole('button', { name: 'More options' }));
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Play without joining' }));
    expect(sent(calls, 'play')).toEqual([{ id: 'survival', join: false }]);
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Copy invite link' }));
    expect(await navigator.clipboard.readText()).toBe(`https://play.example.com/${'A'.repeat(43)}`);
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Copy server address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25566');
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Use a new invite link' }));
    expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', relinkFor: 'survival' });
  });

  it('toast when the menu cannot copy', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Survival' });
    const failure = new Error('denied');
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValueOnce(failure);
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(screen.getByRole('menuitem', { name: 'Copy invite link' }));
    await vi.waitFor(() => expect(toastsSnapshot().map((toast) => toast.error)).toEqual([failure]));
  });

  it('keep the server items off plain instances', async () => {
    const { user } = renderApp(<Home id="creative-flat" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Creative Flat' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.getByRole('menuitem', { name: 'Instance settings' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Play without joining' })).not.toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Use a new invite link' })).not.toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Copy invite link' })).not.toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Copy server address' })).not.toBeInTheDocument();
  });

  it('raise the dead-link plate, which opens relinking', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'gone' });
    expect(await screen.findByText(/This invite link stopped working/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Use a new link' }));
    expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', relinkFor: 'survival' });
  });

  it('keep a running server on its link: no new link from the plate or the menu', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'gone' });
    await screen.findByText(/This invite link stopped working/);
    act(() => runStore.apply({ type: 'run_state', instance: 'survival', state: 'running', exit: null, update: null }));
    expect(screen.getByRole('button', { name: 'Use a new link' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.getByRole('menuitem', { name: 'Use a new invite link' })).toBeDisabled();
    expect(screen.getByRole('menuitem', { name: 'Copy invite link' })).toBeEnabled();
    await user.keyboard('{Escape}');
    act(() => runStore.apply({ type: 'run_state', instance: 'survival', state: 'idle', exit: null, update: null }));
    expect(screen.getByRole('button', { name: 'Use a new link' })).toBeEnabled();
  });

  it('say a server update happens at Play, with no Update button', async () => {
    const backend = createMockBackend({ server: 'online' });
    renderWithProviders(<Home id="survival" />, {
      ipc: (cmd, args) =>
        cmd === 'pack_status'
          ? {
              installedVersion: '1.0.4',
              update: { fromVersion: '1.0.4', toVersion: '1.0.5', added: [], updated: [], removed: [], kept: [] },
              checkError: null,
            }
          : backend.handle(cmd, args),
    });
    expect(await screen.findByText('Server updated · updates when you play')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Update' })).not.toBeInTheDocument();
  });

  it.each(['checking', 'syncing'] as const)('label the wait while the server builds its pack (%s)', async (state) => {
    renderApp(<Home id="survival" />, { server: 'pending' });
    await screen.findByRole('heading', { name: 'Survival' });
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'survival', state, exit: null, update: null });
      runStore.apply({
        type: 'progress',
        instance: 'survival',
        task: { key: 'server_pack_wait', params: { name: 'survival' } },
        done: 0,
        total: 0,
        bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Server is preparing the modpack…' })).toBeInTheDocument();
  });

  it('never poll a plain instance', async () => {
    const { calls } = renderApp(<Home id="creative-flat" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Creative Flat' });
    expect(sent(calls, 'server_status')).toEqual([]);
  });
});

it("fills the marquee with the player's own picture, which arrives once as the icon", async () => {
  const backend = createMockBackend();
  await backend.handle('instance_art_set', { id: 'asalith-fields', choice: { kind: 'upload', path: 'C:/castle.png' } });
  expect(await backend.handle('instance_art', { id: 'asalith-fields' })).toMatchObject({ banner: null });
  const { container } = renderWithProviders(<Home id="asalith-fields" />, { ipc: (cmd, args) => backend.handle(cmd, args) });
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  await vi.waitFor(() => expect(container.querySelector('img.marquee-banner')?.getAttribute('src')).toMatch(/^data:image\/png;base64,/));
});

it('draws the marquee from the art seed, not the id', async () => {
  const backend = createMockBackend();
  backend.state.listing.instances = backend.state.listing.instances.map((i) =>
    i.id === 'creative-flat' ? { ...i, artSeed: 'old-name', art: 'generated' as const } : i,
  );
  const { container } = renderWithProviders(<Home id="creative-flat" />, { ipc: (cmd, args) => backend.handle(cmd, args) });
  await screen.findByRole('heading', { name: 'Creative Flat' });
  const hills = (root: Element) => [...root.querySelectorAll('.marquee-layer path')].map((p) => p.getAttribute('d'));
  const want = hills(render(<Marquee seed="old-name" attract={false} />).container);
  expect(want).not.toEqual(hills(render(<Marquee seed="creative-flat" attract={false} />).container));
  expect(hills(container.querySelector('section .marquee')!)).toEqual(want);
});
