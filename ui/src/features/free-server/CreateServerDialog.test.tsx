import { act, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { MOCK_FREE_CREATE_MS, MOCK_FREE_INVITE, MOCK_FREE_LIMITS } from '../../ipc/mockFree';
import type { FreeServerPage, InstanceListing } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders, type IpcHandler } from '../../test/render';
import { ServersScreen } from '../servers-list/ServersScreen';

const ENTRY = 'Create a server for friends';

afterEach(async () => {
  vi.useRealTimers();
  await i18n.changeLanguage('en');
});

/** The open dialog. */
const dialog = () => screen.findByRole('dialog');

async function open(options: MockOptions = { perland: 'signedIn', free: 'none' }) {
  const app = renderApp(<ServersScreen />, options);
  await app.user.click(await screen.findByRole('button', { name: ENTRY }));
  return { ...app, box: await dialog() };
}

/** The mock backend, with `override` answering first (undefined falls through). */
function renderWith(override: (cmd: string, args: unknown, backend: ReturnType<typeof createMockBackend>) => unknown) {
  const backend = createMockBackend({ perland: 'signedIn', free: 'none' });
  const calls: [string, unknown][] = [];
  const ipc: IpcHandler = (cmd, args) => {
    calls.push([cmd, args]);
    const answer = override(cmd, args, backend);
    return answer === undefined ? backend.handle(cmd, args) : answer;
  };
  return { ...renderWithProviders(<ServersScreen />, { ipc }), calls, backend };
}

const advance = (ms: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });

