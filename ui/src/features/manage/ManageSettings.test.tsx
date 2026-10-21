import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { MOCK_TEAM_SERVER, MOCK_TEAM_TENANT } from '../../ipc/mockManage';
import type { ManageSettingsSave } from '../../ipc/schema';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ManageSettings } from './ManageSettings';

const ref = { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER };
const show = (manage: MockOptions['manage']) => renderApp(<ManageSettings {...ref} />, { perland: 'signedIn', manage });
const saves = (calls: [string, unknown][]) => sent(calls, 'manage_settings_save').map((a) => (a as { save: ManageSettingsSave }).save);
const refusal = (code: string, params: Record<string, string> = {}) => ({ category: 'network', code, message: code, fix: null, detail: null, params });

/** The mock, with the first save answered by `first` instead. */
function withFirstSave(manage: MockOptions['manage'], first: unknown) {
  const backend = createMockBackend({ perland: 'signedIn', manage });
  const calls: [string, unknown][] = [];
  let refused = false;
  const result = renderWithProviders(<ManageSettings {...ref} />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'manage_settings_save' && !refused) {
        refused = true;
        return Promise.reject(first);
      }
      return backend.handle(cmd, args);
    },
  });
  return { ...result, calls };
}

describe('ManageSettings', () => {
  it('a field the plan locks shows locked, with Upgrade', async () => {
    const { user, calls } = show('locked');
    const max = await screen.findByRole('spinbutton', { name: 'Max players' });
    await user.clear(max);
    await user.type(max, '20');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText("Your plan doesn't include changing this.")).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Upgrade' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_open_panel')).toEqual([{ tenantId: ref.tenantId, stack: null }]));
  });

  it('a server-list name with colours is read only here', async () => {
    const { user, calls } = show('motdCodes');
    await screen.findByRole('combobox', { name: 'Game mode' });
    expect(screen.queryByRole('textbox', { name: 'Server-list name' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Edit in Perland' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_open_panel')).toEqual([ref]));
  });

  it('a modpack server has no version to pick', async () => {
    show('modpack');
    await screen.findByRole('combobox', { name: 'Game mode' });
    expect(screen.queryByRole('textbox', { name: 'Minecraft version' })).toBeNull();
  });

  it('a version change asks first, then sends it again confirmed', async () => {
    const { user, calls } = withFirstSave('stopped', refusal('version_change_needs_confirmation'));
    const version = await screen.findByRole('textbox', { name: 'Minecraft version' });
    await user.clear(version);
    await user.type(version, '1.21.4');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    const dialog = await screen.findByRole('dialog', { name: 'Change the Minecraft version?' });
    await user.click(within(dialog).getByRole('button', { name: 'Change version' }));
    await vi.waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[0]?.confirmVersionChange).toBeUndefined();
    expect(saves(calls)[1]).toMatchObject({ version: '1.21.4', confirmVersionChange: true });
  });

  it('a change that can lose world data asks louder and sends both confirmations', async () => {
    const { user, calls } = withFirstSave('stopped', refusal('data_loss_needs_confirmation'));
    const version = await screen.findByRole('textbox', { name: 'Minecraft version' });
    await user.clear(version);
    await user.type(version, '1.20.1');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    const dialog = await screen.findByRole('dialog', { name: 'This can erase parts of your world' });
    await user.click(within(dialog).getByRole('button', { name: 'Change it anyway' }));
    await vi.waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[1]).toMatchObject({ confirmVersionChange: true, acknowledgeDataLoss: true });
  });

  it('while running, the player picks when the change applies', async () => {
    const { user, calls } = show('running');
    await user.selectOptions(await screen.findByRole('combobox', { name: 'Difficulty' }), 'hard');
    await user.click(screen.getByRole('radio', { name: 'Apply at next restart' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0]).toMatchObject({ difficulty: 'hard', apply: 'next_restart', motd: null, version: null });
    expect(await screen.findByText('Saved. It applies at the next restart.')).toBeInTheDocument();
  });

  it('while it starts, a save waits for the next start', async () => {
    const { user, calls } = show('starting');
    await user.selectOptions(await screen.findByRole('combobox', { name: 'Difficulty' }), 'easy');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0]?.apply).toBe('next_restart');
  });

  it('says when max players is out of range, and saves nothing', async () => {
    const { user, calls } = show('stopped');
    const max = await screen.findByRole('spinbutton', { name: 'Max players' });
    await user.clear(max);
    await user.type(max, '0');
    expect(screen.getByText('Pick a number from 1 to 1000.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(saves(calls)).toEqual([]);
  });

  it('a stopped server just saves', async () => {
    show('stopped');
    await screen.findByRole('combobox', { name: 'Game mode' });
    expect(screen.queryByRole('radio', { name: 'Apply at next restart' })).toBeNull();
  });

  it('settings someone else changed are loaded again', async () => {
    const { user, calls } = withFirstSave('stopped', refusal('settings_changed'));
    await user.selectOptions(await screen.findByRole('combobox', { name: 'Game mode' }), 'creative');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Someone changed these settings. We loaded the new ones.')).toBeInTheDocument();
    await vi.waitFor(() => expect(sent(calls, 'manage_settings').length).toBeGreaterThan(1));
  });
});
