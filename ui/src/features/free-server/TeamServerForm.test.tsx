import { screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { MOCK_TEAM_TENANT } from '../../ipc/mockManage';
import { MOCK_FULL_TENANT, MOCK_STARTER_TENANT } from '../../ipc/mockTeams';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders, type IpcHandler } from '../../test/render';
import { Home } from '../home/Home';
import { ServersScreen } from '../servers-list/ServersScreen';

const TEAMS: MockOptions = { perland: 'signedIn', free: 'none', teams: 'some' };

afterEach(async () => {
  await i18n.changeLanguage('en');
});

async function open(options: MockOptions = TEAMS) {
  const app = renderApp(<ServersScreen />, options);
  await app.user.click(await screen.findByRole('button', { name: 'Create a server' }));
  const box = await screen.findByRole('dialog', { name: 'Create a server' });
  return { ...app, box };
}

/** The team form, once Where names `team` and its versions have loaded. */
async function onTeam(app: Awaited<ReturnType<typeof open>>, team: string) {
  await app.user.click(within(app.box).getByRole('radio', { name: team }));
  await within(app.box).findByRole('combobox', { name: 'Memory' });
  return app;
}

/** The dialog's text without its Where choice (whose free server segment names the free server). */
const formText = (box: HTMLElement) => {
  const copy = box.cloneNode(true) as HTMLElement;
  copy.querySelectorAll('fieldset').forEach((f) => {
    if (f.querySelector('input[name="server-where"]')) f.remove();
  });
  return copy.textContent ?? '';
};

