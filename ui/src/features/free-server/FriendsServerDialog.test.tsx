import { act, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import type { MockOptions } from '../../ipc/mock';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Home } from '../home/Home';

const ITEM = 'Play this with friends…';

afterEach(async () => {
  await i18n.changeLanguage('en');
});

async function menu(options: MockOptions = { perland: 'signedIn', free: 'none' }, id = 'cozy-pack', name = 'Cozy Pack') {
  const app = renderApp(<Home id={id} />, options);
  await screen.findByRole('heading', { name });
  await app.user.click(screen.getByRole('button', { name: 'More options' }));
  return app;
}

async function open(options?: MockOptions) {
  const app = await menu(options);
  await app.user.click(await screen.findByRole('menuitem', { name: ITEM }));
  return {
    ...app,
    box: screen.getByRole('dialog', { name: 'Play Cozy Pack with friends' }),
  };
}

describe('FriendsServerDialog', () => {
  it('shows what the server gets and creates it from the instance', async () => {
    const { box, user, calls } = await open();
    expect(await within(box).findByText('14 mods')).toBeInTheDocument();
    expect(within(box).getByText('Needs 2.5 GB of memory. A free server has 6 GB.')).toBeInTheDocument();
    expect(within(box).getByText('52 mod settings files')).toBeInTheDocument();
    expect(within(box).getByText('Worlds, resource packs and shaders stay on your computer.')).toBeInTheDocument();
    expect(within(box).getByText(/^Up to 10 players\./)).toBeInTheDocument();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(sent(calls, 'friends_create')).toEqual([{ id: 'cozy-pack', name: 'Steve', language: 'en' }]);
    expect(await screen.findByRole('heading', { name: 'Your server is ready' }, { timeout: 4000 })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Play now' })).toBeInTheDocument();
  });

  it('refuses mods that are not on Modrinth before anything is sent', async () => {
    const { box, calls } = await open({
      perland: 'signedIn',
      free: 'none',
      friends: 'missing',
    });
    expect(
      await within(box).findByText(
        "Free servers only run mods from Modrinth. 2 of your mods aren't there: create-tweaks-1.2.jar, my-own-mod.jar.",
      ),
    ).toBeInTheDocument();
    const create = within(box).getByRole('button', { name: 'Create' });
    expect(create).toBeDisabled();
    expect(sent(calls, 'friends_create')).toEqual([]);
  });

  it('refuses mods that need more memory than a free server has', async () => {
    const { box } = await open({
      perland: 'signedIn',
      free: 'none',
      friends: 'heavy',
    });
    expect(await within(box).findByText('Your mods need 7.5 GB of memory. Free servers have 6 GB.')).toBeInTheDocument();
    expect(within(box).getByRole('button', { name: 'Create' })).toBeDisabled();
  });

  it('speaks Spanish', async () => {
    await i18n.changeLanguage('es');
    const app = renderApp(<Home id="cozy-pack" />, {
      perland: 'signedIn',
      free: 'none',
      friends: 'missing',
    });
    await screen.findByRole('heading', { name: 'Cozy Pack' });
    await app.user.click(screen.getByRole('button', { name: 'Más opciones' }));
    await app.user.click(await screen.findByRole('menuitem', { name: 'Jugar esto con amigos…' }));
    const box = screen.getByRole('dialog', {
      name: 'Juega Cozy Pack con amigos',
    });
    expect(
      await within(box).findByText(
        'Los servidores gratis solo usan mods de Modrinth. 2 de tus mods no están ahí: create-tweaks-1.2.jar, my-own-mod.jar.',
      ),
    ).toBeInTheDocument();
  });

  it('is offered only to a signed-in player without a server, on a modded instance', async () => {
    await menu({ perland: 'signedIn', free: 'has' });
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is not offered while signed out', async () => {
    await menu({ perland: 'signedOut', free: 'none' });
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is not offered on a vanilla instance', async () => {
    await menu({ perland: 'signedIn', free: 'none' }, 'creative-flat', 'Creative Flat');
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is off while the game runs', async () => {
    const app = renderApp(<Home id="cozy-pack" />, {
      perland: 'signedIn',
      free: 'none',
    });
    await screen.findByRole('heading', { name: 'Cozy Pack' });
    act(() =>
      runStore.apply({
        type: 'run_state',
        instance: 'cozy-pack',
        state: 'running',
        exit: null,
        update: null,
      }),
    );
    await app.user.click(screen.getByRole('button', { name: 'More options' }));
    expect(await screen.findByRole('menuitem', { name: ITEM })).toBeDisabled();
  });
});
