import { act, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MOCK_INVITE, MOCK_PUBLIC_SERVERS } from '../../ipc/mockServers';
import packStatusFx from '../../ipc/contract/pack_status.json';
import type { PackStatus, PackUpdate } from '../../ipc/schema';
import { runStore } from '../../state/runStore';
import { routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ChangeSummary } from '../home/ChangeSummary';
import { Home } from '../home/Home';
import { InstanceSettings } from '../instance-settings/InstanceSettings';
import { JoinServer } from '../join/JoinServer';
import { ServersScreen } from '../servers-list/ServersScreen';
import { TrustDialog } from './TrustDialog';

const OTHER = `https://play.example.com/${'B'.repeat(43)}`;
const BANNED = /\b(safe|clean|verified|virus)/i;

describe('the trust step on Join server', () => {
  it('says who runs the server, what installs and where it comes from, before anything is added', async () => {
    const { user, calls } = renderApp(<JoinServer invite={OTHER} />, { server: 'online' });
    const step = (await screen.findByRole('heading', { name: 'Before you join survival' })).closest('section')!;
    expect(within(step).getByText('Run by Acme')).toBeInTheDocument();
    expect(within(step).getByText('Joining installs Survival Pack on your computer: 142 files.')).toBeInTheDocument();
    const sources = within(step).getByRole('region', { name: 'Where they come from' });
    const row = (label: string) => within(sources).getByText(label).closest('div')!;
    expect(row('From Modrinth')).toHaveTextContent('104');
    expect(row('From CurseForge')).toHaveTextContent('34');
    expect(row('From other websites the server\'s owner chose')).toHaveTextContent('1');
    expect(row("Uploaded by the server's owner")).toHaveTextContent('3');
    expect(row("Uploaded by the server's owner")).toHaveAttribute('data-callout');
    expect(within(step).getByText(/Mods are programs: they run on your computer with the same access you have/)).toBeInTheDocument();
    expect(step.textContent).not.toMatch(BANNED);
    expect(sent(calls, 'join_add')).toEqual([]);
    // The one choice: the pack updates on its own unless the player says otherwise.
    const auto = within(step).getByRole('switch', { name: "Update this server's pack automatically" });
    expect(auto).toBeChecked();
    await user.click(auto);
    await user.click(screen.getByRole('button', { name: 'Trust and add' }));
    expect(sent(calls, 'join_add')).toEqual([{ link: OTHER, autoUpdate: false }]);
  });

  it('Cancel is the way out and installs nothing', async () => {
    const { user, calls } = renderApp(<JoinServer invite={OTHER} />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Before you join survival' });
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(routeSnapshot()).toEqual({ name: 'home', id: null });
    expect(sent(calls, 'join_add')).toEqual([]);
    expect(sent(calls, 'play')).toEqual([]);
  });

  it('a vanilla server needs no step', async () => {
    const { user, calls } = renderApp(<JoinServer invite={OTHER} />, { server: 'vanilla' });
    await user.click(await screen.findByRole('button', { name: 'Add server' }));
    expect(screen.queryByRole('heading', { name: /Before you join/ })).toBeNull();
    expect(sent(calls, 'join_add')).toEqual([{ link: OTHER, autoUpdate: null }]);
  });

  it('a server the instance already trusts opens without asking', async () => {
    const { user, calls } = renderApp(<JoinServer invite={MOCK_INVITE} />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Open' }));
    expect(screen.queryByRole('heading', { name: /Before you join/ })).toBeNull();
    expect(sent(calls, 'join_add')).toEqual([]);
  });

  it('an instance joined before the step is trusted when it is opened here', async () => {
    const { user, calls } = renderApp(<JoinServer invite={MOCK_INVITE} />, { server: 'untrusted' });
    await user.click(await screen.findByRole('button', { name: 'Trust and open' }));
    expect(sent(calls, 'join_add')).toEqual([{ link: MOCK_INVITE, autoUpdate: true }]);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
  });
});

describe('the trust step as a dialog', () => {
  const survival = MOCK_PUBLIC_SERVERS.find((c) => c.pack !== null)!;

  async function playCard(user: ReturnType<typeof renderApp>['user'], name: string) {
    const heading = await screen.findByRole('heading', { name });
    const card = heading.closest('article')!;
    await user.click(within(card).getByRole('button', { name: 'Play' }));
  }

  it("a server list's Play asks before anything is added; Not now leaves nothing behind", async () => {
    const { user, calls } = renderApp(
      <>
        <ServersScreen />
        <TrustDialog />
      </>,
    );
    await playCard(user, survival.displayName);
    const dialog = await screen.findByRole('dialog', { name: `Before you join ${survival.displayName}` });
    expect(within(dialog).getByText(/Mods are programs/)).toBeInTheDocument();
    expect(sent(calls, 'join_trust')).toEqual([{ link: survival.invite, instance: null }]);
    await user.click(within(dialog).getByRole('button', { name: 'Not now' }));
    await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(sent(calls, 'join_add')).toEqual([]);
    expect(sent(calls, 'play')).toEqual([]);
  });

  it("a server list's Play goes on once the player trusts the server", async () => {
    const { user, calls } = renderApp(
      <>
        <ServersScreen />
        <TrustDialog />
      </>,
    );
    await playCard(user, survival.displayName);
    const dialog = await screen.findByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: 'Trust and play' }));
    await vi.waitFor(() => expect(sent(calls, 'play')).toHaveLength(1));
    expect(sent(calls, 'join_add')).toEqual([{ link: survival.invite, autoUpdate: true }]);
    const order = calls.map(([cmd]) => cmd).filter((cmd) => ['join_trust', 'join_add', 'play'].includes(cmd));
    expect(order).toEqual(['join_trust', 'join_add', 'play']);
  });

  it('a Play refused for an untrusted server shows the step, then plays', async () => {
    const { user, calls } = renderApp(
      <>
        <Home id="survival" />
        <TrustDialog />
      </>,
      { server: 'untrusted' },
    );
    await user.click(await screen.findByRole('button', { name: 'Play' }));
    const dialog = await screen.findByRole('dialog', { name: 'Before you join survival' });
    // The refusal is the step, never a toast.
    expect(toastsSnapshot()).toEqual([]);
    await user.click(within(dialog).getByRole('switch', { name: "Update this server's pack automatically" }));
    await user.click(within(dialog).getByRole('button', { name: 'Trust and play' }));
    await vi.waitFor(() => expect(sent(calls, 'play')).toHaveLength(2));
    expect(sent(calls, 'instance_trust_set')).toEqual([{ id: 'survival', autoUpdate: false }]);
    expect(sent(calls, 'play')[1]).toEqual({ id: 'survival', join: false });
  });
});

