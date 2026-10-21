import { act, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import type { MockOptions } from '../../ipc/mock';
import { runStore } from '../../state/runStore';
import { createMockBackend } from '../../ipc/mock';
import type { FreeServerPage, InstanceListing } from '../../ipc/schema';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders, type IpcHandler } from '../../test/render';
import { Home } from '../home/Home';

const ITEM = 'Play this with friends…';

afterEach(async () => {
  await i18n.changeLanguage('en');
});

async function menu(options: MockOptions = { perland: 'signedIn', free: 'none' }, id = 'asalith-fields', name = 'Asalith Fields') {
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
    box: screen.getByRole('dialog', { name: 'Play Asalith Fields with friends' }),
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
    expect(sent(calls, 'friends_create')).toEqual([{ id: 'asalith-fields', name: 'Steve', settings: null, language: 'en' }]);
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
    const app = renderApp(<Home id="asalith-fields" />, {
      perland: 'signedIn',
      free: 'none',
      friends: 'missing',
    });
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    await app.user.click(screen.getByRole('button', { name: 'Más opciones' }));
    await app.user.click(await screen.findByRole('menuitem', { name: 'Jugar esto con amigos…' }));
    const box = screen.getByRole('dialog', {
      name: 'Juega Asalith Fields con amigos',
    });
    expect(
      await within(box).findByText(
        'Los servidores gratis solo usan mods de Modrinth. 2 de tus mods no están ahí: create-tweaks-1.2.jar, my-own-mod.jar.',
      ),
    ).toBeInTheDocument();
  });

  it('is not offered with a free server already', async () => {
    await menu({ perland: 'signedIn', free: 'has' });
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is not offered while signed out', async () => {
    await menu({ perland: 'signedOut', free: 'none' });
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is off while the game runs', async () => {
    const app = renderApp(<Home id="asalith-fields" />, {
      perland: 'signedIn',
      free: 'none',
    });
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    act(() =>
      runStore.apply({
        type: 'run_state',
        instance: 'asalith-fields',
        state: 'running',
        exit: null,
        update: null,
      }),
    );
    await app.user.click(screen.getByRole('button', { name: 'More options' }));
    expect(await screen.findByRole('menuitem', { name: ITEM })).toBeDisabled();
  });
});

/** Home on instance `id`, on the mock backend with `override` answering first (undefined falls through). */
async function homeWith(
  id: string,
  heading: string,
  override: (cmd: string, args: unknown, backend: ReturnType<typeof createMockBackend>) => unknown = () => undefined,
  options: MockOptions = { perland: 'signedIn', free: 'none' },
) {
  const backend = createMockBackend(options);
  const calls: [string, unknown][] = [];
  const ipc: IpcHandler = (cmd, args) => {
    calls.push([cmd, args]);
    const answer = override(cmd, args, backend);
    return answer === undefined ? backend.handle(cmd, args) : answer;
  };
  const app = renderWithProviders(<Home id={id} />, { ipc });
  await screen.findByRole('heading', { name: heading });
  await app.user.click(screen.getByRole('button', { name: 'More options' }));
  return { ...app, calls };
}

/** An Engine whose limits leave out `pack` and/or `settings`. */
const engineWithout =
  (...keys: ('pack' | 'settings')[]) =>
  (cmd: string, args: unknown, backend: ReturnType<typeof createMockBackend>) => {
    if (cmd !== 'free_server') return undefined;
    return Promise.resolve(backend.handle(cmd, args)).then((p) => {
      const page = structuredClone(p) as FreeServerPage;
      for (const key of keys) page.limits[key] = null;
      return page;
    });
  };

/** Creative Flat (vanilla 26.3) on another Minecraft version. */
const flatOn = (minecraft: string) => (cmd: string, args: unknown, backend: ReturnType<typeof createMockBackend>) => {
  if (cmd !== 'instances_list') return undefined;
  return Promise.resolve(backend.handle(cmd, args)).then((l) => {
    const listing = structuredClone(l) as InstanceListing;
    return {
      ...listing,
      instances: listing.instances.map((i) => (i.id === 'creative-flat' ? { ...i, game: { ...i.game, minecraft } } : i)),
    };
  });
};