describe('Create a server with teams', () => {
  it('keeps today\'s entry without teams', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'none' });
    expect(await screen.findByRole('button', { name: 'Create a server for friends' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Create a server' })).toBeNull();
  });

  it('asks Where first: the free server, then each team the player may create on', async () => {
    const { box, calls } = await open();
    const where = within(box).getByRole('group', { name: 'Where' });
    expect(within(where).getAllByRole('radio').map((r) => r.closest('label')?.textContent)).toEqual([
      'Your free server',
      'Builders',
      'Night Shift',
      'Starter Club',
      'Weekend Crew',
    ]);
    expect(within(where).getByRole('radio', { name: 'Your free server' })).toBeChecked();
    expect(sent(calls, 'creatable_teams')).toHaveLength(1);
  });

  it('offers only teams once the free server exists', async () => {
    const { box } = await open({ ...TEAMS, free: 'has' });
    const where = within(box).getByRole('group', { name: 'Where' });
    expect(within(where).queryByRole('radio', { name: 'Your free server' })).toBeNull();
    expect(within(where).getByRole('radio', { name: 'Builders' })).toBeChecked();
  });

  it('never says free on a team', async () => {
    const app = await onTeam(await open(), 'Builders');
    expect(formText(app.box)).not.toMatch(/free/i);
    await app.user.click(within(app.box).getByRole('button', { name: /^Options/ }));
    expect(formText(app.box)).not.toMatch(/free/i);
  });

  it('bounds memory by what the team has, starting at 4 GB', async () => {
    const { box } = await onTeam(await open(), 'Builders');
    const memory = within(box).getByRole('combobox', { name: 'Memory' });
    expect(within(memory).getAllByRole('option').map((o) => o.textContent)).toEqual(['1 GB', '2 GB', '3 GB', '4 GB', '5 GB', '6 GB']);
    expect(memory).toHaveValue('4');
  });

  it('creates from type, version and loader, then leads to Manage', async () => {
    const app = await onTeam(await open(), 'Builders');
    const { box, user, calls } = app;
    await user.click(within(box).getByRole('radio', { name: 'Forge' }));
    await within(box).findByRole('option', { name: '52.1.0 (recommended)' });
    expect(within(box).getByRole('combobox', { name: 'Loader version' })).toHaveValue('52.1.0');
    await user.selectOptions(within(box).getByRole('combobox', { name: 'Memory' }), '6');
    await user.clear(within(box).getByRole('textbox', { name: 'Server name' }));
    await user.type(within(box).getByRole('textbox', { name: 'Server name' }), 'Forge-Fun');
    await user.click(within(box).getByRole('button', { name: /^Options/ }));
    await user.click(within(box).getByRole('radio', { name: 'Creative' }));
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await screen.findByRole('heading', { name: 'Your server is ready' }, { timeout: 4000 })).toBeInTheDocument();
    expect(sent(calls, 'team_create')).toEqual([
      {
        request: {
          tenantId: MOCK_TEAM_TENANT,
          name: 'Forge-Fun',
          type: 'forge',
          version: '1.21.1',
          loaderVersion: '52.1.0',
          memoryGb: 6,
          settings: { gameMode: 'creative' },
        },
      },
    ]);
    expect(within(box).getByText('Your server is running. Open Manage to change its settings and add players.')).toBeInTheDocument();
    await user.click(within(box).getByRole('button', { name: 'Manage' }));
    expect(routeSnapshot()).toEqual({ name: 'manage', tenantId: MOCK_TEAM_TENANT, stack: 'Forge-Fun' });
  });

  it('sends max players only when changed from 20', async () => {
    const { box, user, calls } = await onTeam(await open(), 'Builders');
    await user.click(within(box).getByRole('button', { name: /^Options/ }));
    expect(within(box).getByRole('spinbutton', { name: 'Max players' })).toHaveValue(20);
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    await screen.findByRole('heading', { name: 'Your server is ready' }, { timeout: 4000 });
    expect(sent(calls, 'team_create')).toMatchObject([{ request: { tenantId: MOCK_TEAM_TENANT, type: 'vanilla', settings: null, loaderVersion: null } }]);
  });

  it('says plainly when the team has no room to run the new server', async () => {
    const { box, user } = await onTeam(await open(), 'Night Shift');
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(
      await screen.findByText('Your server is ready, but your team has no room to run it now. Stop another server to start it.', undefined, {
        timeout: 4000,
      }),
    ).toBeInTheDocument();
    expect(within(box).getByRole('button', { name: 'Manage' })).toBeInTheDocument();
  });

  it('shows a field the plan locks with Upgrade, and lets another choice through', async () => {
    const { box, user, calls } = await onTeam(await open(), 'Starter Club');
    await user.click(within(box).getByRole('radio', { name: 'Fabric' }));
    await within(box).findByRole('combobox', { name: 'Loader version' });
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByText("Your team's plan doesn't include this.", undefined, { timeout: 4000 })).toBeInTheDocument();
    await user.click(within(box).getByRole('button', { name: 'Upgrade' }));
    expect(sent(calls, 'manage_open_panel')).toEqual([{ tenantId: MOCK_STARTER_TENANT, stack: null }]);
    // Another type is a way out: the lock goes with the change.
    await user.click(within(box).getByRole('radio', { name: 'Vanilla' }));
    expect(within(box).queryByText("Your team's plan doesn't include this.")).toBeNull();
  });

  it('says plainly when the team can make no more servers, and blocks Create', async () => {
    const { box, calls } = await onTeam(await open(), 'Weekend Crew');
    expect(within(box).getByText("Your team's plan has no room for another server.")).toBeInTheDocument();
    expect(within(box).getByRole('button', { name: 'Create' })).toBeDisabled();
    expect(sent(calls, 'team_create')).toEqual([]);
  });

  it('reads a size refusal as the server count when the team has no servers left', async () => {
    const backend = createMockBackend(TEAMS);
    let rooms = 0;
    const ipc: IpcHandler = (cmd, args) => {
      if (cmd === 'team_room') {
        rooms += 1;
        return { memoryGb: [1, 2, 3, 4], defaultGb: 4, cpu: 2, serversLeft: rooms === 1 ? 1 : 0 };
      }
      if (cmd === 'team_create') {
        return Promise.reject({ category: 'network', code: 'team_server_too_big', message: 'x', fix: null, detail: null, params: {} });
      }
      return backend.handle(cmd, args);
    };
    const { user } = renderWithProviders(<ServersScreen />, { ipc });
    await user.click(await screen.findByRole('button', { name: 'Create a server' }));
    const box = await screen.findByRole('dialog', { name: 'Create a server' });
    await user.click(within(box).getByRole('radio', { name: 'Builders' }));
    await within(box).findByRole('combobox', { name: 'Memory' });
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await within(box).findByText("Your team's plan has no room for another server.")).toBeInTheDocument();
    expect(within(box).queryByText("Your team's plan has no room for a server this size.")).toBeNull();
    expect(within(box).getByRole('combobox', { name: 'Memory' })).not.toHaveAttribute('aria-invalid');
  });

  it('puts a refusal under its field, in a team\'s words', async () => {
    const backend = createMockBackend(TEAMS);
    const calls: [string, unknown][] = [];
    const ipc: IpcHandler = (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'team_create') {
        return Promise.reject({ category: 'network', code: 'team_server_too_big', message: 'x', fix: null, detail: null, params: {} });
      }
      return backend.handle(cmd, args);
    };
    const { user } = renderWithProviders(<ServersScreen />, { ipc });
    await user.click(await screen.findByRole('button', { name: 'Create a server' }));
    const box = await screen.findByRole('dialog', { name: 'Create a server' });
    await user.click(within(box).getByRole('radio', { name: 'Builders' }));
    await within(box).findByRole('combobox', { name: 'Memory' });
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    const memory = await within(box).findByRole('combobox', { name: 'Memory' });
    expect(await within(box).findByText("Your team's plan has no room for a server this size.")).toBeInTheDocument();
    expect(memory).toHaveAttribute('aria-invalid', 'true');
  });

  it('reads in Spanish', async () => {
    await i18n.changeLanguage('es');
    const app = renderApp(<ServersScreen />, TEAMS);
    await app.user.click(await screen.findByRole('button', { name: 'Crear un servidor' }));
    const box = await screen.findByRole('dialog', { name: 'Crear un servidor' });
    expect(within(box).getByRole('group', { name: 'Dónde' })).toBeInTheDocument();
    await app.user.click(within(box).getByRole('radio', { name: 'Builders' }));
    expect(await within(box).findByRole('combobox', { name: 'Memoria' })).toBeInTheDocument();
    expect(formText(box)).not.toMatch(/gratis/i);
  });
});