describe('the update summary', () => {
  const update: PackUpdate = {
    fromVersion: '1.0.3',
    toVersion: '1.0.4',
    added: ['Create', 'extras.jar'],
    updated: ['Sodium', 'Tool'],
    removed: [],
    kept: [],
    files: [
      { name: 'Create', change: 'added', origin: 'modrinth', host: null, needsLook: false },
      { name: 'extras.jar', change: 'added', origin: 'uploaded', host: null, needsLook: true },
      { name: 'Sodium', change: 'updated', origin: 'curseForge', host: null, needsLook: false },
      { name: 'Tool', change: 'updated', origin: 'web', host: 'github.com', needsLook: true },
    ],
    held: true,
  };

  it('calls out the files from outside Modrinth and CurseForge and says why it waited', () => {
    renderWithProviders(<ChangeSummary update={update} server />);
    expect(screen.getByText("This update waits for you: it adds or changes files that don't come from Modrinth or CurseForge.")).toBeInTheDocument();
    expect(screen.getByText('2 files from Modrinth or CurseForge')).toBeInTheDocument();
    const look = screen.getByRole('region', { name: '2 files not from Modrinth or CurseForge' });
    const items = within(look).getAllByRole('listitem');
    expect(items[0]).toHaveTextContent("extras.jarUploaded by the server's owner · New");
    expect(items[1]).toHaveTextContent('ToolFrom github.com · Updated');
    expect(look.textContent).not.toMatch(BANNED);
  });

  it('a plain update shows no callout', () => {
    renderWithProviders(<ChangeSummary update={{ ...update, held: false, files: [update.files[0]!] }} />);
    expect(screen.queryByRole('region')).toBeNull();
    expect(screen.queryByText(/waits for you/)).toBeNull();
  });

  it('a server update that waits shows its files on Home before anything installs', async () => {
    const { calls } = renderApp(<Home id="survival" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Survival' });
    const offer = (packStatusFx as PackStatus[])[0]!.update!;
    act(() => runStore.apply({ type: 'run_state', instance: 'survival', state: 'asking', exit: null, update: offer }));
    const dialog = screen.getByRole('dialog', { name: 'Update ready: 1.0.3 → 1.0.4' });
    expect(within(dialog).getByText(/This update waits for you/)).toBeInTheDocument();
    expect(within(dialog).getByText('survival-extras.jar')).toBeInTheDocument();
    expect(within(dialog).getByText("Uploaded by the server's owner · Updated")).toBeInTheDocument();
    expect(within(dialog).getByText("Inside the server's pack file · Updated")).toBeInTheDocument();
    expect(sent(calls, 'play_answer')).toEqual([]);
  });
});

describe('the trust setting', () => {
  it('a trusted server can be taken back, and the pack can ask before updating', async () => {
    const { user, calls } = renderApp(<InstanceSettings id="survival" />, { server: 'online' });
    expect(await screen.findByText('You trust this server, so its modpack installs on this computer.')).toBeInTheDocument();
    const auto = screen.getByRole('switch', { name: "Update this server's pack automatically" });
    expect(auto).toBeChecked();
    await user.click(auto);
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => expect(sent(calls, 'instance_update')).toHaveLength(1));
    expect((sent(calls, 'instance_update')[0] as { instance: { updatePolicy: string } }).instance.updatePolicy).toBe('ask');

    await user.click(screen.getByRole('button', { name: 'Stop trusting this server' }));
    expect(
      await screen.findByText('Next time you play, the launcher asks again before installing anything from this server. Files already installed stay.'),
    ).toBeInTheDocument();
    expect(sent(calls, 'instance_trust_forget')).toEqual([{ id: 'survival' }]);
  });

  it('an untrusted server says the launcher will ask', async () => {
    renderApp(<InstanceSettings id="survival" />, { server: 'untrusted' });
    expect(
      await screen.findByText("You haven't said you trust this server yet. The launcher asks before installing anything from it."),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Stop trusting this server' })).toBeNull();
  });
});