describe('CreateServerDialog', () => {
  it('shows the limits and the prefilled name', async () => {
    const { box } = await open();
    expect(within(box).getByRole('heading', { name: 'Create a server for friends' })).toBeInTheDocument();
    expect(within(box).getByText(/^Up to 10 players\. 2 GB of memory for vanilla, 6 GB with mods\./)).toBeInTheDocument();
    expect(within(box).getByLabelText('Server name')).toHaveValue('Steve');
    expect(within(box).getByRole('radio', { name: 'Vanilla' })).toBeChecked();
  });

  it('prefills my-server for an email-only player', async () => {
    const app = renderWith((cmd, _args, backend) => {
      if (cmd !== 'perland_status') return undefined;
      return Promise.resolve(backend.handle(cmd, {})).then((s) => {
        const status = s as {
          account: { displayName: string; name: string } | null;
        };
        return {
          ...status,
          account: status.account && {
            ...status.account,
            name: 'kid@example.com',
            displayName: '',
          },
        };
      });
    });
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    expect(within(await dialog()).getByLabelText('Server name')).toHaveValue('my-server');
  });

  it('vanilla_create_reaches_share', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { user, box, calls } = await open();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(
      await within(box).findByRole('heading', {
        name: 'Creating your server…',
      }),
    ).toBeInTheDocument();
    await advance(MOCK_FREE_CREATE_MS + 100);
    expect(await within(box).findByRole('heading', { name: 'Your server is ready' })).toBeInTheDocument();
    expect(within(box).getByText(MOCK_FREE_INVITE)).toBeInTheDocument();
    expect(sent(calls, 'free_server_create')).toEqual([
      {
        name: 'Steve',
        kind: 'vanilla',
        version: null,
        modrinthVersionId: null,
        settings: null,
        language: 'en',
      },
    ]);
  });

  it('refuses a bad name before sending', async () => {
    const { user, box, calls } = await open();
    const name = within(box).getByLabelText('Server name');
    await user.clear(name);
    await user.type(name, 'my server!');
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(within(box).getByText('Use letters, numbers, - and _ only, up to 32.')).toBeInTheDocument();
    expect(name).toHaveAttribute('aria-invalid', 'true');
    expect(sent(calls, 'free_server_create')).toEqual([]);
  });

  it('modpack_lists_modrinth_instances', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { user, box, calls } = await open();
    await user.click(within(box).getByRole('radio', { name: 'A modpack' }));
    const pick = await within(box).findByRole('combobox', { name: 'Modpack' });
    const options = within(pick)
      .getAllByRole('option')
      .map((o) => o.textContent);
    // Only the instances installed from Modrinth.
    expect(options).toEqual(['Cozy Pack', 'Mega Pack']);
    await user.selectOptions(pick, 'Cozy Pack');
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    await advance(MOCK_FREE_CREATE_MS + 100);
    expect(await within(box).findByRole('heading', { name: 'Your server is ready' })).toBeInTheDocument();
    expect(sent(calls, 'free_server_create')).toEqual([
      {
        name: 'Steve',
        kind: 'modrinth',
        version: null,
        modrinthVersionId: 'kL2pYz4w',
        settings: null,
        language: 'en',
      },
    ]);
  });

  it('no_modpack_instances_points_to_browse', async () => {
    const app = renderWith((cmd, args, backend) => {
      if (cmd !== 'instances_list') return undefined;
      return Promise.resolve(backend.handle(cmd, args)).then((l) => {
        const listing = l as InstanceListing;
        return {
          ...listing,
          instances: listing.instances.filter((i) => i.source.kind !== 'modrinth'),
        };
      });
    });
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('radio', { name: 'A modpack' }));
    expect(await within(box).findByText('Install a modpack from Browse first.')).toBeInTheDocument();
    expect(within(box).getByRole('button', { name: 'Create' })).toBeDisabled();
    await user.click(within(box).getByRole('button', { name: 'Go to Browse' }));
    expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' });
  });

  it('too_heavy_stays_on_step_one_with_gb', async () => {
    const { user, box } = await open();
    await user.click(within(box).getByRole('radio', { name: 'A modpack' }));
    await user.selectOptions(await within(box).findByRole('combobox', { name: 'Modpack' }), 'Mega Pack');
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByRole('alert')).toHaveTextContent('This modpack needs 8 GB; free servers have 6 GB.');
    expect(within(box).getByRole('heading', { name: 'Create a server for friends' })).toBeInTheDocument();
    expect(within(box).getByLabelText('Server name')).toBeInTheDocument();
  });

  it('shows why an unsupported modpack is refused', async () => {
    const app = renderWith((cmd) =>
      cmd === 'free_server_create'
        ? Promise.reject({
            category: 'pack',
            code: 'modpack_unsupported',
            message: 'x',
            fix: null,
            detail: null,
            params: { reason: 'loader' },
          })
        : undefined,
    );
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByRole('alert')).toHaveTextContent("Free servers can't run this modpack's loader.");
  });

  it('lost_answer_polls_until_the_server_appears', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let pages = 0;
    const app = renderWith((cmd, args, backend) => {
      if (cmd === 'free_server_create') {
        return Promise.reject({
          category: 'network',
          code: 'offline',
          message: 'x',
          fix: null,
          detail: null,
          params: { host: 'engine' },
        });
      }
      if (cmd !== 'free_server') return undefined;
      pages += 1;
      return Promise.resolve(backend.handle(cmd, args)).then((p) => {
        const page = p as FreeServerPage;
        // The server shows up from the fourth read on (the first is the entry's own).
        if (pages < 4) return page;
        const server = {
          ...structuredClone(page),
          server: null,
        } as FreeServerPage;
        return {
          ...server,
          create: {
            allowed: false,
            reason: 'free_server_exists',
            retryAfter: null,
          },
          server: { ...fakeCard, inviteUrl: MOCK_FREE_INVITE },
        };
      });
    });
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(
      await within(box).findByRole('heading', {
        name: 'Still creating your server…',
      }),
    ).toBeInTheDocument();
    const before = sent(app.calls, 'free_server').length;
    await advance(5_100);
    expect(sent(app.calls, 'free_server').length).toBeGreaterThan(before);
    await advance(10_100);
    expect(await within(box).findByRole('heading', { name: 'Your server is ready' })).toBeInTheDocument();
    expect(within(box).getByText(MOCK_FREE_INVITE)).toBeInTheDocument();
  });

  it('lost answer gives up after 5 minutes with a notice', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const app = renderWith((cmd) =>
      cmd === 'free_server_create'
        ? Promise.reject({
            category: 'network',
            code: 'free_server_exists',
            message: 'x',
            fix: null,
            detail: null,
            params: {},
          })
        : undefined,
    );
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(
      await within(box).findByRole('heading', {
        name: 'Still creating your server…',
      }),
    ).toBeInTheDocument();
    await advance(5 * 60_000 + 5_100);
    await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(toastsSnapshot().map((toast) => toast.notice?.title)).toContain('Your server is still being set up.');
  });

  it('a language change while waiting keeps the 5-minute window', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const app = renderWith((cmd) =>
      cmd === 'free_server_create'
        ? Promise.reject({ category: 'network', code: 'free_server_exists', message: 'x', fix: null, detail: null, params: {} })
        : undefined,
    );
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByRole('heading', { name: 'Still creating your server…' })).toBeInTheDocument();
    await advance(4 * 60_000);
    await act(async () => {
      await i18n.changeLanguage('es');
    });
    // One minute later the window that started at Create is over, language or not.
    await advance(60_000 + 5_100);
    await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('an invalid_free_server for a fine name is a line, not a name error', async () => {
    const app = renderWith((cmd) =>
      cmd === 'free_server_create'
        ? Promise.reject({
            category: 'network',
            code: 'invalid_free_server',
            message: 'Check the server name: letters, numbers, - and _ only, up to 32.',
            fix: null,
            detail: null,
            params: { host: 'engine' },
          })
        : undefined,
    );
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByRole('alert')).toHaveTextContent("The server couldn't be created with these choices.");
    expect(within(box).getByLabelText('Server name')).not.toHaveAttribute('aria-invalid');
    expect(within(box).queryByText('Use letters, numbers, - and _ only, up to 32.')).toBeNull();
  });

  it('copy_link', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { user, box } = await open();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    await advance(MOCK_FREE_CREATE_MS + 100);
    await user.click(await within(box).findByRole('button', { name: 'Copy link' }));
    expect(await navigator.clipboard.readText()).toBe(MOCK_FREE_INVITE);
    expect(await within(box).findByText('Link copied')).toBeInTheDocument();
  });

  it('shows the address when the server has no invite link', async () => {
    const app = renderWith((cmd) =>
      cmd === 'free_server_create' ? Promise.resolve({ ...fakeCard, inviteUrl: null, invite: null }) : undefined,
    );
    const { user } = app;
    await user.click(await screen.findByRole('button', { name: ENTRY }));
    const box = await dialog();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByText('play.example.com:25590')).toBeInTheDocument();
    expect(within(box).queryByRole('button', { name: 'Copy link' })).toBeNull();
    await user.click(within(box).getByRole('button', { name: 'Copy address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25590');
  });

  it('play_now_calls_free_server_play_then_play', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { user, box, calls } = await open();
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    await advance(MOCK_FREE_CREATE_MS + 100);
    await user.click(await within(box).findByRole('button', { name: 'Play now' }));
    await vi.waitFor(() => expect(sent(calls, 'play')).toHaveLength(1));
    const order = calls.map(([c]) => c).filter((c) => c === 'free_server_play' || c === 'play');
    expect(order).toEqual(['free_server_play', 'play']);
    const { id, join } = sent(calls, 'play')[0] as {
      id: string;
      join: boolean;
    };
    expect(join).toBe(true);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id }));
  });

  it('creates in Spanish when the launcher speaks Spanish', async () => {
    const i18n = (await import('../../i18n')).default;
    await i18n.changeLanguage('es');
    try {
      const app = renderApp(<ServersScreen />, {
        perland: 'signedIn',
        free: 'none',
      });
      await app.user.click(
        await screen.findByRole('button', {
          name: 'Crear un servidor para tus amigos',
        }),
      );
      const box = await dialog();
      await app.user.click(within(box).getByRole('button', { name: 'Crear' }));
      await vi.waitFor(() => expect(sent(app.calls, 'free_server_create')).toHaveLength(1));
      expect((sent(app.calls, 'free_server_create')[0] as { language: string }).language).toBe('es');
    } finally {
      await i18n.changeLanguage('en');
    }
  });
});

/** A finished free server as the page holds it. */
const fakeCard = {
  token: 'Z'.repeat(43),
  invite: MOCK_FREE_INVITE,
  joinable: true,
  address: 'play.example.com:25590',
  name: 'Steve',
  iconVersion: null,
  game: { version: '1.21.1', loader: null, loaderVersion: null },
  modded: false,
  status: 'starting' as const,
  players: null,
  whitelisted: false,
  pack: null,
  categories: [],
  language: 'en',
  region: null,
  instance: null,
  kind: 'vanilla',
  memoryMb: MOCK_FREE_LIMITS.memoryMb.vanilla,
  maxPlayers: MOCK_FREE_LIMITS.maxPlayers,
  freeStatus: 'starting',
  deletion: null,
  disk: {
    usedBytes: 0,
    limitBytes: MOCK_FREE_LIMITS.diskBytes,
    checkedAt: null,
    over: false,
  },
  inviteUrl: MOCK_FREE_INVITE as string | null,
  joinUrl: null,
};