describe('Play this with friends on a team', () => {
  async function friends(options: MockOptions, id = 'asalith-fields', name = 'Asalith Fields') {
    const app = renderApp(<Home id={id} />, options);
    await screen.findByRole('heading', { name });
    await app.user.click(screen.getByRole('button', { name: 'More options' }));
    await app.user.click(await screen.findByRole('menuitem', { name: 'Play this with friends…' }));
    return { ...app, box: screen.getByRole('dialog', { name: `Play ${name} with friends` }) };
  }

  it('is offered for teams alone once the free server exists', async () => {
    const { box } = await friends({ ...TEAMS, free: 'has' });
    expect(within(box).getByRole('radio', { name: 'Builders' })).toBeChecked();
    expect(within(box).queryByRole('radio', { name: 'Your free server' })).toBeNull();
  });

  it('checks the mods against the memory picked and creates from the instance', async () => {
    const { box, user, calls } = await friends(TEAMS);
    await user.click(within(box).getByRole('radio', { name: 'Builders' }));
    expect(await within(box).findByText('Needs 2.5 GB of memory.')).toBeInTheDocument();
    expect(within(box).getByText('The server runs these mods and settings.')).toBeInTheDocument();
    expect(formText(box)).not.toMatch(/free/i);
    const memory = within(box).getByRole('combobox', { name: 'Memory' });
    await user.selectOptions(memory, '2');
    expect(within(box).getByText('Your mods need 2.5 GB of memory. Pick at least that much.')).toBeInTheDocument();
    expect(within(box).getByRole('button', { name: 'Create' })).toBeDisabled();
    await user.selectOptions(memory, '3');
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await screen.findByRole('heading', { name: 'Your server is ready' }, { timeout: 4000 })).toBeInTheDocument();
    expect(sent(calls, 'team_create_from_instance')).toEqual([
      { request: { tenantId: MOCK_TEAM_TENANT, id: 'asalith-fields', name: 'Asalith-Fields', memoryGb: 3, settings: null } },
    ]);
    expect(sent(calls, 'friends_create')).toEqual([]);
  });

  it('drops the free server\'s refusal when Where changes', async () => {
    const backend = createMockBackend(TEAMS);
    const ipc: IpcHandler = (cmd, args) =>
      cmd === 'friends_create'
        ? Promise.reject({ category: 'network', code: 'host_full', message: 'x', fix: null, detail: null, params: {} })
        : backend.handle(cmd, args);
    const { user } = renderWithProviders(<Home id="creative-flat" />, { ipc });
    await screen.findByRole('heading', { name: 'Creative Flat' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Play this with friends…' }));
    const box = screen.getByRole('dialog', { name: 'Play Creative Flat with friends' });
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    const line = 'Perland is full right now. Try again in a few minutes.';
    expect(await within(box).findByText(line)).toBeInTheDocument();
    await user.click(within(box).getByRole('radio', { name: 'Builders' }));
    await user.click(within(box).getByRole('radio', { name: 'Your free server' }));
    expect(within(box).queryByText(line)).toBeNull();
  });

  it('says a vanilla instance runs its version on the team with no room', async () => {
    const { box, user, calls } = await friends(TEAMS, 'creative-flat', 'Creative Flat');
    await user.click(within(box).getByRole('radio', { name: 'Night Shift' }));
    expect(await within(box).findByText(/^Runs Minecraft .*, the same as this instance\./)).toBeInTheDocument();
    await within(box).findByRole('combobox', { name: 'Memory' });
    await user.click(within(box).getByRole('button', { name: 'Create' }));
    expect(await screen.findByText(/your team has no room to run it now/, undefined, { timeout: 4000 })).toBeInTheDocument();
    expect(sent(calls, 'team_create_from_instance')).toMatchObject([{ request: { tenantId: MOCK_FULL_TENANT, id: 'creative-flat' } }]);
  });
});
