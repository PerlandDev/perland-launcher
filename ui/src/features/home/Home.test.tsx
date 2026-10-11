import { act, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import packStatusFx from '../../ipc/contract/pack_status.json';
import type { PackStatus } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Home } from './Home';

describe('Home', () => {
  it('shows the selected instance with its versions and plays it', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    expect(await screen.findByRole('heading', { name: 'Asalith Fields' })).toBeInTheDocument();
    expect(screen.getByText('Minecraft 1.20.1 · Fabric 0.16.14')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(sent(calls, 'play')).toEqual([{ id: 'asalith-fields' }]);
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
    expect(sent(calls, 'play')).toEqual([{ id: 'asalith-fields' }]);
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
});
