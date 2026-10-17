import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ServersScreen } from '../servers-list/ServersScreen';

const yours = () => screen.findByRole('region', { name: 'Your servers' });
const cardIn = async (section: HTMLElement, name: string) => (await within(section).findByRole('heading', { name })).closest('article')!;

const openDelete = async (user: ReturnType<typeof renderApp>['user']) => {
  const card = await cardIn(await yours(), 'Weekend SMP');
  await user.click(within(card).getByRole('button', { name: 'More for Weekend SMP' }));
  await user.click(screen.getByRole('menuitem', { name: 'Delete server' }));
  return screen.getByRole('dialog', { name: 'Delete Weekend SMP?' });
};

describe('DeleteServerDialog', () => {
  it('delete_confirms_then_deletes', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const dialog = await openDelete(user);
    expect(within(dialog).getByText('The world is deleted too. You can create a new server afterwards.')).toBeInTheDocument();
    // Nothing is deleted before the confirmation.
    expect(sent(calls, 'free_server_delete')).toEqual([]);
    await user.click(within(dialog).getByRole('button', { name: 'Delete server' }));
    await vi.waitFor(() => expect(sent(calls, 'free_server_delete')).toHaveLength(1));
    const section = await yours();
    await vi.waitFor(() => expect(within(section).queryByRole('heading', { name: 'Weekend SMP' })).toBeNull());
    expect(screen.queryByRole('dialog')).toBeNull();
    // The entry comes back.
    expect(await within(section).findByRole('button', { name: 'Create a server for friends' })).toBeInTheDocument();
  });

  it('cancel deletes nothing', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const dialog = await openDelete(user);
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(sent(calls, 'free_server_delete')).toEqual([]);
    expect(within(await yours()).getByRole('heading', { name: 'Weekend SMP' })).toBeInTheDocument();
  });

  it('a failed delete stays in the dialog with the error', async () => {
    const backend = createMockBackend({ perland: 'signedIn', free: 'has' });
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) =>
        cmd === 'free_server_delete'
          ? Promise.reject({ category: 'network', code: 'host_unreachable', message: 'x', fix: null, detail: null, params: { host: 'play.example.com' } })
          : backend.handle(cmd, args),
    });
    const dialog = await openDelete(user);
    await user.click(within(dialog).getByRole('button', { name: 'Delete server' }));
    expect(await within(dialog).findByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: 'Delete Weekend SMP?' })).toBeInTheDocument();
  });

  it('confirms in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await i18n.changeLanguage('es');
    const { user } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const card = await cardIn(await screen.findByRole('region', { name: 'Tus servidores' }), 'Weekend SMP');
    await user.click(within(card).getByRole('button', { name: 'Más opciones para Weekend SMP' }));
    await user.click(screen.getByRole('menuitem', { name: 'Eliminar servidor' }));
    const dialog = screen.getByRole('dialog', { name: '¿Eliminar Weekend SMP?' });
    expect(within(dialog).getByText('El mundo también se elimina. Después puedes crear un servidor nuevo.')).toBeInTheDocument();
  });
});