describe('Play this with friends, by instance kind', () => {
  it('is offered on a vanilla instance when the Engine checks the version', async () => {
    await homeWith('creative-flat', 'Creative Flat');
    expect(await screen.findByRole('menuitem', { name: ITEM })).toBeInTheDocument();
  });

  it('is not offered on a vanilla instance by an Engine without settings', async () => {
    await homeWith('creative-flat', 'Creative Flat', engineWithout('settings'));
    expect(await screen.findByRole('menuitem', { name: 'Rename' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is always offered on a Modrinth modpack instance', async () => {
    await homeWith('cozy-pack', 'Cozy Pack', engineWithout('pack', 'settings'));
    expect(await screen.findByRole('menuitem', { name: ITEM })).toBeInTheDocument();
  });

  it('needs pack uploads on any other modded instance', async () => {
    await homeWith('asalith-fields', 'Asalith Fields', engineWithout('pack'));
    expect(await screen.findByRole('menuitem', { name: 'Rename' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('is never offered on a server instance', async () => {
    await homeWith('survival', 'Survival', undefined, { perland: 'signedIn', free: 'none', server: 'online' });
    expect(await screen.findByRole('menuitem', { name: 'Rename' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: ITEM })).toBeNull();
  });

  it('vanilla: the version as text, no mods check, Options, and Create with the settings', async () => {
    const { user, calls } = await homeWith('creative-flat', 'Creative Flat');
    await user.click(await screen.findByRole('menuitem', { name: ITEM }));
    const box = screen.getByRole('dialog', { name: 'Play Creative Flat with friends' });
    expect(within(box).getByText('Your free server runs Minecraft 26.3, the same as this instance. Friends join on that version.')).toBeInTheDocument();
    expect(within(box).queryByRole('combobox', { name: 'Minecraft version' })).toBeNull();
    expect(within(box).queryByText('Checking your mods…')).toBeNull();
    await user.click(within(box).getByRole('button', { name: /^Options/ }));
    await user.click(within(box).getByRole('radio', { name: 'Creative' }));
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    await vi.waitFor(() => expect(sent(calls, 'friends_create')).toHaveLength(1));
    expect(sent(calls, 'friends_create')).toEqual([{ id: 'creative-flat', name: 'Steve', settings: { gameMode: 'creative' }, language: 'en' }]);
    expect(sent(calls, 'friends_check')).toEqual([]);
  });

  it('Modrinth modpack: runs the pack as published, no mods check', async () => {
    const { user, calls } = await homeWith('cozy-pack', 'Cozy Pack');
    await user.click(await screen.findByRole('menuitem', { name: ITEM }));
    const box = screen.getByRole('dialog', { name: 'Play Cozy Pack with friends' });
    expect(within(box).getByText('Runs Cozy Pack as published on Modrinth. Mods you added yourself stay on your computer.')).toBeInTheDocument();
    expect(within(box).getByText('Minecraft 1.20.1')).toBeInTheDocument();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    await vi.waitFor(() => expect(sent(calls, 'friends_create')).toHaveLength(1));
    expect(sent(calls, 'friends_create')).toEqual([{ id: 'cozy-pack', name: 'Steve', settings: null, language: 'en' }]);
    expect(sent(calls, 'friends_check')).toEqual([]);
  });

  it('an old beta vanilla instance: says free servers cannot run it, and has no Create', async () => {
    const { user, calls } = await homeWith('creative-flat', 'Creative Flat', flatOn('b1.7.3'));
    await user.click(await screen.findByRole('menuitem', { name: ITEM }));
    const box = screen.getByRole('dialog', { name: 'Play Creative Flat with friends' });
    expect(await within(box).findByText("Free servers can't run Minecraft b1.7.3.")).toBeInTheDocument();
    expect(within(box).queryByRole('button', { name: 'Create' })).toBeNull();
    expect(within(box).queryByLabelText('Server name')).toBeNull();
    expect(within(box).getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(sent(calls, 'friends_create')).toEqual([]);
  });

  it('a refused version is said with the instance version, since nothing is picked here', async () => {
    const refuse = (cmd: string) =>
      cmd === 'friends_create'
        ? Promise.reject({ category: 'network', code: 'invalid_free_server', message: 'x', fix: null, detail: null, params: { field: 'version' } })
        : undefined;
    const { user } = await homeWith('creative-flat', 'Creative Flat', refuse);
    await user.click(await screen.findByRole('menuitem', { name: ITEM }));
    const box = screen.getByRole('dialog', { name: 'Play Creative Flat with friends' });
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByRole('alert')).toHaveTextContent("Free servers can't run Minecraft 26.3.");
  });

  it('speaks Spanish on a Modrinth modpack', async () => {
    await i18n.changeLanguage('es');
    const app = renderApp(<Home id="cozy-pack" />, { perland: 'signedIn', free: 'none' });
    await screen.findByRole('heading', { name: 'Cozy Pack' });
    await app.user.click(screen.getByRole('button', { name: 'Más opciones' }));
    await app.user.click(await screen.findByRole('menuitem', { name: 'Jugar esto con amigos…' }));
    const box = screen.getByRole('dialog', { name: 'Juega Cozy Pack con amigos' });
    expect(
      within(box).getByText('Usa Cozy Pack tal como está publicado en Modrinth. Los mods que agregaste tú se quedan en tu computadora.'),
    ).toBeInTheDocument();
  });
});
